//! The platform's side: a `RemoteRuntime` that implements the same `InstanceRuntime` as the
//! local Docker runtime, by calling a lab host's API. The platform treats it like any runtime.
//!
//! The connection is HTTPS. A lab host made with the installer has a self-signed certificate,
//! so the platform pins its SHA-256 fingerprint (shown by `kymo-lab fingerprint`) instead of
//! trusting a certificate authority: no domain name or proxy is needed, and nobody else can
//! stand in for the lab host. A lab host behind a proxy with an ordinary certificate needs no
//! fingerprint.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kymo_instances::{
    InstanceRuntime, Managed, Placement, RuntimeError, RuntimeStatus, StartRequest, Started,
};
use reqwest::{Method, StatusCode};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::protocol::{
    ArtifactInfo, CreateRequest, ErrorBody, HealthInfo, InstanceInfo, LogLine, ReadyInfo,
    ReconcileRequest, Removed,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Parses a fingerprint written as hex, with or without colons or spaces, any case.
pub fn parse_fingerprint(text: &str) -> Result<[u8; 32], String> {
    let hex: String = text
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-'))
        .collect();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("a fingerprint is 64 hexadecimal characters (SHA-256)".into());
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// The fingerprint of a DER certificate, as `AB:CD:...`.
pub fn fingerprint_of(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Accepts exactly one certificate, identified by its fingerprint. The handshake signatures are
/// still verified, so the peer must hold the private key.
#[derive(Debug)]
struct Pinned {
    fingerprint: [u8; 32],
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let seen: [u8; 32] = Sha256::digest(end_entity.as_ref()).into();
        if subtle::ConstantTimeEq::ct_eq(&seen[..], &self.fingerprint[..]).into() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "the lab host's certificate does not match the pinned fingerprint".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// A lab host seen as an `InstanceRuntime`.
pub struct RemoteRuntime {
    http: reqwest::Client,
    base: String,
    token: String,
    /// The address players are told to connect to: the lab host's public name or IP.
    public_host: String,
}

impl RemoteRuntime {
    /// `url` is the lab API root (`https://lab.example.org:7400`). With a `fingerprint` the
    /// connection trusts that one certificate; without, an `https` URL uses the usual
    /// certificate authorities and an `http` URL is plain text (only for a trusted network).
    pub fn new(
        url: &str,
        token: &str,
        fingerprint: Option<&str>,
        public_host: &str,
    ) -> Result<Self, String> {
        let base = url.trim().trim_end_matches('/').to_owned();
        if !(base.starts_with("https://") || base.starts_with("http://")) {
            return Err("the lab URL must start with https:// or http://".into());
        }
        let mut builder = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("kymo/", env!("CARGO_PKG_VERSION")));
        if let Some(text) = fingerprint.filter(|f| !f.trim().is_empty()) {
            if !base.starts_with("https://") {
                return Err("a fingerprint only makes sense with an https:// URL".into());
            }
            let provider: Arc<CryptoProvider> = Arc::new(rustls::crypto::ring::default_provider());
            let verifier = Pinned {
                fingerprint: parse_fingerprint(text)?,
                algorithms: provider.signature_verification_algorithms,
            };
            let config = rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map_err(|e| e.to_string())?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(verifier))
                .with_no_client_auth();
            builder = builder.use_preconfigured_tls(config);
        }
        Ok(Self {
            http: builder.build().map_err(|e| e.to_string())?,
            base,
            token: token.to_owned(),
            public_host: public_host.trim().to_owned(),
        })
    }

    pub fn public_host(&self) -> &str {
        &self.public_host
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&impl serde::Serialize>,
    ) -> Result<T, RuntimeError> {
        let response = self.call(method, path, body).await?;
        response.json::<T>().await.map_err(|e| {
            RuntimeError::Failed(format!("the lab host sent something unexpected: {e}"))
        })
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&impl serde::Serialize>,
    ) -> Result<reqwest::Response, RuntimeError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| RuntimeError::Unavailable(describe(&e)))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let message = response
            .json::<ErrorBody>()
            .await
            .map(|b| b.error)
            .unwrap_or_else(|_| status.to_string());
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                RuntimeError::Unavailable(format!("the lab host refused this platform: {message}"))
            }
            s if s.is_server_error() || s == StatusCode::TOO_MANY_REQUESTS => {
                RuntimeError::Unavailable(message)
            }
            _ => RuntimeError::Failed(message),
        })
    }

    pub async fn health(&self) -> Result<HealthInfo, RuntimeError> {
        self.send(Method::GET, "/v1/health", None::<&()>).await
    }

    pub async fn artifacts(&self) -> Result<Vec<ArtifactInfo>, RuntimeError> {
        self.send(Method::GET, "/v1/artifacts", None::<&()>).await
    }

    pub async fn log(&self) -> Result<Vec<LogLine>, RuntimeError> {
        self.send(Method::GET, "/v1/log", None::<&()>).await
    }

    /// A virtual machine's console as a PNG.
    pub async fn screen(&self, name: &str) -> Result<Vec<u8>, RuntimeError> {
        let response = self
            .call(
                Method::GET,
                &format!("/v1/instances/{}/screen", urlencode(name)),
                None::<&()>,
            )
            .await?;
        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))
    }

    pub async fn instances(&self) -> Result<Vec<InstanceInfo>, RuntimeError> {
        self.send(Method::GET, "/v1/instances", None::<&()>).await
    }

    /// Asks the lab host to remove everything the platform does not list. Returns the names.
    pub async fn reconcile(
        &self,
        keep: Vec<String>,
        grace_secs: Option<i64>,
    ) -> Result<Vec<String>, RuntimeError> {
        let removed: Removed = self
            .send(
                Method::POST,
                "/v1/reconcile",
                Some(&ReconcileRequest { keep, grace_secs }),
            )
            .await?;
        Ok(removed.removed)
    }
}

/// A connection error in words an operator can act on.
fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "the lab host did not answer in time".into()
    } else if e.is_connect() {
        format!("cannot connect to the lab host ({})", root_cause(e))
    } else {
        root_cause(e)
    }
}

fn root_cause(e: &dyn std::error::Error) -> String {
    let mut cause: &dyn std::error::Error = e;
    while let Some(next) = cause.source() {
        cause = next;
    }
    cause.to_string()
}

#[async_trait]
impl InstanceRuntime for RemoteRuntime {
    async fn start(&self, request: &StartRequest) -> Result<Started, RuntimeError> {
        if request.placement != Placement::Published {
            return Err(RuntimeError::Failed(
                "private VPN networks are not available on a remote lab host".into(),
            ));
        }
        let name = request.name.clone().ok_or_else(|| {
            RuntimeError::Failed("a remote start needs a name built by the platform".into())
        })?;
        let body = CreateRequest {
            name,
            instance_id: request.instance_id.clone(),
            spec: request.spec.clone(),
            meta: request.meta.iter().cloned().collect(),
        };
        let info: InstanceInfo = self
            .send(Method::POST, "/v1/instances", Some(&body))
            .await?;
        let mut started = Started::new(info.id, info.host_port, None);
        started.extra_ports = info.ports;
        started.ready_secs = info.ready_secs;
        Ok(started)
    }

    async fn stop(&self, handle: &str) -> Result<(), RuntimeError> {
        self.call(
            Method::DELETE,
            &format!("/v1/instances/{}", urlencode(handle)),
            None::<&()>,
        )
        .await
        .map(|_| ())
    }

    async fn status(&self, handle: &str) -> Result<RuntimeStatus, RuntimeError> {
        match self
            .send::<InstanceInfo>(
                Method::GET,
                &format!("/v1/instances/{}", urlencode(handle)),
                None::<&()>,
            )
            .await
        {
            Ok(info) => Ok(match info.status.as_str() {
                "running" => RuntimeStatus::Running,
                "exited" => RuntimeStatus::Exited,
                _ => RuntimeStatus::Missing,
            }),
            Err(RuntimeError::Failed(m)) if m.contains("not found") => Ok(RuntimeStatus::Missing),
            Err(e) => Err(e),
        }
    }

    async fn list_managed(&self) -> Result<Vec<Managed>, RuntimeError> {
        Ok(self
            .instances()
            .await?
            .into_iter()
            .map(|i| Managed {
                handle: i.id,
                instance_id: i.instance_id,
            })
            .collect())
    }

    async fn ensure_lab(&self, _network: &str, _subnet: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::Failed(
            "private VPN networks are not available on a remote lab host".into(),
        ))
    }

    async fn remove_lab(&self, _network: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Connects the way a player will: to the lab host's public address and the published port.
    /// That is the check that matters, and it also catches a firewall in the way.
    async fn reachable(&self, host_port: u16) -> bool {
        let address = format!("{}:{host_port}", self.public_host);
        match self
            .send::<ReadyInfo>(
                Method::GET,
                &format!("/v1/ready?port={host_port}"),
                None::<&()>,
            )
            .await
        {
            // The lab host says the service is not up (a machine still booting).
            Ok(ready) if !ready.ready => false,
            // The lab host checked the guest itself: all that is left is that the port can be
            // reached from here, the way a player will.
            Ok(ready) if ready.vouched => connects(&address).await,
            // A container, or a lab host that does not know the question: judge by the connection.
            _ => probe(&address).await,
        }
    }
}

/// Whether a TCP connection to `address` can be made at all.
pub async fn connects(address: &str) -> bool {
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, tokio::net::TcpStream::connect(address)).await,
        Ok(Ok(_))
    )
}

/// Whether a service really answers on `address`. A plain connect is not enough: a port forwarded
/// to a virtual machine or a container that is still starting accepts the connection and drops it
/// at once. So after connecting, the connection must stay open for a moment (a service waiting for
/// the client to speak) or send something (a banner).
pub async fn probe(address: &str) -> bool {
    use tokio::io::AsyncReadExt;
    let Ok(Ok(mut stream)) =
        tokio::time::timeout(PROBE_TIMEOUT, tokio::net::TcpStream::connect(address)).await
    else {
        return false;
    };
    let mut byte = [0u8; 1];
    match tokio::time::timeout(Duration::from_millis(400), stream.read(&mut byte)).await {
        // Silence for a moment: the service is there and waiting for us.
        Err(_) => true,
        // Data arrived (a banner): there.
        Ok(Ok(n)) if n > 0 => true,
        // Closed or reset straight away: nothing behind the port yet.
        _ => false,
    }
}

/// Handles are container ids or names; encode anything else so a path cannot be forged.
fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_parse_in_the_usual_spellings() {
        let colon = "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89";
        let plain = colon.replace(':', "").to_lowercase();
        assert_eq!(
            parse_fingerprint(colon).unwrap(),
            parse_fingerprint(&plain).unwrap()
        );
        assert!(parse_fingerprint("abc").is_err());
        assert!(parse_fingerprint(&"zz".repeat(32)).is_err());
        assert_eq!(fingerprint_of(b"x").len(), 95);
    }

    #[test]
    fn urls_and_pinning_are_checked_up_front() {
        assert!(RemoteRuntime::new("ftp://x", "t", None, "h").is_err());
        assert!(RemoteRuntime::new("http://x:1", "t", Some(&"ab".repeat(32)), "h").is_err());
        assert!(RemoteRuntime::new("https://x:1", "t", Some("nope"), "h").is_err());
        assert!(RemoteRuntime::new("https://x:1/", "t", Some(&"ab".repeat(32)), "h").is_ok());
        assert!(RemoteRuntime::new("http://127.0.0.1:1", "t", None, "h").is_ok());
    }

    #[test]
    fn path_segments_cannot_be_forged() {
        assert_eq!(urlencode("kymo-c1-u2-abc123"), "kymo-c1-u2-abc123");
        assert_eq!(urlencode("../x?y"), "..%2Fx%3Fy");
    }
}

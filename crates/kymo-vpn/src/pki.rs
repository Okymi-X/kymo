//! The platform's own certificate authority. Elliptic curve keys (P-256), one certificate per
//! player, a certificate revocation list, and the static key that wraps the control channel.
//!
//! OpenVPN's `remote-cert-tls` check needs both a key usage and an extended key usage on
//! the peer certificate, so every certificate here carries them.

use std::fmt::Write as _;

use rand::RngCore;
use rcgen::{
    BasicConstraints, CertificateParams, CertificateRevocationListParams, DistinguishedName,
    DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyIdMethod, KeyPair, KeyUsagePurpose,
    RevocationReason, RevokedCertParams, SerialNumber,
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::error::VpnError;

const CA_COMMON_NAME: &str = "Kymo VPN CA";

fn cert_err(e: impl std::fmt::Display) -> VpnError {
    VpnError::Certificate(e.to_string())
}

fn at(unix: i64) -> Result<OffsetDateTime, VpnError> {
    OffsetDateTime::from_unix_timestamp(unix).map_err(cert_err)
}

/// A certificate with its private key and serial number (lowercase hex).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issued {
    pub cert_pem: String,
    pub key_pem: String,
    pub serial: String,
}

/// What is stored to keep the authority across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaMaterial {
    pub cert_pem: String,
    pub key_pem: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revoked {
    pub serial: String,
    pub at: i64,
}

pub struct Ca {
    material: CaMaterial,
}

fn random_serial() -> (SerialNumber, String) {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    // A positive serial: clear the top bit of the first byte, and never start with zero.
    bytes[0] = (bytes[0] & 0x7f) | 0x40;
    let hex = bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    });
    (SerialNumber::from_slice(&bytes), hex)
}

fn name(common_name: &str) -> DistinguishedName {
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    dn
}

fn validity(params: &mut CertificateParams, now: i64, days: i64) -> Result<(), VpnError> {
    // A day of slack on the start tolerates players' clocks being a little behind.
    params.not_before = at(now)? - Duration::days(1);
    params.not_after = at(now)? + Duration::days(days);
    Ok(())
}

impl Ca {
    /// Makes a new authority valid for `days` from `now` (unix seconds).
    pub fn generate(now: i64, days: i64) -> Result<CaMaterial, VpnError> {
        let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(cert_err)?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(cert_err)?;
        params.distinguished_name = name(CA_COMMON_NAME);
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        validity(&mut params, now, days)?;
        let cert = params.self_signed(&key).map_err(cert_err)?;
        Ok(CaMaterial {
            cert_pem: cert.pem(),
            key_pem: key.serialize_pem(),
        })
    }

    pub fn load(material: CaMaterial) -> Result<Self, VpnError> {
        // Fail early on a damaged record instead of at the first player download.
        let ca = Self { material };
        ca.issuer()?;
        Ok(ca)
    }

    pub fn cert_pem(&self) -> &str {
        &self.material.cert_pem
    }

    fn issuer(&self) -> Result<Issuer<'static, KeyPair>, VpnError> {
        let key = KeyPair::from_pem(&self.material.key_pem).map_err(cert_err)?;
        Issuer::from_ca_cert_pem(&self.material.cert_pem, key).map_err(cert_err)
    }

    fn issue(
        &self,
        common_name: &str,
        usages: Vec<KeyUsagePurpose>,
        extended: ExtendedKeyUsagePurpose,
        now: i64,
        days: i64,
    ) -> Result<Issued, VpnError> {
        let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(cert_err)?;
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(cert_err)?;
        params.distinguished_name = name(common_name);
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = usages;
        params.extended_key_usages = vec![extended];
        let (serial, hex) = random_serial();
        params.serial_number = Some(serial);
        validity(&mut params, now, days)?;
        let cert = params.signed_by(&key, &self.issuer()?).map_err(cert_err)?;
        Ok(Issued {
            cert_pem: cert.pem(),
            key_pem: key.serialize_pem(),
            serial: hex,
        })
    }

    pub fn issue_server(&self, common_name: &str, now: i64, days: i64) -> Result<Issued, VpnError> {
        self.issue(
            common_name,
            vec![
                KeyUsagePurpose::DigitalSignature,
                KeyUsagePurpose::KeyEncipherment,
            ],
            ExtendedKeyUsagePurpose::ServerAuth,
            now,
            days,
        )
    }

    pub fn issue_client(&self, common_name: &str, now: i64, days: i64) -> Result<Issued, VpnError> {
        self.issue(
            common_name,
            vec![KeyUsagePurpose::DigitalSignature],
            ExtendedKeyUsagePurpose::ClientAuth,
            now,
            days,
        )
    }

    /// A signed revocation list. `number` must grow with every list, so OpenVPN can tell which
    /// is newer; `valid_days` is how long the list is accepted.
    pub fn crl(
        &self,
        revoked: &[Revoked],
        number: u64,
        now: i64,
        valid_days: i64,
    ) -> Result<String, VpnError> {
        let mut list = Vec::with_capacity(revoked.len());
        for r in revoked {
            let bytes = (0..r.serial.len())
                .step_by(2)
                .map(|i| {
                    r.serial
                        .get(i..i + 2)
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                })
                .collect::<Option<Vec<u8>>>()
                .filter(|b| !b.is_empty())
                .ok_or_else(|| VpnError::Certificate(format!("bad serial {}", r.serial)))?;
            list.push(RevokedCertParams {
                serial_number: SerialNumber::from_slice(&bytes),
                revocation_time: at(r.at)?,
                reason_code: Some(RevocationReason::CessationOfOperation),
                invalidity_date: None,
            });
        }
        let params = CertificateRevocationListParams {
            this_update: at(now)? - Duration::days(1),
            next_update: at(now)? + Duration::days(valid_days),
            crl_number: SerialNumber::from(number),
            issuing_distribution_point: None,
            revoked_certs: list,
            key_identifier_method: KeyIdMethod::Sha256,
        };
        let crl = params.signed_by(&self.issuer()?).map_err(cert_err)?;
        crl.pem().map_err(cert_err)
    }
}

/// An OpenVPN static key (2048 bits), used with `tls-crypt` so that only holders of a profile
/// can even start a handshake with the gateway.
pub fn generate_tls_crypt_key() -> String {
    let mut bytes = [0u8; 256];
    rand::rng().fill_bytes(&mut bytes);
    let mut out = String::from(
        "#\n# 2048 bit OpenVPN static key\n#\n-----BEGIN OpenVPN Static key V1-----\n",
    );
    for line in bytes.chunks(16) {
        for b in line {
            let _ = write!(out, "{b:02x}");
        }
        out.push('\n');
    }
    out.push_str("-----END OpenVPN Static key V1-----\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use x509_parser::prelude::*;

    const NOW: i64 = 1_800_000_000;

    fn der(pem: &str) -> Vec<u8> {
        parse_x509_pem(pem.as_bytes()).unwrap().1.contents
    }

    fn ca() -> Ca {
        Ca::load(Ca::generate(NOW, 3650).unwrap()).unwrap()
    }

    #[test]
    fn the_authority_is_a_real_ca_that_can_only_sign_leaf_certificates() {
        let material = Ca::generate(NOW, 365).unwrap();
        let bytes = der(&material.cert_pem);
        let (_, cert) = X509Certificate::from_der(&bytes).unwrap();
        let bc = cert.basic_constraints().unwrap().unwrap().value;
        assert!(bc.ca);
        assert_eq!(bc.path_len_constraint, Some(0));
        let ku = cert.key_usage().unwrap().unwrap().value;
        assert!(ku.key_cert_sign() && ku.crl_sign() && !ku.digital_signature());
        assert_eq!(cert.subject().to_string(), "CN=Kymo VPN CA");
        cert.verify_signature(None).expect("self signed");
    }

    #[test]
    fn server_and_client_certificates_pass_openvpn_remote_cert_tls_rules() {
        let ca = ca();
        let server = ca.issue_server("kymo-gateway", NOW, 365).unwrap();
        let client = ca.issue_client("u42", NOW, 30).unwrap();
        let ca_bytes = der(ca.cert_pem());
        let (_, ca_cert) = X509Certificate::from_der(&ca_bytes).unwrap();

        let s_bytes = der(&server.cert_pem);
        let (_, s) = X509Certificate::from_der(&s_bytes).unwrap();
        s.verify_signature(Some(ca_cert.public_key()))
            .expect("signed by the authority");
        assert!(s.extended_key_usage().unwrap().unwrap().value.server_auth);
        assert!(!s.extended_key_usage().unwrap().unwrap().value.client_auth);
        let ku = s.key_usage().unwrap().unwrap().value;
        assert!(ku.digital_signature() && ku.key_encipherment());
        assert!(!s.basic_constraints().unwrap().unwrap().value.ca);

        let c_bytes = der(&client.cert_pem);
        let (_, c) = X509Certificate::from_der(&c_bytes).unwrap();
        c.verify_signature(Some(ca_cert.public_key()))
            .expect("signed by the authority");
        assert!(c.extended_key_usage().unwrap().unwrap().value.client_auth);
        assert!(!c.extended_key_usage().unwrap().unwrap().value.server_auth);
        assert!(c.key_usage().unwrap().unwrap().value.digital_signature());
        assert_eq!(c.subject().to_string(), "CN=u42");
        assert_eq!(c.issuer().to_string(), "CN=Kymo VPN CA");
        assert_eq!(hex_of(c.raw_serial()), client.serial);
        // Slack on the start, the requested lifetime on the end.
        assert_eq!(c.validity().not_before.timestamp(), NOW - 86_400);
        assert_eq!(c.validity().not_after.timestamp(), NOW + 30 * 86_400);
    }

    fn hex_of(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn serials_are_unique_positive_and_never_start_with_zero() {
        let ca = ca();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            let c = ca.issue_client("u1", NOW, 1).unwrap();
            assert_eq!(c.serial.len(), 32);
            assert!(u8::from_str_radix(&c.serial[..2], 16).unwrap() >= 0x40);
            assert!(u8::from_str_radix(&c.serial[..2], 16).unwrap() < 0x80);
            assert!(seen.insert(c.serial));
        }
    }

    #[test]
    fn a_stored_authority_loads_again_and_still_signs_for_the_same_ca() {
        let material = Ca::generate(NOW, 3650).unwrap();
        let first = Ca::load(material.clone())
            .unwrap()
            .issue_client("a", NOW, 1)
            .unwrap();
        let second = Ca::load(material.clone())
            .unwrap()
            .issue_client("b", NOW, 1)
            .unwrap();
        let ca_bytes = der(&material.cert_pem);
        let (_, ca_cert) = X509Certificate::from_der(&ca_bytes).unwrap();
        for issued in [first, second] {
            let b = der(&issued.cert_pem);
            let (_, c) = X509Certificate::from_der(&b).unwrap();
            c.verify_signature(Some(ca_cert.public_key()))
                .expect("same authority");
        }
        let broken = CaMaterial {
            cert_pem: material.cert_pem,
            key_pem: "nope".into(),
        };
        assert!(Ca::load(broken).is_err());
    }

    #[test]
    fn the_revocation_list_names_the_revoked_serials_and_is_signed_by_the_authority() {
        let ca = ca();
        let gone = ca.issue_client("u1", NOW, 30).unwrap();
        let kept = ca.issue_client("u2", NOW, 30).unwrap();
        let pem = ca
            .crl(
                &[Revoked {
                    serial: gone.serial.clone(),
                    at: NOW + 5,
                }],
                7,
                NOW + 10,
                365,
            )
            .unwrap();
        let (_, parsed) = parse_x509_pem(pem.as_bytes()).unwrap();
        let (_, crl) = CertificateRevocationList::from_der(&parsed.contents).unwrap();
        let revoked: Vec<String> = crl
            .iter_revoked_certificates()
            .map(|r| hex_of(r.raw_serial()))
            .collect();
        assert_eq!(revoked, vec![gone.serial.clone()]);
        assert!(!revoked.contains(&kept.serial));
        let ca_bytes = der(ca.cert_pem());
        let (_, ca_cert) = X509Certificate::from_der(&ca_bytes).unwrap();
        crl.verify_signature(ca_cert.public_key())
            .expect("signed by the authority");
        assert!(crl.next_update().unwrap().timestamp() > NOW + 300 * 86_400);
        assert_eq!(
            crl.crl_number().map(|n| n.to_string()),
            Some("7".to_owned())
        );
    }

    #[test]
    fn an_empty_revocation_list_is_valid_and_bad_serials_are_refused() {
        let ca = ca();
        assert!(ca.crl(&[], 1, NOW, 30).unwrap().contains("BEGIN X509 CRL"));
        for bad in ["", "zz", "abc"] {
            assert!(
                ca.crl(
                    &[Revoked {
                        serial: bad.into(),
                        at: NOW
                    }],
                    2,
                    NOW,
                    30
                )
                .is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_tls_crypt_key_has_openvpns_static_key_shape_and_is_random() {
        let key = generate_tls_crypt_key();
        let lines: Vec<&str> = key.lines().collect();
        assert_eq!(lines[3], "-----BEGIN OpenVPN Static key V1-----");
        assert_eq!(
            *lines.last().unwrap(),
            "-----END OpenVPN Static key V1-----"
        );
        let body = &lines[4..lines.len() - 1];
        assert_eq!(body.len(), 16);
        assert!(
            body.iter()
                .all(|l| l.len() == 32 && l.chars().all(|c| c.is_ascii_hexdigit()))
        );
        assert_ne!(key, generate_tls_crypt_key());
    }
}

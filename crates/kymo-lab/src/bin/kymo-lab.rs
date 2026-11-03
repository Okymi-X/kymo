//! `kymo-lab`: the Kymo lab host. Runs challenge containers for a Kymo platform on this machine.
//!
//! ```text
//! kymo-lab               run the lab host (configured by KYMO_LAB_* environment variables)
//! kymo-lab fingerprint   print the certificate fingerprint the platform must pin
//! kymo-lab check         verify the configuration, Docker access and manifests, then exit
//! kymo-lab version       print the version
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use kymo_instances::InstanceRuntime;
use kymo_instances::docker::DockerRuntime;
use kymo_lab::manifest;
use kymo_lab::server::{self, Cidr, Lab, LabConfig, Mode};
use kymo_lab::tls;

const MIN_TOKEN: usize = 24;

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

struct Settings {
    addr: String,
    data: PathBuf,
    manifests: PathBuf,
    bind: String,
    tls: bool,
    tls_names: Vec<String>,
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
    config: LabConfig,
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|h| h.trim().to_owned())
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "lab".into())
}

fn settings() -> Result<Settings, String> {
    let token = match (var("KYMO_LAB_TOKEN"), var("KYMO_LAB_TOKEN_FILE")) {
        (Some(t), _) => t,
        (None, Some(file)) => std::fs::read_to_string(&file).map_err(|e| format!("KYMO_LAB_TOKEN_FILE {file}: {e}"))?.trim().to_owned(),
        (None, None) => return Err("set KYMO_LAB_TOKEN (or KYMO_LAB_TOKEN_FILE): the secret the platform authenticates with".into()),
    };
    if token.len() < MIN_TOKEN {
        return Err(format!(
            "the token must be at least {MIN_TOKEN} characters (try: openssl rand -hex 24)"
        ));
    }
    let data: PathBuf = var("KYMO_LAB_DATA")
        .unwrap_or_else(|| "/var/lib/kymo-lab".into())
        .into();
    let manifests: PathBuf = var("KYMO_LAB_MANIFESTS")
        .unwrap_or_else(|| "/etc/kymo-lab/manifests".into())
        .into();
    let mut config = LabConfig::new(var("KYMO_LAB_NAME").unwrap_or_else(hostname), token);
    config.mode = match var("KYMO_LAB_MODE").as_deref() {
        None | Some("manifest") => Mode::Manifest,
        Some("open") => Mode::Open,
        Some(other) => return Err(format!("KYMO_LAB_MODE is manifest or open, not {other}")),
    };
    if let Some(max) = var("KYMO_LAB_MAX_INSTANCES") {
        config.max_instances = max
            .parse()
            .map_err(|_| "KYMO_LAB_MAX_INSTANCES must be a number".to_owned())?;
    }
    if let Some(prefix) = var("KYMO_LAB_NAME_PREFIX") {
        config.name_prefix = prefix;
    }
    for part in var("KYMO_LAB_ALLOW_FROM")
        .unwrap_or_default()
        .split(',')
        .filter(|p| !p.trim().is_empty())
    {
        config.allow_from.push(Cidr::parse(part)?);
    }
    config.manifests = manifest::load_dir(&manifests)?;
    config.state_path = Some(data.join("instances.json"));
    Ok(Settings {
        addr: var("KYMO_LAB_ADDR").unwrap_or_else(|| "0.0.0.0:7400".into()),
        data,
        manifests,
        bind: var("KYMO_LAB_BIND").unwrap_or_else(|| "0.0.0.0".into()),
        tls: var("KYMO_LAB_TLS").as_deref() != Some("off"),
        tls_names: var("KYMO_LAB_TLS_NAMES")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .collect(),
        cert: var("KYMO_LAB_TLS_CERT").map(Into::into),
        key: var("KYMO_LAB_TLS_KEY").map(Into::into),
        config,
    })
}

fn identity(s: &Settings) -> Result<tls::Identity, String> {
    match (&s.cert, &s.key) {
        (Some(cert), Some(key)) => tls::Identity::from_pem(
            std::fs::read_to_string(cert).map_err(|e| format!("{}: {e}", cert.display()))?,
            std::fs::read_to_string(key).map_err(|e| format!("{}: {e}", key.display()))?,
        ),
        (None, None) => {
            let mut names = s.tls_names.clone();
            names.push(hostname());
            tls::load_or_create(&s.data.join("tls"), &names)
        }
        _ => Err("set both KYMO_LAB_TLS_CERT and KYMO_LAB_TLS_KEY, or neither".into()),
    }
}

async fn check(s: &Settings) -> Result<(), String> {
    println!("lab name:      {}", s.config.name);
    println!("mode:          {}", s.config.mode.as_str());
    println!(
        "listen:        {} ({})",
        s.addr,
        if s.tls {
            "https"
        } else {
            "http, no encryption"
        }
    );
    println!(
        "manifests:     {} in {}",
        s.config.manifests.len(),
        s.manifests.display()
    );
    for m in &s.config.manifests {
        println!("  - {} (port {})", m.artifact, m.port);
    }
    if s.config.mode == Mode::Manifest && s.config.manifests.is_empty() {
        println!("warning: manifest mode with no manifests: the lab host will refuse every start");
    }
    let runtime = DockerRuntime::connect(s.bind.clone()).map_err(|e| format!("docker: {e}"))?;
    let managed = runtime
        .list_managed()
        .await
        .map_err(|e| format!("docker: {e}"))?;
    println!(
        "docker:        reachable, {} managed container(s)",
        managed.len()
    );
    if s.tls {
        println!("fingerprint:   {}", identity(s)?.fingerprint);
    }
    println!("ok");
    Ok(())
}

async fn run(s: Settings) -> Result<(), String> {
    let runtime: Arc<dyn InstanceRuntime> =
        Arc::new(DockerRuntime::connect(s.bind.clone()).map_err(|e| format!("docker: {e}"))?);
    let acceptor = if s.tls {
        let id = identity(&s)?;
        tracing::info!(
            "certificate fingerprint (pin this in the platform): {}",
            id.fingerprint
        );
        Some(tls::acceptor(&id)?)
    } else {
        tracing::warn!(
            "TLS is off: the token and instance details cross the network in clear text"
        );
        None
    };
    let lab = Lab::new(s.config, runtime);
    lab.load().await;
    lab.spawn_reaper();
    let listener = tokio::net::TcpListener::bind(&s.addr)
        .await
        .map_err(|e| format!("bind {}: {e}", s.addr))?;
    tracing::info!(
        "kymo-lab {} listening on {}",
        env!("CARGO_PKG_VERSION"),
        s.addr
    );
    server::serve(listener, server::router(lab), acceptor, async {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async { match term.as_mut() { Some(t) => { t.recv().await; } None => std::future::pending().await } } => {}
        }
    })
    .await;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let command = std::env::args().nth(1).unwrap_or_default();
    let result = match command.as_str() {
        "version" | "--version" => {
            println!("kymo-lab {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("{}", include_str!("../../README.txt"));
            Ok(())
        }
        "fingerprint" => settings()
            .and_then(|s| identity(&s))
            .map(|id| println!("{}", id.fingerprint)),
        "check" => match settings() {
            Ok(s) => check(&s).await,
            Err(e) => Err(e),
        },
        "" | "run" => match settings() {
            Ok(s) => run(s).await,
            Err(e) => Err(e),
        },
        other => Err(format!("unknown command {other}; try kymo-lab help")),
    };
    if let Err(e) = result {
        eprintln!("kymo-lab: {e}");
        std::process::exit(1);
    }
}

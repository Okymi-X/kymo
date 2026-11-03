//! `kymo-lab`: the Kymo lab host. Runs challenge containers for a Kymo platform on this machine.
//!
//! ```text
//! kymo-lab               run the lab host (configured by KYMO_LAB_* environment variables)
//! kymo-lab fingerprint   print the certificate fingerprint the platform must pin
//! kymo-lab check         verify the configuration, Docker and QEMU access and manifests, then exit
//! kymo-lab prepare       unpack the virtual machine images now (they also unpack when the service starts)
//! kymo-lab version       print the version
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use kymo_instances::InstanceRuntime;
use kymo_instances::docker::DockerRuntime;
use kymo_lab::manifest;
use kymo_lab::server::{self, Cidr, Lab, LabConfig, Mode};
use kymo_lab::tls;
use kymo_lab::vm::{Accel, VmConfig, VmRuntime};

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
    docker: bool,
    vm: Option<VmConfig>,
    config: LabConfig,
}

/// A program found on PATH.
fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return std::path::Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
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
    if let Some(mb) = var("KYMO_LAB_MAX_MEMORY_MB") {
        config.caps.max_memory_mb = mb
            .parse()
            .map_err(|_| "KYMO_LAB_MAX_MEMORY_MB must be a number".to_owned())?;
    }
    if let Some(cores) = var("KYMO_LAB_MAX_CPUS") {
        let cores: u32 = cores
            .parse()
            .map_err(|_| "KYMO_LAB_MAX_CPUS must be a number".to_owned())?;
        config.caps.max_cpu_millis = cores.saturating_mul(1000);
    }
    if let Some(secs) = var("KYMO_LAB_MAX_TTL_SECS") {
        config.caps.max_ttl_secs = secs
            .parse()
            .map_err(|_| "KYMO_LAB_MAX_TTL_SECS must be a number".to_owned())?;
    }

    // Which runtimes: containers (Docker or Podman), virtual machines (QEMU), or both.
    let asked = var("KYMO_LAB_RUNTIMES");
    let wants = |name: &str| {
        asked
            .as_deref()
            .is_none_or(|list| list.split(',').any(|r| r.trim() == name))
    };
    if let Some(list) = &asked
        && let Some(bad) = list
            .split(',')
            .map(str::trim)
            .find(|r| !matches!(*r, "docker" | "vm"))
    {
        return Err(format!("KYMO_LAB_RUNTIMES lists docker and vm, not {bad}"));
    }
    let bind = var("KYMO_LAB_BIND").unwrap_or_else(|| "0.0.0.0".into());
    let qemu = var("KYMO_LAB_QEMU").unwrap_or_else(|| "qemu-system-x86_64".into());
    let vm = if wants("vm") && (asked.is_some() || on_path(&qemu)) {
        let images = var("KYMO_LAB_IMAGES").map_or_else(|| data.join("images"), PathBuf::from);
        let mut vm = VmConfig::new(images, data.join("vm"));
        vm.qemu = qemu;
        if let Some(img) = var("KYMO_LAB_QEMU_IMG") {
            vm.qemu_img = img;
        }
        vm.bind_ip = bind.clone();
        vm.accel = Accel::parse(&var("KYMO_LAB_VM_ACCEL").unwrap_or_default())?;
        if let Some(range) = var("KYMO_LAB_VM_PORTS") {
            let (lo, hi) = range
                .split_once('-')
                .ok_or("KYMO_LAB_VM_PORTS is a range like 40000-40999")?;
            vm.port_range = (
                lo.trim()
                    .parse()
                    .map_err(|_| "KYMO_LAB_VM_PORTS is a range like 40000-40999".to_owned())?,
                hi.trim()
                    .parse()
                    .map_err(|_| "KYMO_LAB_VM_PORTS is a range like 40000-40999".to_owned())?,
            );
            if vm.port_range.0 > vm.port_range.1 || vm.port_range.0 < 1024 {
                return Err(
                    "KYMO_LAB_VM_PORTS must be a range of unprivileged ports, low to high".into(),
                );
            }
        }
        Some(vm)
    } else {
        None
    };
    let docker = wants("docker");
    if !docker {
        config.runtime_name = String::new();
    }
    if config.manifests.iter().any(|m| m.is_vm()) && vm.is_none() {
        tracing::warn!(
            "a manifest asks for a virtual machine but QEMU is not set up (install qemu, or set KYMO_LAB_RUNTIMES=docker,vm)"
        );
    }
    Ok(Settings {
        addr: var("KYMO_LAB_ADDR").unwrap_or_else(|| "0.0.0.0:7400".into()),
        data,
        manifests,
        bind,
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
        docker,
        vm,
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
        println!(
            "  - {} ({}, port {})",
            m.artifact,
            m.runtime,
            m.primary_port()
        );
    }
    if s.config.mode == Mode::Manifest && s.config.manifests.is_empty() {
        println!("warning: manifest mode with no manifests: the lab host will refuse every start");
    }
    if s.docker {
        let runtime = DockerRuntime::connect_scoped(s.bind.clone(), "lab")
            .map_err(|e| format!("docker: {e}"))?;
        let managed = runtime
            .list_managed()
            .await
            .map_err(|e| format!("docker: {e}"))?;
        println!(
            "docker:        reachable, {} managed container(s)",
            managed.len()
        );
    } else {
        println!("docker:        off (KYMO_LAB_RUNTIMES)");
    }
    match &s.vm {
        Some(cfg) => {
            let vm = VmRuntime::new(cfg.clone_config());
            for line in vm.report().await {
                println!("vm:            {line}");
            }
            for m in s.config.manifests.iter().filter(|m| m.is_vm()) {
                let file = m.file.clone().unwrap_or_default();
                let found = cfg.images_dir.join(&file).is_file();
                let ports: Vec<String> = m
                    .declared_ports()
                    .iter()
                    .map(|p| format!("{} {}", p.name, p.port))
                    .collect();
                println!(
                    "  - {} ({file}, {}): {}",
                    m.artifact,
                    if found {
                        "image found"
                    } else {
                        "IMAGE FILE MISSING"
                    },
                    ports.join(", ")
                );
            }
        }
        None => println!("vm:            off (QEMU not found, or KYMO_LAB_RUNTIMES leaves it out)"),
    }
    if s.tls {
        println!("fingerprint:   {}", identity(s)?.fingerprint);
    }
    println!("ok");
    Ok(())
}

/// Unpacks every virtual machine image now and says how long it took.
async fn prepare(s: &Settings) -> Result<(), String> {
    let cfg =
        s.vm.as_ref()
            .ok_or("virtual machines are not set up (QEMU not found)")?;
    let vm = VmRuntime::new(cfg.clone_config());
    let mut failed = false;
    for m in s.config.manifests.iter().filter(|m| m.is_vm()) {
        let file = m.file.clone().unwrap_or_default();
        let started = std::time::Instant::now();
        print!("{}: preparing {file} ... ", m.artifact);
        match vm.prepare(&m.artifact, &file).await {
            Ok(()) => println!("ready ({} s)", started.elapsed().as_secs()),
            Err(e) => {
                failed = true;
                println!("FAILED: {e}");
            }
        }
    }
    if failed {
        Err("some images could not be prepared".into())
    } else {
        Ok(())
    }
}

async fn run(s: Settings) -> Result<(), String> {
    let runtime: Arc<dyn InstanceRuntime> = if s.docker {
        Arc::new(
            DockerRuntime::connect_scoped(s.bind.clone(), "lab")
                .map_err(|e| format!("docker: {e}"))?,
        )
    } else {
        Arc::new(server::Absent)
    };
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
    let vm = s.vm.map(VmRuntime::new);
    let lab = Lab::with_vm(s.config, runtime, vm);
    lab.load().await;
    lab.spawn_reaper();
    lab.prepare_images();
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
        "prepare" => match settings() {
            Ok(s) => prepare(&s).await,
            Err(e) => Err(e),
        },
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

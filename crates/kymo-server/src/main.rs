use std::env;
use std::process::ExitCode;

use std::sync::Arc;
use std::time::Duration;

use kymo_instances::{Policy, ResourceCaps};
use kymo_server::ops::BackupConfig;
use kymo_server::state::InstanceSetup;
use kymo_server::vpn::{self, VpnSetup};
use kymo_server::{AppState, db, instances, ops, restore as restore_db, router};
use tracing_subscriber::EnvFilter;

const MIN_PASSWORD: usize = 12;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!("{message}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("restore") {
        return restore(&args[1..]).await;
    }
    let seed_only = args.first().map(String::as_str) == Some("seed-demo");
    let password =
        env::var("KYMO_ADMIN_PASSWORD").map_err(|_| "KYMO_ADMIN_PASSWORD is required")?;
    if password.chars().count() < MIN_PASSWORD {
        return Err(format!(
            "KYMO_ADMIN_PASSWORD must be at least {MIN_PASSWORD} characters"
        ));
    }
    let db_url = env::var("KYMO_DATABASE_URL").unwrap_or_else(|_| "sqlite://kymo.db".into());
    let addr = env::var("KYMO_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());

    let pool = db::connect(&db_url)
        .await
        .map_err(|e| format!("database: {e}"))?;
    if seed_only {
        let added = kymo_server::demo::seed(&pool)
            .await
            .map_err(|_| "could not add demo data".to_owned())?;
        tracing::info!("added {added} demo tournaments");
        return Ok(());
    }
    let mut state = AppState::new(pool, &password).map_err(|e| format!("password hashing: {e}"))?;
    if let Ok(dir) = env::var("KYMO_BACKUP_DIR") {
        state = state.with_backups(backup_config(dir)?);
    }
    state = state.with_trusted_proxy(env::var("KYMO_TRUST_PROXY").as_deref() == Ok("1"));
    spawn_maintenance(state.clone());
    // `docker` runs challenge containers on this machine (and still lets events use lab hosts);
    // `labs` runs none here and leaves everything to the lab hosts added in the admin pages.
    match env::var("KYMO_INSTANCES").as_deref() {
        Ok("docker") => {
            state = state.with_instances(docker_setup()?);
            spawn_reaper(state.clone());
        }
        Ok("labs") => {
            state = state.with_instances(labs_only_setup());
            spawn_reaper(state.clone());
        }
        Ok(other) if !other.is_empty() => {
            return Err(format!("KYMO_INSTANCES is docker or labs, not {other}"));
        }
        _ => {}
    }
    if env::var("KYMO_VPN").as_deref() == Ok("1") {
        let runtime = state
            .instance_setup()
            .map(|setup| Arc::clone(&setup.runtime))
            .ok_or(
                "KYMO_VPN=1 needs KYMO_INSTANCES=docker or labs: the VPN reaches the challenge containers",
            )?;
        // With `labs` this server has no gateway of its own: each lab host runs one, and this
        // server keeps the authority and drives them.
        let local = env::var("KYMO_INSTANCES").as_deref() == Ok("docker");
        state = state.with_vpn(vpn_setup(runtime, local)?);
        // A gateway that cannot start yet (image not built, platform down) is retried by the
        // maintenance task, so the server still starts and says what is wrong.
        match vpn::sync(&state).await {
            Ok(report) => tracing::info!(
                "vpn ready: {} labs, {} players",
                report.labs,
                report.clients
            ),
            Err(e) => tracing::warn!("vpn could not start yet: {e:?}"),
        }
    }
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    tracing::info!("kymo listening on {addr}");
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
    .map_err(|e| e.to_string())
}

/// A platform that runs no containers itself: every instance goes to a lab host.
fn labs_only_setup() -> InstanceSetup {
    InstanceSetup {
        runtime: Arc::new(kymo_server::labs::NoLocalRuntime),
        policy: Policy::default(),
        caps: ResourceCaps::default(),
        public_host: "127.0.0.1".into(),
        reap_grace_secs: 5,
    }
}

/// Docker backed instances. The bind address decides who can reach published ports; the host
/// name is what players are told to connect to.
fn docker_setup() -> Result<InstanceSetup, String> {
    let bind = env::var("KYMO_INSTANCE_BIND").unwrap_or_else(|_| "0.0.0.0".into());
    let host = env::var("KYMO_INSTANCE_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let runtime =
        kymo_instances::docker::DockerRuntime::connect(bind).map_err(|e| format!("docker: {e}"))?;
    Ok(InstanceSetup {
        runtime: Arc::new(runtime),
        policy: Policy::default(),
        caps: ResourceCaps::default(),
        public_host: host,
        reap_grace_secs: 5,
    })
}

/// Expires instances and removes orphaned containers every 30 seconds.
fn spawn_reaper(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            if let Err(e) = instances::reap(&state).await {
                tracing::warn!("instance reaper failed: {e:?}");
            }
        }
    });
}

/// Housekeeping that runs whatever the configuration: old request log rows and expired
/// sessions are deleted, and a scheduled backup is written when one is due.
fn spawn_maintenance(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            if let Err(e) = ops::maintain(&state).await {
                tracing::warn!("maintenance failed: {e:?}");
            }
        }
    });
}

fn backup_config(dir: String) -> Result<BackupConfig, String> {
    let number = |name: &str, default: u64| -> Result<u64, String> {
        match env::var(name) {
            Ok(v) => v
                .parse::<u64>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("{name} must be a positive number")),
            Err(_) => Ok(default),
        }
    };
    Ok(BackupConfig {
        dir: dir.into(),
        every_secs: i64::try_from(number("KYMO_BACKUP_EVERY_HOURS", 24)? * 3600)
            .map_err(|_| "KYMO_BACKUP_EVERY_HOURS is too large".to_owned())?,
        keep: usize::try_from(number("KYMO_BACKUP_KEEP", 7)?)
            .map_err(|_| "KYMO_BACKUP_KEEP is too large".to_owned())?,
    })
}

/// `kymo-server restore <backup.db> [--force]`: replaces the database file with a backup
/// while the server is stopped, keeping the old file next to it.
async fn restore(args: &[String]) -> Result<(), String> {
    let force = args.iter().any(|a| a == "--force");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [backup] = files.as_slice() else {
        return Err("usage: kymo-server restore <backup.db> [--force]".into());
    };
    let url = env::var("KYMO_DATABASE_URL").unwrap_or_else(|_| "sqlite://kymo.db".into());
    let target = restore_db::path_of_url(&url)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let kept = restore_db::restore(
        &target,
        std::path::Path::new(backup.as_str()),
        force,
        now as i64,
    )
    .await?;
    match kept {
        Some(path) => tracing::info!(
            "restored {}; the previous database is kept at {}",
            target.display(),
            path.display()
        ),
        None => tracing::info!("restored {}", target.display()),
    }
    Ok(())
}

/// The OpenVPN gateway. The directory is created if needed and made absolute, because a bind
/// mount needs an absolute path.
fn vpn_setup(
    runtime: Arc<dyn kymo_instances::InstanceRuntime>,
    local: bool,
) -> Result<VpnSetup, String> {
    use kymo_vpn::{GatewayConfig, Ipv4Net, LabPool, TunnelPool, valid_remote_host};
    let host = match env::var("KYMO_VPN_HOST") {
        Ok(h) => h,
        Err(_) if !local => "-".into(),
        Err(_) => {
            return Err("KYMO_VPN_HOST is required: the name or address players connect to".into());
        }
    };
    if local && !valid_remote_host(&host) {
        return Err("KYMO_VPN_HOST must be a host name or an IPv4 address".into());
    }
    let number = |name: &str, default: u64| -> Result<u64, String> {
        match env::var(name) {
            Ok(v) => v.parse().map_err(|_| format!("{name} must be a number")),
            Err(_) => Ok(default),
        }
    };
    let port = u16::try_from(number("KYMO_VPN_PORT", 1194)?)
        .ok()
        .filter(|p| *p > 0)
        .ok_or("KYMO_VPN_PORT must be 1 to 65535")?;
    let net = |name: &str, default: &str| -> Result<Ipv4Net, String> {
        env::var(name)
            .unwrap_or_else(|_| default.into())
            .parse::<Ipv4Net>()
            .map_err(|e| format!("{name}: {e}"))
    };
    let tunnel = TunnelPool::new(net("KYMO_VPN_TUNNEL", "10.8.0.0/16")?)
        .map_err(|e| format!("KYMO_VPN_TUNNEL: {e}"))?;
    let labs = LabPool::new(net("KYMO_VPN_LABS", "10.77.0.0/16")?, 27)
        .map_err(|e| format!("KYMO_VPN_LABS: {e}"))?;
    if tunnel.net().overlaps(&labs.pool()) {
        return Err("KYMO_VPN_TUNNEL and KYMO_VPN_LABS must not overlap".into());
    }
    let dir = env::var("KYMO_VPN_DIR").unwrap_or_else(|_| "kymo-vpn".into());
    std::fs::create_dir_all(&dir).map_err(|e| format!("KYMO_VPN_DIR: {e}"))?;
    let dir = std::fs::canonicalize(&dir).map_err(|e| format!("KYMO_VPN_DIR: {e}"))?;
    let gateway_config = GatewayConfig {
        name: "kymo-vpn-gateway".into(),
        image: env::var("KYMO_VPN_IMAGE").unwrap_or_else(|_| "localhost/kymo/vpn-gateway:1".into()),
        dir: dir.clone(),
        publish_ip: env::var("KYMO_VPN_BIND").unwrap_or_else(|_| "0.0.0.0".into()),
        publish_port: port,
        bind_options: env::var("KYMO_VPN_BIND_OPTIONS").unwrap_or_default(),
    };
    let gateway: Arc<dyn kymo_vpn::Gateway> = if local {
        Arc::new(kymo_vpn::docker::DockerGateway::connect().map_err(|e| format!("docker: {e}"))?)
    } else {
        Arc::new(kymo_server::labs::NoLocalGateway)
    };
    Ok(VpnSetup {
        gateway,
        runtime,
        dir,
        gateway_config,
        public_host: host,
        public_port: port,
        tunnel,
        labs,
        client_days: i64::try_from(number("KYMO_VPN_CERT_DAYS", 60)?)
            .map_err(|_| "KYMO_VPN_CERT_DAYS is too large")?,
        lock: Arc::default(),
        local,
    })
}

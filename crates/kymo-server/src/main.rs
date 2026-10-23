use std::env;
use std::process::ExitCode;

use kymo_server::{AppState, db, router};
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
    let seed_only = env::args().nth(1).as_deref() == Some("seed-demo");
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
    let state = AppState::new(pool, &password).map_err(|e| format!("password hashing: {e}"))?;
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

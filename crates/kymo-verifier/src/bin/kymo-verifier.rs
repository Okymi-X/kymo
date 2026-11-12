//! The Verifier service of one instance. The lab host starts it in a container with the
//! definition directory mounted read-only and a private file describing the run.

use std::path::PathBuf;
use std::process::ExitCode;

use kymo_verifier::service::{AppState, RunInfo, router, system_clock};
use kymo_verifier::{Context, load};

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

async fn run() -> Result<(), String> {
    let definition = PathBuf::from(env_or("KYMO_VERIFIER_DEFINITION", "/verifier"));
    let run_file = PathBuf::from(env_or("KYMO_VERIFIER_RUN", "/run/kymo/run.json"));
    let lib_dir = PathBuf::from(env_or("KYMO_VERIFIER_LIB", "/usr/local/lib/kymo-verifier"));
    let addr = env_or("KYMO_VERIFIER_ADDR", "0.0.0.0:8080");

    let loaded = load(&definition).map_err(|problems| problems.join("; "))?;
    // The lab host hands the run over in the environment; a file works for running by hand.
    let text = match std::env::var("KYMO_VERIFIER_RUN_JSON") {
        Ok(text) if !text.is_empty() => text,
        _ => std::fs::read_to_string(&run_file)
            .map_err(|e| format!("{}: {e}", run_file.display()))?,
    };
    let run: RunInfo =
        serde_json::from_str(&text).map_err(|e| format!("the run description: {e}"))?;
    let context = Context::new(&loaded, &run.target_ip, run.target_ports.clone(), &lib_dir)
        .map_err(|e| e.to_string())?;
    let app = router(AppState::new(loaded, context, &run, system_clock()));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("{addr}: {e}"))?;
    tracing::info!("verifier listening on {addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string())
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kymo-verifier: {e}");
            ExitCode::FAILURE
        }
    }
}

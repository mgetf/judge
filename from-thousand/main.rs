use axum::Router;
use tracing_subscriber::EnvFilter;

use slugsocial_server::{
    create_app,
    event_log::EventLog,
    journal,
    state::{AppConfig, AppState},
    x402::{X402Config, DEFAULT_BASE_RPC_URL},
};

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

fn env_bool(name: &str) -> Option<bool> {
    env_var(name).map(|s| matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[boot] slugsocial-server starting");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let data_dir = env_var("SLUG_DATA_DIR").unwrap_or_else(|| "/data".to_string());
    let event_log_path =
        env_var("SLUG_EVENT_LOG").unwrap_or_else(|| format!("{data_dir}/events.jsonl"));
    let view_log_path =
        env_var("SLUG_VIEW_LOG").unwrap_or_else(|| format!("{data_dir}/views.jsonl"));
    let port: u16 = env_var("PORT").and_then(|s| s.parse().ok()).unwrap_or(8080);

    let allow_unpaid_posts = env_bool("SLUG_ALLOW_UNPAID_POSTS").unwrap_or(false);
    let x402_pay_to = env_var("SLUG_X402_PAY_TO");
    let x402_enabled = env_bool("SLUG_X402_ENABLED").unwrap_or(x402_pay_to.is_some());
    if !allow_unpaid_posts && !x402_enabled {
        return Err(
            "refusing to start: x402 is not configured (set SLUG_X402_PAY_TO). \
             For local dev without payments, set SLUG_ALLOW_UNPAID_POSTS=1"
                .into(),
        );
    }
    if x402_enabled && x402_pay_to.is_none() {
        return Err("SLUG_X402_PAY_TO is required when x402 is enabled".into());
    }
    let settler_private_key = env_var("SLUG_X402_SETTLER_PRIVATE_KEY");
    if x402_enabled && settler_private_key.is_none() {
        return Err(
            "SLUG_X402_SETTLER_PRIVATE_KEY is required when x402 is enabled \
             (hex private key for the wallet that submits Base settlement txs; needs ETH for gas)"
                .into(),
        );
    }
    let rpc_url = env_var("SLUG_BASE_RPC_URL").unwrap_or_else(|| DEFAULT_BASE_RPC_URL.to_string());
    let x402 = X402Config {
        enabled: x402_enabled,
        pay_to: x402_pay_to,
        settler_private_key,
        rpc_url,
        max_timeout_seconds: env_var("SLUG_X402_MAX_TIMEOUT_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(60),
        public_url: env_var("SLUG_PUBLIC_URL")
            .unwrap_or_else(|| format!("http://localhost:{port}")),
    };

    eprintln!(
        "[boot] data_dir={data_dir} event_log_path={event_log_path} view_log_path={view_log_path}"
    );
    if let Some(ref pay_to) = x402.pay_to {
        eprintln!("[boot] x402 pay_to={pay_to} (board revenue settles to creator wallet on Base)");
    }
    if x402.enabled {
        eprintln!(
            "[boot] x402 local settle via Base RPC {} (no external facilitator)",
            x402.rpc_url
        );
    }

    let cfg = AppConfig {
        data_dir: data_dir.clone(),
        event_log_path: event_log_path.clone(),
        view_log_path: view_log_path.clone(),
        x402,
        allow_unpaid_posts,
    };

    let state = AppState::new(cfg)?;

    let event_log_for_errors = event_log_path.clone();
    let (events, bad) = EventLog::new(event_log_path).load_all().await?;
    if !bad.is_empty() {
        tracing::warn!(bad_lines = bad.len(), "skipped corrupt JSONL lines");
    }
    {
        let mut reduced = state.reduced.write().await;
        for ev in events {
            reduced
                .apply_event(ev)
                .map_err(|e| format!("invalid event in {event_log_for_errors}: {e}"))?;
        }
    }

    let (view_events, view_bad) = state.view_log.load_all().await?;
    if !view_bad.is_empty() {
        tracing::warn!(
            bad_lines = view_bad.len(),
            "skipped corrupt view JSONL lines"
        );
    }
    {
        let mut view_reduced = state.view_reduced.write().await;
        journal::replay(view_events, &mut *view_reduced, |r, ev| r.apply(ev));
    }

    let app: Router = create_app(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%addr, "listening");
    eprintln!("[boot] binding {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    eprintln!("[boot] server exited");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                eprintln!("[shutdown] received SIGTERM");
                tracing::info!("shutdown: sigterm");
            }
            _ = sigint.recv() => {
                eprintln!("[shutdown] received SIGINT");
                tracing::info!("shutdown: sigint");
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("[shutdown] ctrl_c");
        tracing::info!("shutdown");
    }
}

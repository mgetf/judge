use judge::app::{serve_judge, JudgeOptions};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let opts = JudgeOptions::from_env().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    eprintln!(
        "[boot] judge listening on {} (discord api {})",
        opts.bind, opts.discord.api_base
    );
    let server = serve_judge(opts).await?;
    eprintln!("[boot] bound {}", server.addr);
    tokio::signal::ctrl_c().await?;
    server.abort();
    Ok(())
}

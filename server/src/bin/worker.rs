use anyhow::Result;
use nabu_server::{
    config::WorkerConfig,
    mail::{SmtpMailSender, run_worker},
};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = WorkerConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    let sender = SmtpMailSender::new(&config.smtp)?;
    tracing::info!("email worker ready");
    run_worker(&pool, &sender, config.auth_secret.as_bytes()).await?;

    Ok(())
}

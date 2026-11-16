use scoreboard_master::Application;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let app = match Application::new() {
        Ok(app) => app,
        Err(error) => {
            tracing::error!("Application failed: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = app.run().await {
        tracing::error!("Application failed: {error}");
        std::process::exit(1);
    }
}

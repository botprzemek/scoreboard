use scoreboard_master::ApplicationBuilder;

#[tokio::main]
async fn main() {
    ApplicationBuilder::new().build().run().await;
}

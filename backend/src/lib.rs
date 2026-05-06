mod app;
pub mod bot;
pub mod bot_trainer;
pub mod core;
pub mod projection;
pub mod room_scoring;
pub mod rules;
pub mod scoring;

pub async fn run_server() -> anyhow::Result<()> {
    app::server::run().await
}

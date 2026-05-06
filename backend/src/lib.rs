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

pub async fn run_from_env() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("admin") {
        return run_admin_command(&args[1..]);
    }
    run_server().await
}

fn run_admin_command(args: &[String]) -> anyhow::Result<()> {
    match args {
        [command, flag, count] if command == "create-invite" && flag == "--count" => {
            let count = count.parse::<usize>()?;
            create_invite_codes(count)
        }
        _ => Err(anyhow::anyhow!(
            "unsupported admin command; expected: admin create-invite --count N"
        )),
    }
}

fn create_invite_codes(count: usize) -> anyhow::Result<()> {
    let settings = app::Settings::from_env()?;
    let db = app::persistence::Database::open(&settings.database_path)?;
    for _ in 0..count {
        let code = app::auth::generate_invite_code();
        db.create_invite_code(&code, &app::now_iso(), None)?;
        println!("{code}");
    }
    Ok(())
}

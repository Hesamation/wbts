use clap::Parser;
use wbts_cli::{App, Engine, live::Live};

fn main() {
    if let Err(error) = run() {
        eprintln!("Stopped: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let app = App::parse();
    app.validate()?;
    let live = Live::connect(&app)?;
    let mut engine = Engine::new(live, &app)?;
    engine.execute(app.command)
}

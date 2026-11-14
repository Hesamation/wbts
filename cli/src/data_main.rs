use clap::Parser;
fn main() {
    if let Err(error) = wbts_cli::data::run(wbts_cli::data::DataApp::parse()) {
        eprintln!("Stopped: {error:#}");
        std::process::exit(1);
    }
}

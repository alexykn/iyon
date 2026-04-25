#[tokio::main]
async fn main() {
    if let Err(error) = iyon_tui::run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

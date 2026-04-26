#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    if let Err(err) = canopus::cli::run(std::env::args().collect()).await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

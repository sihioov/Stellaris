fn main() {
    if let Err(err) = canopus::cli::run(std::env::args().collect()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

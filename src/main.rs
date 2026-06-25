#[tokio::main]
async fn main() {
    if let Err(err) = screwball::app::run().await {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}

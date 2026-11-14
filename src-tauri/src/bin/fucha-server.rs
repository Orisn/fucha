// Runs the local API on its own so the UI can be developed in a browser.
#[tokio::main]
async fn main() {
    if let Err(error) = fucha_lib::serve().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

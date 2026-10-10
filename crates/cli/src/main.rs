mod commands;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    commands::run().await
}

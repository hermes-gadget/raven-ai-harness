#[tokio::main]
async fn main() -> anyhow::Result<()> {
    odin_cli::run(true).await
}

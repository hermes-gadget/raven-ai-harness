//! Backward-compatible `odin` command alias.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    odin_cli::run(false).await
}

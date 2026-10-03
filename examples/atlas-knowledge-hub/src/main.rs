use anyhow::Result;
use atlas::{build_state, ModelDeps, Settings};

#[tokio::main]
async fn main() -> Result<()> {
    let settings = Settings::from_env();
    let models = ModelDeps::ollama(&settings);
    let state = build_state(settings, models).await?;
    println!("Atlas engine ready (collection halcyon, admin key minted).");
    drop(state);
    Ok(())
}

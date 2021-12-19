use anyhow::{Context, Result};
use cargo_metadata::Package;
use tokio::task::spawn_blocking;

pub async fn fetch_ws_crates() -> Result<Vec<Package>> {
    spawn_blocking(|| -> Result<_> {
        let res = cargo_metadata::MetadataCommand::new()
            .no_deps()
            .exec()
            .context("failed to run `cargo metadata`")?;
        let packages = res.packages;
        let members = res.workspace_members;

        let ws_packages = packages
            .into_iter()
            .filter(|p| members.iter().any(|pid| *pid == p.id))
            .collect::<Vec<_>>();

        Ok(ws_packages)
    })
    .await
    .expect("failed to fetch metadata")
}

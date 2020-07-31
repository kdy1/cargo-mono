use anyhow::{Context, Result};
use semver::Version;
use std::time::Duration;

/// Fetches the current version from crates.io
pub async fn get_published_version(name: &str) -> Result<Version> {
    let client =
        crates_io_api::AsyncClient::new("cargo-mono (kdy1997.dev@gmail.com)", Duration::new(1, 0))
            .context("failed to create a client for crates.io")?;

    let p = client
        .get_crate(name)
        .await
        .with_context(|| format!("failed to get version of `{}` from crates.io", name))?;

    let versions: Vec<Version> = p
        .versions
        .iter()
        .map(|v| {
            Version::parse(&v.num)
                .with_context(|| format!("failed to parse `{}` as a version of `{}`", v.num, name))
        })
        .collect::<Result<Vec<_>>>()?;

    let ver = versions.iter().max().cloned();

    Ok(ver.expect("version should exist"))
}

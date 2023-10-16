use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use cargo_metadata::Package;
use futures_util::future::join_all;
use semver::Version;

pub async fn get_published_versions(
    names: &[&str],
    allow_not_found: bool,
) -> Result<HashMap<String, Version>> {
    let mut futures = vec![];
    for &name in names {
        futures.push(fetch_published_version(name, allow_not_found));
    }
    let results = join_all(futures).await;

    if results.iter().any(|res| res.is_err()) {
        let errors: String = results
            .into_iter()
            .filter_map(Result::err)
            .map(|err| format!("{:?}", err))
            .collect();

        bail!("failed to get version of crates: \n{}", errors);
    }

    Ok(results
        .into_iter()
        .map(Result::unwrap)
        .enumerate()
        .map(|(idx, v)| (names[idx].to_string(), v))
        .collect())
}

/// Fetches the current version from crates.io
async fn fetch_published_version(name: &str, allow_not_found: bool) -> Result<Version> {
    let index = crates_index::GitIndex::new_cargo_default()
        .context("failed to read the git index for crates.io")?;

    let pkg = match index.crate_(name) {
        Some(v) => v,
        None => {
            if !allow_not_found {
                bail!("failed to find crate {}", name)
            }
            return Ok(Version::new(0, 0, 0));
        }
    };

    let v = pkg.highest_version();
    Ok(v.version()
        .parse()
        .with_context(|| format!("failed to parse version of {} ({})", name, v.version()))?)
}

pub fn can_publish(p: &Package) -> bool {
    // Skip if publish is false
    match &p.publish {
        Some(v) if v.is_empty() => return false,
        _ => {}
    }

    for d in &p.dependencies {
        if d.req.to_string() == "*" {
            return false;
        }
    }

    true
}

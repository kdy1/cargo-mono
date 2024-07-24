use anyhow::{Context, Result};
use cargo_metadata::Package;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use semver::Version;
use serde::Deserialize;

/// Fetches the current version from crates.io
pub async fn fetch_published_version(package_name: &str, allow_not_found: bool) -> Result<Version> {
    static CACHE: Lazy<DashMap<String, Version>> = Lazy::new(DashMap::new);

    if let Some(v) = CACHE.get(package_name) {
        return Ok(v.clone());
    }

    let body = reqwest::get(&build_url(package_name)).await?.text().await?;

    let mut v = body
        .lines()
        .into_iter()
        .filter_map(|line| {
            let desc = serde_json::from_str::<Descriptor>(&line);
            let line = match desc {
                Ok(v) => v,
                Err(err) => {
                    return Some(Err(anyhow::anyhow!(
                        "failed to parse line: {:?}\n{}",
                        err,
                        line
                    )))
                }
            };

            Some(Ok(line.vers))
        })
        .collect::<Result<Vec<_>>>()
        .with_context(|| format!("failed to parse index of {}", package_name))?;

    v.sort_by(|a, b| b.cmp(a));

    if allow_not_found && v.is_empty() {
        CACHE.insert(package_name.to_string(), Version::new(0, 0, 0));
        return Ok(Version::new(0, 0, 0));
    }

    CACHE.insert(package_name.to_string(), v[0].clone());
    Ok(v[0].clone())
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

fn build_url(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    match name.len() {
        1 => format!("https://index.crates.io/1/{name}"),
        2 => format!("https://index.crates.io/2/{name}"),
        3 => {
            let first_char = name.chars().next().unwrap();
            format!("https://index.crates.io/3/{first_char}/{name}")
        }
        _ => {
            let first_two = &name[0..2];
            let second_two = &name[2..4];

            format!("https://index.crates.io/{first_two}/{second_two}/{name}",)
        }
    }
}

#[derive(Debug, Deserialize)]
struct Descriptor {
    pub vers: Version,
}

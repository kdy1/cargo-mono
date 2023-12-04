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
    let body = reqwest::get(&build_url(package_name)).await?.text().await?;

    let mut v = body
        .lines()
        .into_iter()
        .filter_map(|line| {
            let desc = serde_json::from_str::<Descriptor>(&line);
            let line = match desc {
                Ok(v) => v,
                Err(err) => return Some(Err(anyhow!("failed to parse line: {:?}\n{}", err, line))),
            };

            if !constraints.matches(&line.vers) {
                return None;
            }

            Some(Ok(PackageVersion {
                name: line.name,
                version: line.vers,
                deps: line
                    .deps
                    .into_iter()
                    .filter(|dep| dep.kind == "normal")
                    .map(|d| Dependency {
                        name: d.package.unwrap_or(d.name),
                        constraints: d.req,
                    })
                    .collect(),
            }))
        })
        .collect::<Result<Vec<_>>>()
        .with_context(|| format!("failed to parse index of {}", package_name))?;

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

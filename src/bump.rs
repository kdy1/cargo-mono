use crate::info::fetch_ws_crates;
use anyhow::{Context, Result};
use cargo_metadata::{Package, PackageId};
use clap::ArgMatches;
use semver::Version;
use std::collections::HashSet;

pub async fn run<'a>(matches: &ArgMatches<'a>) -> Result<()> {
    let ws_crates = fetch_ws_crates().await?;

    let crate_to_bump = matches
        .value_of_lossy("crate")
        .expect("crate name is required argument");

    // Get list of crates to bump
    let mut dependants = Default::default();
    public_dependants(&mut dependants, &ws_crates, &crate_to_bump);

    dbg!(&dependants);

    Ok(())
}

/// This is recursive and returned value does not contain original crate itself.
///
/// **Note**:
///  - Package is excluded if `publish` is [false].
fn public_dependants(dependants: &mut HashSet<String>, packages: &[Package], crate_to_bump: &str) {
    for p in packages {
        // Skip if publish is false
        match &p.publish {
            Some(v) if v.is_empty() => continue,
            _ => {}
        }

        for p in packages {
            if dependants.contains(&p.name) {
                continue;
            }

            if p.name == crate_to_bump {
                continue;
            }

            for dep in &p.dependencies {
                if dep.name == crate_to_bump {
                    eprintln!("{} depends on {}", p.name, dep.name);

                    dependants.insert(p.name.clone());
                    public_dependants(dependants, packages, &p.name)
                }
            }
        }
    }
}

fn calc_bumped_version(previous: &str, breaking: bool) -> Result<Version> {
    let mut v: Version = previous
        .parse()
        .context("failed to parse previous version")?;

    // Semver treats 0.x specially
    if v.major == 0 {
        if breaking {
            v.increment_minor();
        } else {
            v.increment_patch();
        }
    } else {
        if breaking {
            v.increment_major()
        } else {
            v.increment_patch();
        }
    }

    Ok(v)
}

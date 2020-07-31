use crate::info::fetch_ws_crates;
use crate::util::get_published_version;
use anyhow::bail;
use anyhow::{Context, Result};
use cargo_metadata::Package;
use clap::ArgMatches;
use semver::Version;
use std::collections::HashSet;

pub async fn run<'a>(matches: &ArgMatches<'a>) -> Result<()> {
    let packages = fetch_ws_crates().await?;

    let crate_to_bump = matches
        .value_of_lossy("crate")
        .expect("crate name is required argument");

    let main = match packages.iter().find(|p| p.name == crate_to_bump) {
        None => bail!("Package {} is not a member of workspace", crate_to_bump),
        Some(v) => v.clone(),
    };

    let breaking = matches.is_present("breaking");

    patch(&main, breaking)
        .await
        .with_context(|| format!("failed to patch {}", crate_to_bump))?;

    if breaking {
        // Get list of crates to bump
        let mut dependants = Default::default();
        public_dependants(&mut dependants, &packages, &crate_to_bump);

        for dep in &dependants {
            match packages.iter().find(|p| p.name == &**dep) {
                None => bail!("Package {} is not a member of workspace", crate_to_bump),
                Some(v) => {
                    patch(v, breaking)
                        .await
                        .with_context(|| format!("failed to patch {}", v.name))?;
                }
            };
        }
    }

    Ok(())
}

async fn patch(package: &Package, breaking: bool) -> Result<()> {
    let previous = get_published_version(&package.name)
        .await
        .context("failed to get published version from crates.io")?;
    let new_version = calc_bumped_version(previous.clone(), breaking)?;

    eprintln!("Package({}): {} -> {}", package.name, previous, new_version);

    unimplemented!()
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

fn calc_bumped_version(mut v: Version, breaking: bool) -> Result<Version> {
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

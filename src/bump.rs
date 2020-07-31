use crate::info::fetch_ws_crates;
use crate::util::get_published_version;
use anyhow::bail;
use anyhow::{Context, Result};
use cargo_metadata::Package;
use clap::ArgMatches;
use futures_util::future::{BoxFuture, FutureExt};
use semver::Version;
use std::collections::HashMap;
use std::fs::{read_to_string, write};
use std::sync::Arc;
use tokio::task::spawn_blocking;

pub async fn run<'a>(matches: &ArgMatches<'a>) -> Result<()> {
    let ws_packages = fetch_ws_crates().await?;

    let crate_to_bump = matches
        .value_of_lossy("crate")
        .expect("crate name is required argument");

    let main = match ws_packages.iter().find(|p| p.name == crate_to_bump) {
        None => bail!("Package {} is not a member of workspace", crate_to_bump),
        Some(v) => v.clone(),
    };

    let breaking = matches.is_present("breaking");

    // Get list of crates to bump
    let mut dependants = Default::default();
    public_dependants(&mut dependants, &ws_packages, &crate_to_bump, breaking).await?;
    dbg!(&dependants);
    let dependants = Arc::new(dependants);

    patch(main.clone(), dependants.clone())
        .await
        .with_context(|| format!("failed to patch {}", crate_to_bump))?;

    if breaking {
        for dep in dependants.keys() {
            match ws_packages.iter().find(|p| p.name == &**dep) {
                None => bail!("Package {} is not a member of workspace", crate_to_bump),
                Some(v) => {
                    patch(v.clone(), dependants.clone())
                        .await
                        .with_context(|| format!("failed to patch {}", v.name))?;
                }
            };
        }
    }

    Ok(())
}

async fn patch(package: Package, deps_to_bump: Arc<HashMap<String, Version>>) -> Result<()> {
    eprintln!(
        "Package({}) -> {}",
        package.name, deps_to_bump[&package.name]
    );

    spawn_blocking(move || -> Result<_> {
        let toml = read_to_string(&package.manifest_path).context("failed to read error")?;

        let mut doc = toml
            .parse::<toml_edit::Document>()
            .context("toml file is invalid")?;

        // Bump version of package itself
        let v = deps_to_bump[&package.name].to_string();
        doc["package"]["version"] = toml_edit::value(&*v);

        // Bump version of dependencies
        let deps_section = &mut doc["dependencies"];
        if !deps_section.is_none() {
            //
            let table = deps_section.as_inline_table_mut();
            if let Some(table) = table {
                for (dep_to_bump, new_version) in deps_to_bump.iter() {
                    if table.contains_key(&dep_to_bump) {
                        *table.get_mut(&**dep_to_bump).unwrap() =
                            toml_edit::value(&*new_version.to_string())
                                .as_value()
                                .cloned()
                                .unwrap();
                    }
                }
            }
        }

        write(&package.manifest_path, doc.to_string())
            .context("failed to save modified Cargo.toml")?;

        Ok(())
    })
    .await
    .expect("failed to edit toml file")
}

/// This is recursive and returned value does not contain original crate itself.
fn public_dependants<'a>(
    dependants: &'a mut HashMap<String, Version>,
    packages: &'a [Package],
    crate_to_bump: &'a str,
    breaking: bool,
) -> BoxFuture<'a, Result<()>> {
    eprintln!("Calculating dependants of `{}`", crate_to_bump);
    // eprintln!(
    //     "Packages: {:?}",
    //     packages.iter().map(|v| &*v.name).collect::<Vec<_>>()
    // );

    async move {
        for p in packages {
            // Skip if publish is false
            match &p.publish {
                Some(v) if v.is_empty() => continue,
                _ => {}
            }

            if dependants.contains_key(&p.name) {
                continue;
            }

            if p.name == crate_to_bump {
                let previous = get_published_version(&crate_to_bump)
                    .await
                    .context("failed to get published version from crates.io")?;
                let new_version = calc_bumped_version(previous.clone(), breaking)?;

                dependants.insert(p.name.clone(), new_version);
                continue;
            }

            if breaking {
                for dep in &p.dependencies {
                    if dep.name == crate_to_bump {
                        eprintln!("{} depends on {}", p.name, dep.name);

                        public_dependants(dependants, packages, &p.name, breaking).await?;
                    }
                }
            }
        }

        Ok(())
    }
    .boxed()
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

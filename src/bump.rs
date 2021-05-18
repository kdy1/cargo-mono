use crate::info::fetch_ws_crates;
use crate::util::{can_publish, get_published_versions};
use anyhow::bail;
use anyhow::{Context, Result};
use cargo_metadata::Package;
use futures_util::future::{BoxFuture, FutureExt};
use semver::Version;
use std::collections::HashMap;
use std::fs::{read_to_string, write};
use std::sync::Arc;
use structopt::StructOpt;
use tokio::task::spawn_blocking;
use toml_edit::{Item, Value};

/// "Bump versions of a crate and dependant crates.
///
/// The command ensures that the version is bumped compared to **the published
/// version on crates.io**,

#[derive(Debug, StructOpt)]
pub struct BumpCommand {
    /// Name of the crate to bump version
    #[structopt(name = "crate")]
    pub crate_name: String,

    /// True if it's a breaking change.
    #[structopt(long)]
    pub breaking: bool,

    /// Bump version of dependants and update requirements.
    ///
    /// Has effect only if `breaking` is false.
    #[structopt(short = "D", long)]
    pub with_dependants: bool,
}

impl BumpCommand {
    pub async fn run(&self) -> Result<()> {
        let ws_packages = fetch_ws_crates().await?;

        let crate_names = ws_packages.iter().map(|p| &*p.name).collect::<Vec<_>>();
        let published_versions = get_published_versions(&crate_names).await?;

        let crate_to_bump = &*self.crate_name;

        let main = match ws_packages.iter().find(|p| p.name == crate_to_bump) {
            None => bail!("Package {} is not a member of workspace", crate_to_bump),
            Some(v) => v.clone(),
        };

        // Get list of crates to bump
        let mut dependants = Default::default();
        public_dependants(
            &mut dependants,
            &published_versions,
            &ws_packages,
            &crate_to_bump,
            self.breaking,
            self.with_dependants,
        )
        .await?;
        let dependants = Arc::new(dependants);

        if self.breaking || self.with_dependants {
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
        } else {
            patch(main.clone(), dependants.clone())
                .await
                .with_context(|| format!("failed to patch {}", crate_to_bump))?;
        }

        Ok(())
    }
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

        {
            // Bump version of package itself
            let v = deps_to_bump[&package.name].to_string();
            doc["package"]["version"] = toml_edit::value(&*v);
        }

        // Bump version of dependencies
        for &dep_type in &["dependencies", "dev-dependencies", "build-dependencies"] {
            let deps_section = &mut doc[dep_type];
            if !deps_section.is_none() {
                //
                let table = deps_section.as_table_mut();
                if let Some(table) = table {
                    for (dep_to_bump, new_version) in deps_to_bump.iter() {
                        if table.contains_key(&dep_to_bump) {
                            let prev: &mut toml_edit::Item = &mut table[dep_to_bump];

                            let new_version = toml_edit::value(new_version.to_string());
                            // We should handle object like
                            //
                            // { version = "0.1", path = "./macros" }

                            match prev {
                                Item::None => {
                                    unreachable!("{}.{} cannot be none", dep_type, dep_to_bump,)
                                }
                                Item::Value(v) => match v {
                                    Value::String(_) => {
                                        *v = new_version.as_value().unwrap().clone()
                                    }
                                    Value::InlineTable(v) => {
                                        *v.get_mut("version").expect("should have version") =
                                            new_version.as_value().unwrap().clone();
                                    }
                                    _ => unreachable!(
                                        "{}.{}: cannot be unknown type {:?}",
                                        dep_type, dep_to_bump, prev
                                    ),
                                },
                                Item::Table(_) => {}
                                Item::ArrayOfTables(_) => unreachable!(
                                    "{}.{} cannot be array of table",
                                    dep_type, dep_to_bump
                                ),
                            }
                        }
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
    published_versions: &'a HashMap<String, Version>,
    packages: &'a [Package],
    crate_to_bump: &'a str,
    breaking: bool,
    with_dependants: bool,
) -> BoxFuture<'a, Result<()>> {
    eprintln!("Calculating dependants of `{}`", crate_to_bump);
    // eprintln!(
    //     "Packages: {:?}",
    //     packages.iter().map(|v| &*v.name).collect::<Vec<_>>()
    // );

    async move {
        for p in packages {
            if !can_publish(&p) {
                continue;
            }

            if dependants.contains_key(&p.name) {
                continue;
            }

            if p.name == crate_to_bump {
                let previous = published_versions[&p.name].clone();
                let new_version = calc_bumped_version(previous, breaking)?;

                dependants.insert(p.name.clone(), new_version);
                continue;
            }

            if breaking || with_dependants {
                for dep in &p.dependencies {
                    if dep.name == crate_to_bump {
                        eprintln!("{} depends on {}", p.name, dep.name);

                        public_dependants(
                            dependants,
                            published_versions,
                            packages,
                            &p.name,
                            breaking,
                            with_dependants,
                        )
                        .await?;
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

use crate::{
    info::fetch_ws_crates,
    util::{can_publish, get_published_versions},
};
use anyhow::{bail, Context, Result};
use cargo_metadata::Package;
use futures_util::future::{BoxFuture, FutureExt};
use requestty::{prompt_one, Answer, Question};
use semver::Version;
use std::{
    collections::HashMap,
    fs::{read_to_string, write},
    path::Path,
    sync::Arc,
};
use structopt::StructOpt;
use tokio::{process::Command, task::spawn_blocking};
use toml_edit::{Item, Value};
use walkdir::WalkDir;

/// Bump versions of a crate and dependant crates.
///
/// The command ensures that the version is bumped compared to **the published
/// version on crates.io**,

#[derive(Debug, StructOpt)]
pub struct BumpCommand {
    /// Name of the crate to bump version
    #[structopt(name = "crate", required_unless = "interactive")]
    pub crate_name: Option<String>,

    /// Run in interactive mode
    #[structopt(short = "i", long)]
    pub interactive: bool,

    /// True if it's a breaking change.
    #[structopt(long)]
    pub breaking: bool,

    /// Bump version of dependants and update requirements.
    ///
    /// Has effect only if `breaking` is false.
    #[structopt(short = "D", long)]
    pub with_dependants: bool,

    /// Commit with the messahe `Bump version`.
    #[structopt(short = "g", long)]
    pub git: bool,
}

impl BumpCommand {
    async fn get_crates_to_bump(&self, crates: &[Package]) -> Result<Vec<String>> {
        if let Some(n) = &self.crate_name {
            return Ok(vec![n.clone()]);
        }

        let q = Question::multi_select("crates")
            .message("Select crates to bump version")
            .choices(crates.iter().map(|p| &*p.name));

        let answer = prompt_one(q).context("failed to prompt")?;

        match answer {
            Answer::ListItems(v) => Ok(v.into_iter().map(|v| v.text).collect()),
            _ => {
                bail!(
                    "Expected answer of type `Answer::ListItems`, got {:?}",
                    answer
                )
            }
        }
    }

    pub async fn run(&self) -> Result<()> {
        let workspace_crates = fetch_ws_crates().await?;

        let crate_names = workspace_crates
            .iter()
            .map(|p| &*p.name)
            .collect::<Vec<_>>();
        let published_versions = get_published_versions(&crate_names).await?;

        let publishable_crates = workspace_crates
            .iter()
            .filter(|p| published_versions.contains_key(&p.name))
            .cloned()
            .collect::<Vec<_>>();

        let crates_to_bump = self
            .get_crates_to_bump(&publishable_crates)
            .await
            .context("failed to get crates to bump")?;

        for crate_to_bump in crates_to_bump {
            let main = match workspace_crates.iter().find(|p| p.name == crate_to_bump) {
                None => bail!("Package {} is not a member of workspace", crate_to_bump),
                Some(v) => v.clone(),
            };

            // Get list of crates to bump
            let mut dependants = Default::default();
            public_dependants(
                self.interactive,
                &mut dependants,
                &published_versions,
                &workspace_crates,
                &crate_to_bump,
                self.breaking,
                self.with_dependants,
            )
            .await?;
            let dependants = Arc::new(dependants);

            if self.breaking || self.with_dependants {
                for dep in dependants.keys() {
                    match workspace_crates.iter().find(|p| p.name == &**dep) {
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
        }

        generate_lockfile()
            .await
            .context("failed to update `Cargo.lock`")?;

        if self.git {
            git_commit().await.context("failed to commit using git")?;
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
    interactive: bool,
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
                            interactive,
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

async fn generate_lockfile() -> Result<()> {
    Command::new("cargo")
        .arg("generate-lockfile")
        .status()
        .await
        .context("failed to generate lockfile")?;

    Ok(())
}

async fn git_commit() -> Result<()> {
    let mut files = vec![];
    for e in WalkDir::new(".") {
        let e = e?;

        if e.path().is_file() {
            if let Some(name) = e.path().file_name() {
                if name == "Cargo.lock" || name == "Cargo.toml" {
                    files.push(e.path().to_path_buf());
                }
            }
        }
    }

    let mut cmd = Command::new("git");
    cmd.arg("commit");

    for file in &*files {
        if !is_ignored_by_git(&file).await? {
            cmd.arg(file);
        }
    }

    cmd.arg("-m").arg("Bump version");

    cmd.status().await.context("failed to run git")?;

    Ok(())
}

async fn is_ignored_by_git(path: &Path) -> Result<bool> {
    Command::new("git")
        .arg("check-ignore")
        .arg(path)
        .output()
        .await
        .map(|output| output.status.success())
        .context("failed to run git")
}

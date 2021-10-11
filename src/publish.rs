use crate::info::fetch_ws_crates;
use crate::util::can_publish;
use crate::util::get_published_versions;
use anyhow::bail;
use anyhow::{Context, Result};
use cargo_metadata::{Package, PackageId};
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use semver::Version;
use std::collections::HashMap;
use std::{process::Stdio, time::Duration};
use structopt::StructOpt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::{spawn, time::sleep};

/// Publishes crates and its dependencies.
#[derive(Debug, StructOpt)]
pub struct PublishCommand {
    /// Name of the crate to publish.
    #[structopt(name = "crate", default_value = "*")]
    pub crate_name: String,

    /// Allow publishing only dependencies
    #[structopt(long)]
    pub allow_only_deps: bool,

    /// Skip verification.
    #[structopt(long)]
    pub no_veirfy: bool,
}

impl PublishCommand {
    pub async fn run(&self) -> Result<()> {
        let ws_packages = fetch_ws_crates().await?;
        let ws_packages = ws_packages
            .into_iter()
            .filter(can_publish)
            .collect::<Vec<_>>();

        let crate_names = ws_packages.iter().map(|s| &*s.name).collect::<Vec<_>>();

        let published_versions = get_published_versions(&crate_names).await?;

        let target_crate = &*self.crate_name;
        let allow_only_deps = self.allow_only_deps;
        let graph = dependency_graph(&ws_packages, &target_crate);

        if !allow_only_deps {
            let p = ws_packages.iter().find(|p| p.name == target_crate);
            if let Some(p) = p {
                let published_version = published_versions[&p.name].clone();

                if published_version >= p.version {
                    bail!("version of `{}` is same as published version", p.name)
                }
            }
        }

        let packages: Vec<&PackageId> = match toposort(&graph, None) {
            Ok(v) => v,
            Err(e) => bail!("circular dependency detected: {:?}", e),
        };

        for p in packages {
            let pkg = ws_packages.iter().find(|ws_pkg| ws_pkg.id == *p);

            if let Some(pkg) = pkg {
                publish_if_possible(
                    pkg,
                    &published_versions,
                    PublishOpts {
                        no_verify: self.no_veirfy,
                    },
                )
                .await
                .context("failed to publish")?;
            }
        }

        Ok(())
    }
}
async fn publish_if_possible(
    package: &Package,
    published_versions: &HashMap<String, Version>,
    opts: PublishOpts,
) -> Result<()> {
    eprintln!("Checking if `{}` should be published", package.name);

    let published_version = &published_versions[&package.name];

    if *published_version < package.version {
        publish(package, opts).await.context("failed to publish")?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]

struct PublishOpts {
    no_verify: bool,
}

async fn publish(p: &Package, opts: PublishOpts) -> Result<()> {
    sleep(Duration::new(5, 0)).await;

    eprintln!("Publishing `{}`", p.name);

    let mut cmd = Command::new("cargo");

    cmd.arg("publish");
    if opts.no_verify {
        cmd.arg("--no-verify");
    }

    let mut process: Child = cmd
        .arg("--color")
        .arg("always")
        .arg("--manifest-path")
        .arg(&p.manifest_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn cargo publish")?;

    let stderr = process.stderr.take().unwrap();
    let mut reader = BufReader::new(stderr).lines();

    // Ensure the child process is spawned in the runtime so it can
    // make progress on its own while we await for any output.
    spawn(async move {
        let status = process
            .wait()
            .await
            .expect("child process encountered an error");

        println!("child status was: {}", status);
    });

    while let Some(line) = reader.next_line().await? {
        println!("{}", line);
    }

    Ok(())
}

/// `packages` should contain only workspace members.
fn dependency_graph<'a>(packages: &'a [Package], target: &str) -> DiGraphMap<&'a PackageId, usize> {
    let mut graph = DiGraphMap::new();

    for p in packages {
        let pkg_node = graph.add_node(&p.id);

        for dep in &p.dependencies {
            let dep_pkg = packages.iter().find(|p| p.name == dep.name);

            // Local dependency
            if let Some(dep_pkg) = dep_pkg {
                let dep_node = graph.add_node(&dep_pkg.id);

                graph.add_edge(dep_node, pkg_node, 1);
            }

            if p.name == target {
                break;
            }
        }
        //
    }

    graph
}

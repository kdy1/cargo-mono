use crate::info::fetch_ws_crates;
use crate::util::{can_publish, get_published_version};
use anyhow::bail;
use anyhow::{Context, Result};
use cargo_metadata::{Package, PackageId};
use clap::ArgMatches;
use petgraph::algo::toposort;
use petgraph::graphmap::DiGraphMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::spawn;

pub async fn run<'a>(matches: &ArgMatches<'a>) -> Result<()> {
    let ws_packages = fetch_ws_crates().await?;
    let ws_packages = ws_packages
        .into_iter()
        .filter(can_publish)
        .collect::<Vec<_>>();

    let target_crate = matches.value_of("crate").unwrap_or_default();
    let allow_only_deps = matches.is_present("allow-only-deps");
    let graph = dependency_graph(&ws_packages, &target_crate);

    if !allow_only_deps {
        let p = ws_packages.iter().find(|p| p.name == target_crate);
        if let Some(p) = p {
            let published_version = get_published_version(&p.name)
                .await
                .context("failed to determine if a crate should be published")?;

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
            publish_if_possible(pkg)
                .await
                .context("failed to publish")?;
        }
    }

    Ok(())
}

async fn publish_if_possible(package: &Package) -> Result<()> {
    eprintln!("Checking if `{}` should be published", package.name);

    let published_version = get_published_version(&package.name)
        .await
        .context("failed to determine if a crate should be published")?;

    if published_version < package.version {
        publish(package).await.context("failed to publish")?;
    }

    Ok(())
}

async fn publish(p: &Package) -> Result<()> {
    eprintln!("Publishing `{}`", p.name);

    let mut process: Child = Command::new("cargo")
        .arg("publish")
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
    spawn(async {
        let status = process.await.expect("child process encountered an error");

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

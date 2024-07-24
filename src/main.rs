use std::env;

use anyhow::{Context, Result};
use bump::BumpCommand;
use clap::Parser;
use publish::PublishCommand;

mod bump;
mod cargo_workspace;
mod crates_io;
mod publish;

#[derive(Debug, Parser)]
#[clap(author, about)]
enum Command {
    Bump(BumpCommand),
    Publish(PublishCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = {
        let mut args = env::args().collect::<Vec<_>>();

        if env::var("CARGO").is_ok() {
            args.remove(1);
        };

        args
    };

    let cmd = Command::parse_from(args);

    match cmd {
        Command::Bump(cmd) => {
            cmd.run()
                .await
                .context("failed to bump version of a crate")?;
        }
        Command::Publish(cmd) => {
            cmd.run().await.context("failed to publish")?;
        }
    }
    Ok(())
}

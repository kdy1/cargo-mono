use std::env;

use anyhow::{Context, Result};
use bump::BumpCommand;
use publish::PublishCommand;
use structopt::StructOpt;

mod bump;
mod info;
mod publish;
mod util;

#[derive(Debug, StructOpt)]
#[structopt(author, about)]
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

    let cmd = Command::from_iter(args);

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

mod bump;
mod check;
mod info;
mod publish;
mod util;

use anyhow::Context;
use anyhow::Result;
use clap::{
    app_from_crate, crate_authors, crate_description, crate_name, crate_version, AppSettings, Arg,
    SubCommand,
};

#[tokio::main]
async fn main() -> Result<()> {
    let matches = app_from_crate!()
        .subcommand(
            SubCommand::with_name("bump")
                .about(
                    "Bump versions of a crate and dependant crates
The command ensures that the version is bumped compared to **the published version on crates.io**",
                )
                .arg(
                    Arg::with_name("crate")
                        .help("Name of the crate to bump version")
                        .value_name("CRATE")
                        .required(true),
                )
                .arg(Arg::with_name("breaking").help("Mark ").long("breaking")),
        )
        .subcommand(SubCommand::with_name("check").about("Verify that version is bumped"))
        .subcommand(SubCommand::with_name("publish").about("Publishes crates and its dependencies"))
        .global_setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    if let Some(sub) = matches.subcommand_matches("bump") {
        bump::run(sub).await.context("failed to bump version")?;
    }

    Ok(())
}

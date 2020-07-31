mod bump;
mod check;
mod info;
mod publish;

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
                    Arg::with_name("crates")
                        .help(
                            "Crates to bump version. You don't need to pass names of dependent \
                             crates.",
                        )
                        .value_delimiter(",")
                        .value_name("CRATES")
                        .multiple(true)
                        .required(true),
                ),
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

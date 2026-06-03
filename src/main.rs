use std::process;

use clap::Parser;

use crate::cli::Cli;

mod cli;
mod config;
mod extract;
mod fs_utils;

fn main() {
    let cli = Cli::parse();

    if let Err(err) = cli.run() {
        eprintln!("Something went wrong: {err}");

        process::exit(1);
    }
}

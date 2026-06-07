use std::process;

use clap::Parser;

use crate::cli::Cli;

mod archive;
mod cli;
mod config;
mod extract;
mod theme;
mod util;

fn main() {
    let cli = Cli::parse();

    if let Err(err) = cli.run() {
        eprintln!("{}", err);

        process::exit(1);
    }
}

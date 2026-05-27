use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use anyhow::Context;
use clap::{CommandFactory, Parser, error::ErrorKind};
use thiserror::Error;

use crate::config::Config;
use crate::extract::{ExtractError, extract_archive, is_archive};

mod config;
mod extract;
mod fs_utils;

#[derive(Debug, Parser)]
#[command(
    name = "sax",
    version,
    about = "Smart archiving and extracting utility",
    after_help = "Supported archive formats: zip, tar, tar.gz, tgz, tar.xz, txz, tar.bz2, tbz2, tar.zst, tzst, 7z, rar."
)]
struct Cli {
    /// Archive file to extract.
    archive: PathBuf,
    /// Directory to extract into. Created if it does not exist.
    out: PathBuf,
    /// Strip a single top-level directory when extracting.
    #[arg(long = "strip", action = clap::ArgAction::SetTrue, conflicts_with = "no_strip")]
    strip: bool,
    /// Preserve the archive's top-level directory when extracting.
    #[arg(long = "no-strip", action = clap::ArgAction::SetTrue)]
    no_strip: bool,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("{path} is not an archive")]
    NotArchive { path: PathBuf },
    #[error("could not extract archive {archive} to {out}: {source}")]
    Extract {
        archive: PathBuf,
        out: PathBuf,
        #[source]
        source: ExtractError,
    },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

fn main() {
    if let Err(err) = run(std::env::args().skip(1), &mut io::stdout()) {
        eprintln!("Something went wrong: {err}");

        process::exit(1);
    }
}

fn run<I, S>(args: I, stdout: &mut dyn Write) -> Result<(), AppError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();

    if args.is_empty() {
        let mut help = Vec::new();
        Cli::command()
            .write_long_help(&mut help)
            .context("failed to render help")?;
        stdout.write_all(&help).context("failed to write help")?;
        writeln!(stdout).context("failed to write help")?;
        return Ok(());
    }

    match Cli::try_parse_from(std::iter::once("sax".to_string()).chain(args)) {
        Ok(cli) => {
            let strip_top_level_dir = if cli.strip {
                true
            } else if cli.no_strip {
                false
            } else {
                Config::load_or_create()?.extract_prefs.strip_top_level_dir
            };

            extract(&cli.archive, &cli.out, strip_top_level_dir)
        }
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            write!(stdout, "{err}").context("failed to write CLI output")?;
            Ok(())
        }
        Err(err) => Err(anyhow::Error::from(err).into()),
    }
}

fn extract(archive: &Path, out: &Path, strip_top_level_dir: bool) -> Result<(), AppError> {
    if !is_archive(archive) {
        return Err(AppError::NotArchive {
            path: archive.to_path_buf(),
        });
    }

    extract_archive(archive, out, strip_top_level_dir).map_err(|source| AppError::Extract {
        archive: archive.to_path_buf(),
        out: out.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_prints_usage_without_arguments() {
        let mut stdout = Vec::new();

        run(Vec::<String>::new(), &mut stdout).unwrap();

        let got = String::from_utf8(stdout).unwrap();
        assert!(got.contains("Usage: sax [OPTIONS] <ARCHIVE> <OUT>"));
        assert!(got.contains("Supported archive formats:"));
    }

    #[test]
    fn run_prints_usage_with_help() {
        for flag in ["--help", "-h"] {
            let mut stdout = Vec::new();

            run([flag], &mut stdout).unwrap();

            let got = String::from_utf8(stdout).unwrap();
            assert!(got.contains("Usage: sax [OPTIONS] <ARCHIVE> <OUT>"));
            assert!(got.contains("Supported archive formats:"));
            assert!(got.contains("--strip"));
            assert!(got.contains("--no-strip"));
        }
    }

    #[test]
    fn run_prints_version() {
        let mut stdout = Vec::new();

        run(["--version"], &mut stdout).unwrap();

        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            format!("sax {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}

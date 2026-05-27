use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use clap::{CommandFactory, Parser, error::ErrorKind};

use crate::extract::{SaxResult, extract_archive, is_archive};

mod extract;

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
}

fn main() {
    if let Err(err) = run(std::env::args().skip(1), &mut io::stdout()) {
        eprintln!("Something went wrong: {err}");
        process::exit(1);
    }
}

fn run<I, S>(args: I, stdout: &mut dyn Write) -> SaxResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();

    if args.is_empty() {
        let mut help = Vec::new();
        Cli::command().write_long_help(&mut help)?;
        stdout.write_all(&help)?;
        writeln!(stdout)?;
        return Ok(());
    }

    match Cli::try_parse_from(std::iter::once("sax".to_string()).chain(args)) {
        Ok(cli) => extract(&cli.archive, &cli.out),
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            write!(stdout, "{err}")?;
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

fn extract(archive: &Path, out: &Path) -> SaxResult<()> {
    if !is_archive(archive) {
        return Err(format!("{} is not an archive", archive.display()).into());
    }

    extract_archive(archive, out).map_err(|err| {
        format!(
            "could not extract archive {} to {}: {err}",
            archive.display(),
            out.display()
        )
        .into()
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
        assert!(got.contains("Usage: sax <ARCHIVE> <OUT>"));
        assert!(got.contains("Supported archive formats:"));
    }

    #[test]
    fn run_prints_usage_with_help() {
        for flag in ["--help", "-h"] {
            let mut stdout = Vec::new();

            run([flag], &mut stdout).unwrap();

            let got = String::from_utf8(stdout).unwrap();
            assert!(got.contains("Usage: sax <ARCHIVE> <OUT>"));
            assert!(got.contains("Supported archive formats:"));
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

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
    /// Paths to extract from or archive.
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
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
    #[error("{path} is not a supported archive name")]
    UnsupportedArchiveName { path: PathBuf },
    #[error("{path} is not a folder")]
    NotFolder { path: PathBuf },
    #[error("{path} does not exist")]
    InputDoesNotExist { path: PathBuf },
    #[error("archive already exists: {path}")]
    ArchiveAlreadyExists { path: PathBuf },
    #[error("{0}")]
    InvalidArgs(String),
    #[error("creating archives is not implemented yet")]
    CreateNotImplemented,
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

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Extract {
        archive: PathBuf,
        out: PathBuf,
    },
    Create {
        inputs: Vec<PathBuf>,
        archive: PathBuf,
    },
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
            let action = parse_action(cli.paths)?;

            let strip_top_level_dir = if cli.strip {
                true
            } else if cli.no_strip {
                false
            } else {
                Config::load_or_create()?.extract_prefs.strip_top_level_dir
            };

            match action {
                Action::Extract { archive, out } => extract(&archive, &out, strip_top_level_dir),
                Action::Create { .. } => Err(AppError::CreateNotImplemented),
            }
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

fn parse_action(paths: Vec<PathBuf>) -> Result<Action, AppError> {
    let Some(first) = paths.first() else {
        return Err(AppError::InvalidArgs(
            "expected an archive and output folder, or inputs followed by an archive name"
                .to_string(),
        ));
    };

    if is_archive(first) && first.exists() {
        return parse_extract_action(paths);
    }

    if first.exists() {
        return parse_create_action(paths);
    }

    Err(AppError::InvalidArgs(format!(
        "{} must be an existing archive or input path",
        first.display()
    )))
}

fn parse_extract_action(paths: Vec<PathBuf>) -> Result<Action, AppError> {
    if paths.len() > 2 {
        return Err(AppError::InvalidArgs(
            "extracting expects exactly one archive and one output folder".to_string(),
        ));
    }

    let archive = paths[0].clone();
    let out = paths[1].clone();

    if out.exists() && !out.is_dir() {
        return Err(AppError::NotFolder { path: out });
    }

    Ok(Action::Extract { archive, out })
}

fn parse_create_action(paths: Vec<PathBuf>) -> Result<Action, AppError> {
    if paths.len() < 2 {
        return Err(AppError::InvalidArgs(
            "archiving expects one or more input paths followed by an archive name".to_string(),
        ));
    }

    let out = paths
        .last()
        .expect("paths has at least two entries")
        .clone();
    let inputs = paths[..paths.len() - 1].to_vec();

    for input in &inputs {
        if !input.exists() {
            return Err(AppError::InputDoesNotExist {
                path: input.clone(),
            });
        }
    }

    if out.exists() {
        return Err(AppError::ArchiveAlreadyExists { path: out });
    }

    if !is_archive(&out) {
        return Err(AppError::UnsupportedArchiveName { path: out });
    }

    Ok(Action::Create {
        inputs,
        archive: out,
    })
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
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn run_should_print_usage_when_no_arguments_given() {
        // Arrang
        let mut stdout = Vec::new();

        // Act
        run(Vec::<String>::new(), &mut stdout).unwrap();
        let actual = String::from_utf8(stdout).unwrap();

        // Assert
        assert!(actual.contains("Usage: sax [OPTIONS] [PATH]..."));
        assert!(actual.contains("Supported archive formats:"));
    }

    #[test]
    fn run_should_print_usage_when_help_flag_given() {
        // Arrange
        let mut stdout = Vec::new();

        // Act
        run(["--help"], &mut stdout).unwrap();
        let actual = String::from_utf8(stdout).unwrap();

        // Assert
        assert!(actual.contains("Usage: sax [OPTIONS] [PATH]..."));
        assert!(actual.contains("Supported archive formats:"));
        assert!(actual.contains("--strip"));
        assert!(actual.contains("--no-strip"));
    }

    #[test]
    fn run_should_print_version_when_version_flag_given() {
        // Arrange
        let mut stdout = Vec::new();

        // Act
        run(["--version"], &mut stdout).unwrap();

        // Assert
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            format!("sax {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn parse_action_should_return_err_when_no_paths_given() {
        // Arrange
        let input = vec![];

        // Act
        let actual = parse_action(input);

        // Assert
        assert!(actual.is_err());

        let actual_err = actual.unwrap_err();

        assert!(matches!(actual_err, AppError::InvalidArgs(_)));
    }

    #[test]
    fn parse_action_should_return_error_when_multiple_out_paths_given() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("input.zip");
        File::create(&archive).unwrap();

        // Act
        let actual = parse_action(vec![
            archive,
            dir.path().join("out"),
            dir.path().join("extra"),
        ]);

        // Assert
        assert!(actual.is_err());

        let actual_err = actual.unwrap_err();

        assert!(matches!(actual_err, AppError::InvalidArgs(_)));
    }

    #[test]
    fn parse_action_should_return_error_when_out_dir_is_not_a_dir() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("input.zip");
        let input = dir.path().join("file.txt");
        File::create(&archive).unwrap();
        File::create(&input).unwrap();

        // Act
        let actual = parse_action(vec![archive, input]);

        // Assert
        assert!(actual.is_err());

        let actual_err = actual.unwrap_err();

        assert!(matches!(actual_err, AppError::NotFolder { path: _ }));
    }

    #[test]
    fn parse_action_should_return_extract_action_when_first_path_is_archive() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("input.zip");
        let out = dir.path().join("out");
        File::create(&archive).unwrap();

        // Act
        let actual = parse_action(vec![archive.clone(), out.clone()]);

        // Assert
        assert!(actual.is_ok());

        let actual_action = actual.unwrap();

        assert_eq!(actual_action, Action::Extract { archive, out });
    }

    #[test]
    fn parse_action_should_return_err_when_only_one_file_path_given() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        File::create(&input).unwrap();

        // Act
        let actual = parse_action(vec![input]);

        // Assert
        assert!(actual.is_err());

        let actual_err = actual.unwrap_err();

        assert!(matches!(actual_err, AppError::InvalidArgs(_)));
    }

    #[test]
    fn parse_action_should_return_err_when_one_of_the_inputs_doesnt_exist() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        let second_input = dir.path().join("input2.txt");
        let out = dir.path().join("out.zip");

        File::create(&input).unwrap();

        // Act
        let actual = parse_action(vec![input, second_input, out]);

        // Assert
        assert!(actual.is_err());

        let actual_err = actual.unwrap_err();

        assert!(matches!(
            actual_err,
            AppError::InputDoesNotExist { path: _second_path }
        ));
    }

    #[test]
    fn parse_action_should_return_err_when_output_already_exists() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        let out = dir.path().join("out.zip");

        File::create(&input).unwrap();
        File::create(&out).unwrap();

        // Act
        let actual = parse_action(vec![input, out]);

        // Assert
        assert!(actual.is_err());

        let actual_err = actual.unwrap_err();

        assert!(matches!(
            actual_err,
            AppError::ArchiveAlreadyExists { path: _out }
        ));
    }

    #[test]
    fn parse_action_should_return_err_when_output_is_not_an_archive() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        let out = dir.path().join("out.foo");

        File::create(&input).unwrap();

        // Act
        let actual = parse_action(vec![input, out]);

        // Assert
        assert!(actual.is_err());

        let actual_err = actual.unwrap_err();

        assert!(matches!(
            actual_err,
            AppError::UnsupportedArchiveName { path: _out }
        ));
    }

    #[test]
    fn parse_action_should_return_extract_action_when_last_path_is_archive() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("file.txt");
        let second_input = dir.path().join("file2.txt");
        let out = dir.path().join("input.zip");

        File::create(&input).unwrap();
        File::create(&second_input).unwrap();

        // Act
        let actual = parse_action(vec![input.clone(), second_input.clone(), out.clone()]);

        // Assert
        assert!(actual.is_ok());

        let actual_action = actual.unwrap();

        assert_eq!(
            actual_action,
            Action::Create {
                inputs: vec![input, second_input],
                archive: out
            }
        );
    }
}

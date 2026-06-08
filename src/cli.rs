use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{CommandFactory, Parser};
use itertools::Itertools;
use thiserror::Error;

use crate::archive::{ArchiveError, create_archive};
use crate::config::Config;
use crate::extract::{ExtractError, extract_archive};
use crate::util::fs::is_archive;
use crate::util::prompt::{Prompt, Prompter};

#[derive(Debug, Parser)]
#[command(
    name = Cli::SAX,
    version,
    about = "Smart archiving and extracting utility",
    after_help = Cli::SUPPORTED_ARCHIVE_FORMATS
)]
pub struct Cli {
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
pub enum AppError {
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
    #[error("could not extract archive {archive} to {out}: {source}")]
    Extract {
        archive: PathBuf,
        out: PathBuf,
        #[source]
        source: ExtractError,
    },
    #[error("could not create archive {out}: {source}")]
    Archive {
        out: PathBuf,
        #[source]
        source: ArchiveError,
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

impl Cli {
    pub const SAX: &str = "sax";
    pub const SUPPORTED_ARCHIVE_FORMATS: &str = "Supported archive formats: zip, tar, tar.gz, tgz, tar.xz, txz, tar.bz2, tbz2, tar.zst, tzst, 7z, rar.";

    pub fn run(&self) -> Result<(), AppError> {
        if self.paths.is_empty() {
            Self::print_help(&mut io::stdout())?;
            return Ok(());
        }

        let prompt = Prompter;
        let action = parse_action(self.paths.clone(), &prompt)?;

        let mut config = Config::load_or_create()?;

        if self.strip {
            config.extract_prefs.strip_top_level_dir = true;
        } else if self.no_strip {
            config.extract_prefs.strip_top_level_dir = false;
        }

        match action {
            Action::Extract { archive, out } => {
                extract_archive(&archive, &out, &config.extract_prefs).map_err(|source| {
                    AppError::Extract {
                        archive: archive.to_path_buf(),
                        out: out.to_path_buf(),
                        source,
                    }
                })
            }
            Action::Create { inputs, archive } => {
                let input_paths: Vec<&Path> = inputs.iter().map(|i| i.as_path()).unique().collect();

                create_archive(input_paths, &archive, &config.create_prefs).map_err(|source| {
                    AppError::Archive {
                        out: archive.to_path_buf(),
                        source,
                    }
                })
            }
        }
    }

    fn print_help(stdout: &mut dyn Write) -> Result<(), AppError> {
        let mut help = Vec::new();
        Self::command()
            .write_long_help(&mut help)
            .context("failed to render help")?;
        stdout.write_all(&help).context("failed to write help")?;
        writeln!(stdout).context("failed to write help")?;
        Ok(())
    }
}

fn parse_action(paths: Vec<PathBuf>, prompt: &dyn Prompt) -> Result<Action, AppError> {
    let Some(first) = paths.first() else {
        return Err(AppError::InvalidArgs(
            "Could not parse arguments".to_string(),
        ));
    };

    if is_archive(first) && first.exists() {
        return parse_extract_action(paths, prompt);
    }

    if first.exists() {
        return parse_create_action(paths, prompt);
    }

    Err(AppError::InvalidArgs(
        "Cannot infer desired functionality from arguments.".to_string(),
    ))
}

fn parse_extract_action(paths: Vec<PathBuf>, prompt: &dyn Prompt) -> Result<Action, AppError> {
    if paths.len() > 2 {
        let first = paths.first().unwrap();
        return Err(AppError::InvalidArgs(format!(
            "Could not extract {}, too many arguments.",
            first.display()
        )));
    }

    let archive = paths[0].clone();
    let out = if paths.len() == 1 {
        prompt.path("Where do you want to extract to?")
    } else {
        paths[1].clone()
    };

    if out.exists() && !out.is_dir() {
        return Err(AppError::NotFolder { path: out });
    }

    Ok(Action::Extract { archive, out })
}

fn parse_create_action(paths: Vec<PathBuf>, prompt: &dyn Prompt) -> Result<Action, AppError> {
    let last = paths
        .last()
        .expect("parse_create_action requires at least one path");
    let (inputs, out) = if !last.exists() && is_archive(last) {
        (paths[..paths.len() - 1].to_vec(), last.to_path_buf())
    } else {
        (
            paths,
            prompt.path("Where do you want to create the archive?"),
        )
    };

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

#[cfg(test)]
mod tests {
    use crate::util::prompt::MockPrompt;

    use super::*;
    use kernal::prelude::*;
    use mockall::predicate::eq;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn parse_action_should_return_err_when_no_paths_given() {
        // Arrange
        let input = vec![];
        let prompt = MockPrompt::new();

        // Act
        let actual = parse_action(input, &prompt);

        // Assert
        assert_that!(matches!(actual.unwrap_err(), AppError::InvalidArgs(_))).is_true();
    }

    #[test]
    fn parse_action_should_prompt_for_output_when_single_archive_given() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("input.zip");
        let out = dir.path().join("out");
        let mut prompt = MockPrompt::new();
        prompt.expect_path().once().return_const(out.clone());

        File::create(&archive).unwrap();

        // Act
        let actual = parse_action(vec![archive.clone()], &prompt);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(Action::Extract { archive, out });
    }

    #[test]
    fn parse_action_should_return_error_when_multiple_out_paths_given() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("input.zip");
        let prompt = MockPrompt::new();
        File::create(&archive).unwrap();

        // Act
        let actual = parse_action(
            vec![archive, dir.path().join("out"), dir.path().join("extra")],
            &prompt,
        );

        // Assert
        assert_that!(matches!(actual.unwrap_err(), AppError::InvalidArgs(_))).is_true();
    }

    #[test]
    fn parse_action_should_return_error_when_out_dir_is_not_a_dir() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("input.zip");
        let input = dir.path().join("file.txt");
        let prompt = MockPrompt::new();
        File::create(&archive).unwrap();
        File::create(&input).unwrap();

        // Act
        let actual = parse_action(vec![archive, input], &prompt);

        // Assert
        assert_that!(matches!(
            actual.unwrap_err(),
            AppError::NotFolder { path: _ }
        ))
        .is_true();
    }

    #[test]
    fn parse_action_should_return_extract_action_when_first_path_is_archive() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive = dir.path().join("input.zip");
        let out = dir.path().join("out");
        let prompt = MockPrompt::new();
        File::create(&archive).unwrap();

        // Act
        let actual = parse_action(vec![archive.clone(), out.clone()], &prompt);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(Action::Extract { archive, out });
    }

    #[test]
    fn parse_action_should_prompt_for_archive_when_only_one_input_given() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        let out = dir.path().join("out.zip");
        let mut prompt = MockPrompt::new();
        prompt
            .expect_path()
            .with(eq("Where do you want to create the archive?"))
            .once()
            .return_const(out.clone());
        File::create(&input).unwrap();

        // Act
        let actual = parse_action(vec![input.clone()], &prompt);

        // Assert
        assert_that!(actual).to_value().is_equal_to(Action::Create {
            inputs: vec![input],
            archive: out,
        });
    }

    #[test]
    fn parse_action_should_return_err_when_one_of_the_inputs_doesnt_exist() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        let second_input = dir.path().join("input2.txt");
        let out = dir.path().join("out.zip");
        let prompt = MockPrompt::new();

        File::create(&input).unwrap();

        // Act
        let actual = parse_action(vec![input, second_input, out], &prompt);

        // Assert
        assert_that!(matches!(
            actual.unwrap_err(),
            AppError::InputDoesNotExist { path: _ }
        ))
        .is_true();
    }

    #[test]
    fn parse_action_should_prompt_when_last_path_exists() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        let second_input = dir.path().join("input.zip");
        let out = dir.path().join("out.tar");
        let mut prompt = MockPrompt::new();
        prompt
            .expect_path()
            .with(eq("Where do you want to create the archive?"))
            .once()
            .return_const(out.clone());

        File::create(&input).unwrap();
        File::create(&second_input).unwrap();

        // Act
        let actual = parse_action(vec![input.clone(), second_input.clone()], &prompt);

        // Assert
        assert_that!(actual).to_value().is_equal_to(Action::Create {
            inputs: vec![input, second_input],
            archive: out,
        });
    }

    #[test]
    fn parse_action_should_prompt_when_last_path_is_not_an_archive() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.txt");
        let second_input = dir.path().join("input.foo");
        let out = dir.path().join("out.zip");
        let mut prompt = MockPrompt::new();
        prompt
            .expect_path()
            .with(eq("Where do you want to create the archive?"))
            .once()
            .return_const(out.clone());

        File::create(&input).unwrap();
        File::create(&second_input).unwrap();

        // Act
        let actual = parse_action(vec![input.clone(), second_input.clone()], &prompt);

        // Assert
        assert_that!(actual).to_value().is_equal_to(Action::Create {
            inputs: vec![input, second_input],
            archive: out,
        });
    }

    #[test]
    fn parse_action_should_return_create_action_when_last_path_is_archive() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("file.txt");
        let second_input = dir.path().join("file2.txt");
        let out = dir.path().join("input.zip");
        let prompt = MockPrompt::new();

        File::create(&input).unwrap();
        File::create(&second_input).unwrap();

        // Act
        let actual = parse_action(
            vec![input.clone(), second_input.clone(), out.clone()],
            &prompt,
        );

        // Assert
        assert_that!(actual).to_value().is_equal_to(Action::Create {
            inputs: vec![input, second_input],
            archive: out,
        });
    }
}

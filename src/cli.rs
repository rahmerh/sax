use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::builder::{IntoResettable, Resettable, StyledStr};
use clap::{CommandFactory, Parser};
use itertools::Itertools;
use thiserror::Error;

use crate::archive::{ArchiveError, create_archive};
use crate::config::{Config, ExtractPrefs, OutputModes};
use crate::extract::{ExtractError, extract_archive};
use crate::util::fs::{archive_stem, is_archive};
use crate::util::prompt::{Prompt, Prompter};

struct HelpTemplate;

impl IntoResettable<StyledStr> for HelpTemplate {
    fn into_resettable(self) -> Resettable<StyledStr> {
        color_print::cstr!(
            "\
<bold>{name}</bold> {version}
{about}

{usage-heading}
{tab}{usage}

{all-args}{after-help}
"
        )
        .into_resettable()
    }
}

#[derive(Debug, Parser)]
#[command(
    name = Cli::SAX,
    version,
    about = "Smart archiving and extracting utility",
    after_help = Cli::SUPPORTED_ARCHIVE_FORMATS,
    help_template = HelpTemplate
)]
pub struct Cli {
    /// Paths to extract from or archive to.
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
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
    pub const SUPPORTED_ARCHIVE_FORMATS: &str = "Supported archive formats: zip, tar, tar.gz/tgz, tar.xz/txz, tar.bz2/tbz2, tar.zst/tzst, 7z, rar.";

    pub fn run(&self) -> Result<(), AppError> {
        if self.paths.is_empty() {
            Self::print_help(&mut io::stdout())?;
            return Ok(());
        }

        let config = Config::load_or_create()?;
        let prompt = Prompter;
        let action = parse_action(self.paths.clone(), &prompt, &config)?;

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
            .write_help(&mut help)
            .context("failed to render help")?;

        stdout.write_all(&help).context("failed to write help")?;
        writeln!(stdout).context("failed to write help")?;

        Ok(())
    }
}

fn parse_action(
    paths: Vec<PathBuf>,
    prompt: &dyn Prompt,
    config: &Config,
) -> Result<Action, AppError> {
    let Some(first) = paths.first() else {
        return Err(AppError::InvalidArgs(
            "Could not parse arguments".to_string(),
        ));
    };

    if is_archive(first) && first.exists() {
        return parse_extract_action(paths, prompt, &config.extract_prefs);
    }

    if first.exists() {
        return parse_create_action(paths, prompt);
    }

    Err(AppError::InvalidArgs(
        "Cannot infer desired functionality from arguments.".to_string(),
    ))
}

fn parse_extract_action(
    paths: Vec<PathBuf>,
    prompt: &dyn Prompt,
    extract_prefs: &ExtractPrefs,
) -> Result<Action, AppError> {
    let archive = paths[0].clone();

    let out = match extract_prefs.output_mode {
        OutputModes::Auto => {
            if paths.len() != 1 {
                return Err(AppError::InvalidArgs(format!(
                    "Could not extract {}, too many arguments.",
                    archive.display()
                )));
            }

            let invalid_output = || {
                AppError::InvalidArgs(format!(
                    "Could not derive output directory from {}.",
                    archive.display()
                ))
            };

            archive
                .parent()
                .ok_or_else(invalid_output)?
                .join(archive_stem(&archive).ok_or_else(invalid_output)?)
        }
        OutputModes::Prompt => {
            if paths.len() == 1 {
                prompt.path("Where do you want to extract to?")
            } else if paths.len() == 2 {
                paths[1].clone()
            } else {
                return Err(AppError::InvalidArgs(format!(
                    "Could not extract {}, too many arguments.",
                    archive.display()
                )));
            }
        }
        OutputModes::Manual => {
            if paths.len() > 2 {
                return Err(AppError::InvalidArgs(format!(
                    "Could not extract {}, too many arguments.",
                    archive.display()
                )));
            }

            if paths.len() == 1 {
                return Err(AppError::InvalidArgs(format!(
                    "Could not extract {}, output directory is required.",
                    archive.display()
                )));
            }

            paths[1].clone()
        }
    };

    if out.exists() && !out.is_dir() {
        return Err(AppError::NotFolder { path: out });
    }

    Ok(Action::Extract { archive, out })
}

fn parse_create_action(paths: Vec<PathBuf>, prompt: &dyn Prompt) -> Result<Action, AppError> {
    let last = paths.last().unwrap();

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
    use crate::{
        config::{CreatePrefs, ExtractPrefs},
        util::prompt::MockPrompt,
    };

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
        let config = Config::default();

        // Act
        let actual = parse_action(input, &prompt, &config);

        // Assert
        assert_that!(matches!(actual.unwrap_err(), AppError::InvalidArgs(_))).is_true();
    }

    #[test]
    fn parse_action_should_return_error_when_multiple_out_paths_given_in_auto_mode() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let prompt = MockPrompt::new();
        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Auto,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(
            vec![
                archive,
                temp_dir.path().join("out"),
                temp_dir.path().join("extra"),
            ],
            &prompt,
            &config,
        );

        // Assert
        assert_that!(matches!(actual.unwrap_err(), AppError::InvalidArgs(_))).is_true();
    }

    #[test]
    fn parse_action_should_return_archive_stem_as_output_dir_when_in_auto_mode() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let prompt = MockPrompt::new();
        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Auto,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(vec![archive.clone()], &prompt, &config);

        // Assert
        assert_that!(&actual).is_ok();

        let actual_action = actual.unwrap();
        let expected = temp_dir.path().join("input");

        assert_that!(&actual_action).is_equal_to(&Action::Extract {
            archive,
            out: expected,
        });
    }

    #[test]
    fn parse_action_should_prompt_for_out_dir_when_only_input_given_in_prompt_mode() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let expected_out = temp_dir.path().join("out/");
        let mut prompt = MockPrompt::new();
        prompt
            .expect_path()
            .times(1)
            .return_const(expected_out.clone());

        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Prompt,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(vec![archive.clone()], &prompt, &config);

        // Assert
        assert_that!(&actual).is_ok();

        let actual_action = actual.unwrap();

        assert_that!(&actual_action).is_equal_to(&Action::Extract {
            archive,
            out: expected_out,
        });
    }

    #[test]
    fn parse_action_should_use_last_arg_for_out_dir_when_two_paths_given_in_prompt_mode() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let expected_out = temp_dir.path().join("out/");
        let prompt = MockPrompt::new();

        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Prompt,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(
            vec![archive.clone(), expected_out.clone()],
            &prompt,
            &config,
        );

        // Assert
        assert_that!(&actual).is_ok();

        let actual_action = actual.unwrap();

        assert_that!(&actual_action).is_equal_to(&Action::Extract {
            archive,
            out: expected_out,
        });
    }

    #[test]
    fn parse_action_should_return_error_when_multiple_out_paths_given_in_prompt_mode() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let prompt = MockPrompt::new();
        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Prompt,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(
            vec![
                archive,
                temp_dir.path().join("out"),
                temp_dir.path().join("extra"),
            ],
            &prompt,
            &config,
        );

        // Assert
        assert_that!(matches!(actual.unwrap_err(), AppError::InvalidArgs(_))).is_true();
    }

    #[test]
    fn parse_action_should_return_error_when_no_out_dir_given() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let prompt = MockPrompt::new();
        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Manual,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(vec![archive], &prompt, &config);

        // Assert
        assert_that!(matches!(actual.unwrap_err(), AppError::InvalidArgs(_))).is_true();
    }

    #[test]
    fn parse_action_should_return_error_when_multiple_out_paths_given_in_manual_mode() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let prompt = MockPrompt::new();
        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Manual,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(
            vec![
                archive,
                temp_dir.path().join("out"),
                temp_dir.path().join("extra"),
            ],
            &prompt,
            &config,
        );

        // Assert
        assert_that!(matches!(actual.unwrap_err(), AppError::InvalidArgs(_))).is_true();
    }

    #[test]
    fn parse_action_should_extract_action_when_out_dir_given_in_manual_mode() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let expected_out = temp_dir.path().join("out");

        let prompt = MockPrompt::new();
        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Manual,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(
            vec![archive.clone(), expected_out.clone()],
            &prompt,
            &config,
        );

        // Assert
        assert_that!(&actual).is_ok();

        let actual_action = actual.unwrap();

        assert_that!(&actual_action).is_equal_to(&Action::Extract {
            archive,
            out: expected_out,
        });
    }

    #[test]
    fn parse_action_should_return_error_when_out_dir_is_not_a_dir() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let archive = temp_dir.path().join("input.zip");
        File::create(&archive).unwrap();

        let out = temp_dir.path().join("file.txt");
        File::create(&out).unwrap();

        let prompt = MockPrompt::new();

        let config = Config {
            extract_prefs: ExtractPrefs {
                strip_top_level_dir: true,
                output_mode: OutputModes::Manual,
            },
            create_prefs: CreatePrefs {},
        };

        // Act
        let actual = parse_action(vec![archive, out.clone()], &prompt, &config);

        // Assert
        assert_that!(&actual).is_err();

        let actual_err = actual.unwrap_err();

        assert!(matches!(actual_err, AppError::NotFolder { path: _ }));
    }

    #[test]
    fn parse_action_should_return_err_when_one_of_the_inputs_doesnt_exist() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let input = temp_dir.path().join("input.txt");
        File::create(&input).unwrap();

        let second_input = temp_dir.path().join("input2.txt");

        let out = temp_dir.path().join("out.zip");

        let prompt = MockPrompt::new();
        let config = Config::default();

        // Act
        let actual = parse_action(vec![input, second_input, out], &prompt, &config);

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
        let temp_dir = TempDir::new().unwrap();

        let input = temp_dir.path().join("input.txt");
        File::create(&input).unwrap();

        let second_input = temp_dir.path().join("input.zip");
        File::create(&second_input).unwrap();

        let out = temp_dir.path().join("out.tar");

        let mut prompt = MockPrompt::new();
        prompt
            .expect_path()
            .with(eq("Where do you want to create the archive?"))
            .once()
            .return_const(out.clone());

        let config = Config::default();

        // Act
        let actual = parse_action(vec![input.clone(), second_input.clone()], &prompt, &config);

        // Assert
        assert_that!(actual).to_value().is_equal_to(Action::Create {
            inputs: vec![input, second_input],
            archive: out,
        });
    }

    #[test]
    fn parse_action_should_prompt_when_last_path_is_not_an_archive() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let input = temp_dir.path().join("input.txt");
        File::create(&input).unwrap();

        let second_input = temp_dir.path().join("input.foo");
        File::create(&second_input).unwrap();

        let out = temp_dir.path().join("out.zip");

        let mut prompt = MockPrompt::new();
        prompt
            .expect_path()
            .with(eq("Where do you want to create the archive?"))
            .once()
            .return_const(out.clone());

        let config = Config::default();

        // Act
        let actual = parse_action(vec![input.clone(), second_input.clone()], &prompt, &config);

        // Assert
        assert_that!(actual).to_value().is_equal_to(Action::Create {
            inputs: vec![input, second_input],
            archive: out,
        });
    }

    #[test]
    fn parse_action_should_return_create_action_when_last_path_is_archive() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let input = temp_dir.path().join("file.txt");
        File::create(&input).unwrap();

        let second_input = temp_dir.path().join("file2.txt");
        File::create(&second_input).unwrap();

        let out = temp_dir.path().join("input.zip");

        let prompt = MockPrompt::new();

        let config = Config::default();

        // Act
        let actual = parse_action(
            vec![input.clone(), second_input.clone(), out.clone()],
            &prompt,
            &config,
        );

        // Assert
        assert_that!(actual).to_value().is_equal_to(Action::Create {
            inputs: vec![input, second_input],
            archive: out,
        });
    }
}

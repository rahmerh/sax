use std::path::PathBuf;

use dialoguer::Input;

use crate::theme;

#[cfg_attr(test, mockall::automock)]
pub trait Prompt {
    fn path(&self, message: &str) -> PathBuf;
}

pub struct Prompter;

impl Prompt for Prompter {
    fn path(&self, message: &str) -> PathBuf {
        let theme = theme::default_theme();

        let input: String = Input::with_theme(&theme)
            .with_prompt(message)
            .interact_text()
            .unwrap();

        PathBuf::from(input)
    }
}

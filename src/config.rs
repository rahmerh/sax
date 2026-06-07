use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

const DEFAULT_CONFIG: &str = "create_prefs: {}\nextract_prefs:\n  strip_top_level_dir: true\n";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    pub create_prefs: CreatePrefs,
    pub extract_prefs: ExtractPrefs,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct CreatePrefs {}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ExtractPrefs {
    pub strip_top_level_dir: bool,
}

impl Default for ExtractPrefs {
    fn default() -> Self {
        Self {
            strip_top_level_dir: true,
        }
    }
}

impl Config {
    pub fn load_or_create() -> Result<Self> {
        let path = config_path()?;

        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create config directory {}", parent.display())
                })?;
            }

            fs::write(&path, DEFAULT_CONFIG)
                .with_context(|| format!("failed to write config {}", path.display()))?;
            return Ok(Self::default());
        }

        let input = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        serde_yaml::from_str(&input)
            .with_context(|| format!("invalid config YAML in {}", path.display()))
    }
}

fn config_path() -> Result<PathBuf> {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(config_home).join("sax").join("config.yaml"));
    }

    let home =
        env::var_os("HOME").ok_or_else(|| anyhow!("could not determine config directory"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("sax")
        .join("config.yaml"))
}

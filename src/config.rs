use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{editor::EditorChoice, terminal::TerminalChoice};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub editor: EditorChoice,
    pub project_roots: Vec<PathBuf>,
    pub worktree_root: PathBuf,
    pub terminal: TerminalChoice,
}

pub fn config_path() -> Result<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let xdg = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    match (xdg, home) {
        (Some(xdg), _) => Ok(config_path_for(Some(&xdg), None)),
        (None, Some(home)) => Ok(config_path_for(None, Some(&home))),
        (None, None) => anyhow::bail!("HOME and XDG_CONFIG_HOME are both unset"),
    }
}

pub fn config_path_for(xdg: Option<&Path>, home: Option<&Path>) -> PathBuf {
    if let Some(xdg) = xdg {
        xdg.join("wt").join("config.toml")
    } else {
        home.unwrap_or_else(|| Path::new("."))
            .join(".config/wt/config.toml")
    }
}

pub fn expand_tilde(value: &str, home: &Path) -> PathBuf {
    if value == "~" {
        home.to_path_buf()
    } else if let Some(rest) = value.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(value)
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("parse config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create config directory {}", parent.display()))?;
        }
        let contents = toml::to_string_pretty(self).context("serialize config")?;
        fs::write(path, contents).with_context(|| format!("write config {}", path.display()))
    }

    pub fn validate(&self) -> Result<()> {
        if !self.project_roots.iter().all(|root| root.is_absolute()) {
            anyhow::bail!("all project roots must be absolute");
        }
        if !self.worktree_root.is_absolute() {
            anyhow::bail!("worktree root must be absolute");
        }
        Ok(())
    }
}

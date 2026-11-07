use std::{fmt, process::Command};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorChoice {
    VisualStudioCode,
    VisualStudioCodeInsiders,
    Cursor,
    Windsurf,
    SublimeText,
    Zed,
    VSCodium,
    Vim,
    Neovim,
    Emacs,
    Helix,
    IntelliJIdea,
    PyCharm,
    WebStorm,
    PhpStorm,
    GoLand,
    RubyMine,
    CLion,
    AndroidStudio,
}

impl EditorChoice {
    pub fn name(self) -> &'static str {
        match self {
            Self::VisualStudioCode => "Visual Studio Code",
            Self::VisualStudioCodeInsiders => "VS Code Insiders",
            Self::Cursor => "Cursor",
            Self::Windsurf => "Windsurf",
            Self::SublimeText => "Sublime Text",
            Self::Zed => "Zed",
            Self::VSCodium => "VSCodium",
            Self::Vim => "Vim",
            Self::Neovim => "Neovim",
            Self::Emacs => "Emacs",
            Self::Helix => "Helix",
            Self::IntelliJIdea => "IntelliJ IDEA",
            Self::PyCharm => "PyCharm",
            Self::WebStorm => "WebStorm",
            Self::PhpStorm => "PhpStorm",
            Self::GoLand => "GoLand",
            Self::RubyMine => "RubyMine",
            Self::CLion => "CLion",
            Self::AndroidStudio => "Android Studio",
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::VisualStudioCode => "code",
            Self::VisualStudioCodeInsiders => "code-insiders",
            Self::Cursor => "cursor",
            Self::Windsurf => "windsurf",
            Self::SublimeText => "subl",
            Self::Zed => "zed",
            Self::VSCodium => "codium",
            Self::Vim => "vim",
            Self::Neovim => "nvim",
            Self::Emacs => "emacs",
            Self::Helix => "hx",
            Self::IntelliJIdea => "idea",
            Self::PyCharm => "pycharm",
            Self::WebStorm => "webstorm",
            Self::PhpStorm => "phpstorm",
            Self::GoLand => "goland",
            Self::RubyMine => "rubymine",
            Self::CLion => "clion",
            Self::AndroidStudio => "studio",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Emacs | Self::Vim | Self::Neovim | Self::Helix)
    }

    pub fn terminal_command(self, path: &std::path::Path) -> String {
        let mode = if self == Self::Emacs { " -nw" } else { "" };
        format!(
            "{}{mode} {}",
            self.command(),
            shell_quote(&path.to_string_lossy())
        )
    }

    pub fn installed(self) -> bool {
        which::which(self.command()).is_ok()
    }

    pub fn launch(self, path: &std::path::Path) -> Result<()> {
        Command::new(self.command())
            .arg(path)
            .spawn()
            .with_context(|| format!("launch {}", self.name()))?;
        Ok(())
    }
}

impl fmt::Display for EditorChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

pub fn editors() -> Vec<EditorChoice> {
    use EditorChoice::*;
    vec![
        VisualStudioCode,
        VisualStudioCodeInsiders,
        Cursor,
        Windsurf,
        SublimeText,
        Zed,
        VSCodium,
        Vim,
        Neovim,
        Emacs,
        Helix,
        IntelliJIdea,
        PyCharm,
        WebStorm,
        PhpStorm,
        GoLand,
        RubyMine,
        CLion,
        AndroidStudio,
    ]
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

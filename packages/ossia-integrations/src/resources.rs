use crate::files::absolute_path;
use ossia_types::{AppError, ErrorCode};
use std::{
    fs,
    path::{Path, PathBuf},
};

const SKILL: &[u8] = include_bytes!("../resources/skill/ossia/SKILL.md");
const ICON: &[u8] = include_bytes!("../resources/assets/ossia.svg");
const DSH_HOST: &[u8] = include_bytes!("../resources/dsh/index.js");
const DSH_CLIENT: &[u8] = include_bytes!("../resources/dsh/client.js");
const DSH_LOCALE_EN: &[u8] = include_bytes!("../resources/dsh/locale/en.json");
const DSH_LOCALE_ZH: &[u8] = include_bytes!("../resources/dsh/locale/zh.json");

/// A host integration template shipped with the installer.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Resource {
    Skill,
    Icon,
    DshHost,
    DshClient,
    DshLocaleEn,
    DshLocaleZh,
}

impl Resource {
    fn path(self) -> &'static str {
        match self {
            Self::Skill => "skill/ossia/SKILL.md",
            Self::Icon => "assets/ossia.svg",
            Self::DshHost => "dsh/index.js",
            Self::DshClient => "dsh/client.js",
            Self::DshLocaleEn => "dsh/locale/en.json",
            Self::DshLocaleZh => "dsh/locale/zh.json",
        }
    }

    fn bundled(self) -> &'static [u8] {
        match self {
            Self::Skill => SKILL,
            Self::Icon => ICON,
            Self::DshHost => DSH_HOST,
            Self::DshClient => DSH_CLIENT,
            Self::DshLocaleEn => DSH_LOCALE_EN,
            Self::DshLocaleZh => DSH_LOCALE_ZH,
        }
    }
}

/// Source of the host integration templates shipped with the installer.
#[derive(Debug, Clone)]
pub(crate) enum ResourceSource {
    Bundled,
    Directory(PathBuf),
}

impl ResourceSource {
    pub(crate) fn directory(path: &Path) -> Result<Self, AppError> {
        Ok(Self::Directory(absolute_path(
            path,
            "integration resources",
        )?))
    }

    pub(crate) fn read_text(&self, resource: Resource) -> Result<String, AppError> {
        self.read(resource)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    pub(crate) fn read(&self, resource: Resource) -> Result<Vec<u8>, AppError> {
        match self {
            Self::Bundled => Ok(resource.bundled().to_vec()),
            Self::Directory(root) => fs::read(root.join(resource.path()))
                .map_err(|error| unavailable(resource.path(), error)),
        }
    }
}

fn unavailable(name: &str, error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::UnsupportedSource,
        format!("integration resource {name} is unavailable: {error}"),
    )
}

#[cfg(test)]
#[path = "../tests/unit/resources.rs"]
mod tests;

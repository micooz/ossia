use crate::command;
use std::path::{Path, PathBuf};

pub(crate) fn find(
    name: &str,
    explicit: Option<&Path>,
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    command::find_program_with_candidates(name, explicit, candidates)
}

pub(crate) fn mac_app(relative: &str) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/Applications").join(relative)];
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join("Applications").join(relative));
    }
    paths
}

pub(crate) fn common_bin(name: &str) -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
        PathBuf::from("/usr/bin").join(name),
    ];
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/bin").join(name));
        paths.push(home.join("bin").join(name));
    }
    paths
}

#[cfg(windows)]
pub(crate) fn program_files(name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for key in ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"] {
        if let Some(root) = std::env::var_os(key) {
            let root = PathBuf::from(root);
            paths.push(root.join("Programs").join(name));
            paths.push(root.join(name));
        }
    }
    paths
}

#[cfg(test)]
mod tests;

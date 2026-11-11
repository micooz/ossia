use ossia_types::{AppError, ErrorCode};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

pub fn run(
    program: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    let output = Command::new(program)
        .args(args)
        .envs(env)
        .output()
        .map_err(|error| {
            AppError::new(
                ErrorCode::RuntimeUnavailable,
                format!("cannot execute {}: {error}", program.display()),
            )
        })?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail: String = stderr
        .lines()
        .chain(stdout.lines())
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Err(AppError::new(
        ErrorCode::RuntimeUnavailable,
        format!(
            "{} exited with {}: {}",
            program.display(),
            output.status,
            if detail.is_empty() {
                "no diagnostic output"
            } else {
                detail.trim()
            }
        ),
    ))
}

pub fn version(program: &Path, env: &BTreeMap<String, String>) -> Result<String, AppError> {
    let text = run(program, &["--version".into()], env)?;
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| AppError::new(ErrorCode::InvalidInput, "host version output is empty"))?;
    Ok(line
        .split_whitespace()
        .last()
        .unwrap_or(line)
        .trim()
        .to_string())
}

pub fn find_program(name: &str, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return executable(path).then(|| path.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for candidate in candidates(&directory, name) {
            if executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// Find an explicit override, then a `PATH` entry, then known installation paths.
pub fn find_program_with_candidates(
    name: &str,
    explicit: Option<&Path>,
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return executable(path).then(|| path.to_path_buf());
    }
    if let Some(path) = find_program(name, None) {
        return Some(path);
    }
    candidates.into_iter().find(|path| executable(path))
}

fn candidates(directory: &Path, name: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let extensions = std::env::var_os("PATHEXT")
            .map(|value| {
                value
                    .to_string_lossy()
                    .split(';')
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![".exe".into(), ".cmd".into(), ".bat".into()]);
        let mut paths = vec![directory.join(name)];
        for extension in extensions {
            paths.push(directory.join(format!("{name}{extension}")));
        }
        paths
    }
    #[cfg(not(windows))]
    {
        vec![directory.join(name)]
    }
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn host_env(root_var: &str, root: &Path) -> BTreeMap<String, String> {
    BTreeMap::from([(root_var.to_string(), root.to_string_lossy().into_owned())])
}

pub fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

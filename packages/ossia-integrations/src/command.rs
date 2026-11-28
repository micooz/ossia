use ossia_types::{path_text, AppError, ErrorCode};
use std::{
    cell::Cell,
    collections::BTreeMap,
    ffi::OsStr,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

thread_local! { static SCANNING: Cell<bool> = const { Cell::new(false) }; }

pub(crate) fn scanning<T>(f: impl FnOnce() -> T) -> T {
    SCANNING.with(|scanning| {
        struct Reset<'a>(&'a Cell<bool>, bool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _reset = Reset(scanning, scanning.replace(true));
        f()
    })
}

pub fn run(
    program: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    run_with_prefix(program, &[], args, env)
}

pub fn run_with_prefix(
    program: &Path,
    prefix: &[String],
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    let args = prefix.iter().chain(args).cloned().collect::<Vec<_>>();
    let output = if SCANNING.get() {
        output_with_timeout(program, &args, env, Duration::from_secs(10))?
    } else {
        host_command(program, &args)
            .envs(env)
            .output()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::RuntimeUnavailable,
                    format!("cannot execute {}: {error}", program.display()),
                )
            })?
    };
    interpret_output(program, output)
}

fn interpret_output(program: &Path, output: Output) -> Result<String, AppError> {
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

fn output_with_timeout(
    program: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<Output, AppError> {
    let failure = |error: std::io::Error| {
        AppError::new(
            ErrorCode::RuntimeUnavailable,
            format!("cannot execute {}: {error}", program.display()),
        )
    };
    let mut stdout = tempfile::tempfile().map_err(failure)?;
    let mut stderr = tempfile::tempfile().map_err(failure)?;
    let mut child = host_command(program, args)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone().map_err(failure)?))
        .stderr(Stdio::from(stderr.try_clone().map_err(failure)?))
        .spawn()
        .map_err(failure)?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::new(
                    ErrorCode::RuntimeUnavailable,
                    format!(
                        "{} timed out after {} seconds",
                        program.display(),
                        timeout.as_secs_f64()
                    ),
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure(error));
            }
        }
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    stdout
        .seek(SeekFrom::Start(0))
        .and_then(|_| stdout.read_to_end(&mut out))
        .map_err(failure)?;
    stderr
        .seek(SeekFrom::Start(0))
        .and_then(|_| stderr.read_to_end(&mut err))
        .map_err(failure)?;
    Ok(Output {
        status,
        stdout: out,
        stderr: err,
    })
}

#[cfg(all(test, unix))]
fn run_with_timeout(
    program: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<String, AppError> {
    interpret_output(program, output_with_timeout(program, args, env, timeout)?)
}

fn host_command(program: &Path, args: &[String]) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = if program
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            }) {
            let mut command =
                Command::new(std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into()));
            command
                .args(["/D", "/S", "/C"])
                .raw_arg(script_command_line(program, args));
            command
        } else {
            let mut command = Command::new(program);
            command.args(args);
            command
        };
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = Command::new(program);
        command.args(args);
        command
    }
}

#[cfg(windows)]
fn script_command_line(program: &Path, args: &[String]) -> String {
    let mut line = quote_script_argument(program);
    for argument in args {
        line.push(' ');
        line.push_str(&quote_script_argument(Path::new(argument)));
    }
    format!("\"{line}\"")
}

#[cfg(windows)]
fn quote_script_argument(value: &Path) -> String {
    format!("\"{}\"", value.to_string_lossy().replace('"', "\"\""))
}

pub fn version(program: &Path, env: &BTreeMap<String, String>) -> Result<String, AppError> {
    version_with_prefix(program, &[], env)
}

#[cfg(unix)]
pub(crate) fn login_shell() -> Option<PathBuf> {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};

    if let Some(shell) = std::env::var_os("SHELL").filter(|shell| !shell.is_empty()) {
        return Some(shell.into());
    }
    let mut size = 16 * 1024;
    loop {
        let mut buffer = vec![0; size];
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // getpwuid_r writes into the provided entry and buffer; both remain alive while read.
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && size < 1024 * 1024 {
            size *= 2;
            continue;
        }
        if status != 0 || result.is_null() {
            return None;
        }
        // A successful lookup supplies a null-terminated shell path in the buffer.
        let shell = unsafe { (*result).pw_shell };
        if shell.is_null() {
            return None;
        }
        let shell = unsafe { CStr::from_ptr(shell) };
        return (!shell.to_bytes().is_empty())
            .then(|| PathBuf::from(OsStr::from_bytes(shell.to_bytes())));
    }
}

#[cfg(unix)]
pub(crate) fn shell_path(shell: &Path, env: &BTreeMap<String, String>) -> Result<String, AppError> {
    let args = arguments(&[
        "-ilc",
        "printf '\\nOSSIA_SHELL_PATH_BEGIN\\n'; /usr/bin/printenv PATH; printf '\\nOSSIA_SHELL_PATH_END\\n'",
    ]);
    let output = output_with_timeout(shell, &args, env, Duration::from_secs(5))?;
    let output = interpret_output(shell, output)?;
    let path = output
        .rsplit_once("\nOSSIA_SHELL_PATH_BEGIN\n")
        .and_then(|(_, tail)| tail.split_once("\nOSSIA_SHELL_PATH_END\n"))
        .map(|(path, _)| path.trim_end_matches('\n'))
        .filter(|path| !path.is_empty() && !path.contains('\n'))
        .ok_or_else(|| AppError::new(ErrorCode::RuntimeUnavailable, "cannot read shell PATH"))?;
    Ok(path.into())
}

pub fn version_with_prefix(
    program: &Path,
    prefix: &[String],
    env: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    let text = run_with_prefix(program, prefix, &["--version".into()], env)?;
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
    find_program_on_path(name, &std::env::var_os("PATH")?)
}

fn find_program_on_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    for directory in std::env::split_paths(path) {
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
        [".exe", ".com", ".cmd", ".bat"]
            .into_iter()
            .map(|extension| directory.join(format!("{name}{extension}")))
            .collect()
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
    BTreeMap::from([(root_var.to_string(), path_text(root))])
}

pub fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[cfg(test)]
#[path = "../tests/command.rs"]
mod tests;

use crate::{failure, io_error};
use ossia_types::{AppError, ErrorCode};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct RunContext {
    pub cancellation: Cancellation,
    /// Deadline for the entire operation, including cache lock waits and all passes.
    deadline: Instant,
}
impl RunContext {
    pub fn new(timeout: Duration) -> Self {
        Self {
            cancellation: Cancellation::default(),
            deadline: Instant::now() + timeout,
        }
    }
    pub fn check(&self) -> Result<(), AppError> {
        if self.cancellation.is_cancelled() {
            Err(AppError::new(
                ErrorCode::Cancelled,
                "engine operation cancelled",
            ))
        } else if Instant::now() >= self.deadline {
            Err(failure("timeout", "engine operation timed out"))
        } else {
            Ok(())
        }
    }
}
impl Default for RunContext {
    fn default() -> Self {
        Self::new(Duration::from_secs(180))
    }
}

pub(crate) struct Scratch(pub TempDir);
impl Scratch {
    pub fn new(root: &Path) -> Result<Self, AppError> {
        fs::create_dir_all(root).map_err(io_error)?;
        let dir = tempfile::Builder::new()
            .prefix("ossia-engine-")
            .tempdir_in(root)
            .map_err(io_error)?;
        for name in ["home", "cache", "tmp"] {
            fs::create_dir(dir.path().join(name)).map_err(io_error)?;
        }
        Ok(Self(dir))
    }
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

pub(crate) struct Output {
    pub success: bool,
    pub log: String,
}

pub(crate) fn run(
    program: &Path,
    prefix: &[String],
    args: &[&str],
    scratch: &Scratch,
    context: &RunContext,
) -> Result<Output, AppError> {
    context.check()?;
    let log_path = scratch.path().join("process.log");
    let log = File::create(&log_path).map_err(io_error)?;
    let mut command = Command::new(program);
    command
        .args(prefix)
        .args(args)
        .current_dir(scratch.path())
        .env("HOME", scratch.path().join("home"))
        .env("USERPROFILE", scratch.path().join("home"))
        .env("XDG_CACHE_HOME", scratch.path().join("cache"))
        .env("TMPDIR", scratch.path().join("tmp"))
        .env("TMP", scratch.path().join("tmp"))
        .env("TEMP", scratch.path().join("tmp"))
        .env("GUILE_AUTO_COMPILE", "0")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("LC_ALL", "C.UTF-8")
        .env("LANG", "C.UTF-8")
        .env_remove("GUILE_LOAD_PATH")
        .env_remove("GUILE_LOAD_COMPILED_PATH")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(io_error)?)
        .stderr(log);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
        command.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
    }
    let child = command
        .spawn()
        .map_err(|_| failure("spawn", "cannot start engine tool"))?;
    let mut tree = ProcessTree::new(child)?;
    loop {
        context.check()?;
        if fs::metadata(&log_path).map_err(io_error)?.len() > 16 * 1024 * 1024 {
            return Err(failure("output-limit", "engine log exceeded 16 MiB"));
        }
        if let Some(status) = tree.child.try_wait().map_err(io_error)? {
            // A successful parent may still leave descendants holding files open.
            tree.terminate();
            let bytes = fs::read(&log_path).map_err(io_error)?;
            return Ok(Output {
                success: status.success(),
                log: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct ProcessTree {
    child: Child,
    terminated: bool,
    #[cfg(windows)]
    job: windows_job::Job,
}
impl ProcessTree {
    fn new(mut child: Child) -> Result<Self, AppError> {
        #[cfg(windows)]
        {
            match windows_job::Job::attach(&child) {
                Ok(job) => Ok(Self {
                    child,
                    job,
                    terminated: false,
                }),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    Err(error)
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = &mut child;
            Ok(Self {
                child,
                terminated: false,
            })
        }
    }
    fn terminate(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        #[cfg(unix)]
        unsafe {
            // The child was made process-group leader before exec; never target our own group.
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        self.job.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(windows)]
mod windows_job {
    use super::*;
    use std::{
        mem::{size_of, zeroed},
        os::windows::io::AsRawHandle,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    pub struct Job(HANDLE);
    impl Job {
        pub fn attach(child: &Child) -> Result<Self, AppError> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(failure("job", "cannot create engine job"));
                }
                let job = Self(handle);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as _,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == 0
                    || AssignProcessToJobObject(handle, child.as_raw_handle() as _) == 0
                {
                    return Err(failure("job", "cannot attach engine process to job"));
                }
                let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
                if snapshot == INVALID_HANDLE_VALUE {
                    return Err(failure("job", "cannot find suspended engine thread"));
                }
                let mut entry: THREADENTRY32 = zeroed();
                entry.dwSize = size_of::<THREADENTRY32>() as u32;
                let mut found = false;
                let mut ok = Thread32First(snapshot, &mut entry);
                while ok != 0 {
                    if entry.th32OwnerProcessID == child.id() {
                        let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                        if !thread.is_null() {
                            found = ResumeThread(thread) != u32::MAX;
                            CloseHandle(thread);
                        }
                        break;
                    }
                    ok = Thread32Next(snapshot, &mut entry);
                }
                CloseHandle(snapshot);
                if !found {
                    return Err(failure("job", "cannot resume engine thread"));
                }
                Ok(job)
            }
        }
        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                TerminateJobObject(self.0, 1);
                CloseHandle(self.0);
            }
        }
    }
}

pub(crate) fn tool_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let p = crate::cache::confined(root, relative)?;
    if !p.is_file() {
        return Err(failure("missing-tool", "engine tool is missing"));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "subprocess fixture invoked by process-tree regression"]
    fn fixture_parent() {
        let mode = fs::read_to_string("mode").unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "process::tests::fixture_grandchild"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        fs::write("descendant.pid", child.id().to_string()).unwrap();
        let start = Instant::now();
        while !Path::new("ready").exists() {
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
        match mode.as_str() {
            "normal" => {}
            "crash" => panic!("intentional fixture crash"),
            _ => {
                let _ = child.wait();
            }
        }
    }
    #[test]
    #[ignore = "subprocess fixture invoked by process-tree regression"]
    fn fixture_grandchild() {
        fs::write("ready", b"ready").unwrap();
        std::thread::sleep(Duration::from_secs(10));
    }

    fn alive(pid: u32) -> bool {
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as i32, 0) == 0
        }
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::Threading::{
                    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                },
            };
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut code = 0;
            let ok = GetExitCodeProcess(handle, &mut code) != 0;
            CloseHandle(handle);
            ok && code == 259
        }
    }

    #[test]
    fn process_trees_are_reaped_on_success_crash_timeout_and_cancel() {
        for mode in ["normal", "crash", "timeout", "cancel"] {
            let root = tempfile::tempdir().unwrap();
            let scratch = Scratch::new(root.path()).unwrap();
            fs::write(scratch.path().join("mode"), mode).unwrap();
            let context = RunContext::new(if mode == "timeout" {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(5)
            });
            let canceller = if mode == "cancel" {
                let token = context.cancellation.clone();
                let ready = scratch.path().join("ready");
                Some(std::thread::spawn(move || {
                    let start = Instant::now();
                    while !ready.exists() && start.elapsed() < Duration::from_secs(3) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    token.cancel();
                }))
            } else {
                None
            };
            let output = run(
                &std::env::current_exe().unwrap(),
                &[],
                &[
                    "--ignored",
                    "--exact",
                    "process::tests::fixture_parent",
                    "--nocapture",
                ],
                &scratch,
                &context,
            );
            if let Some(thread) = canceller {
                thread.join().unwrap();
            }
            match mode {
                "normal" => assert!(output.unwrap().success),
                "crash" => assert!(!output.unwrap().success),
                "cancel" => assert_eq!(output.err().unwrap().code, ErrorCode::Cancelled),
                _ => assert_eq!(output.err().unwrap().detail.unwrap()["reason"], "timeout"),
            }
            let pid = fs::read_to_string(scratch.path().join("descendant.pid"))
                .unwrap()
                .parse()
                .unwrap();
            let start = Instant::now();
            while alive(pid) && start.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!alive(pid), "descendant {pid} survived {mode}");
            drop(scratch);
            #[cfg(not(windows))]
            assert!(fs::read_dir(root.path()).unwrap().next().is_none());
        }
    }
}

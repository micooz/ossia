use super::*;

#[derive(Debug, Clone)]
pub(crate) struct Host {
    pub(crate) executable: PathBuf,
    pub(crate) env: BTreeMap<String, String>,
}

pub fn detect(integrator: &Integrator) -> HostDetection {
    let paths = integrator.paths();
    let config_root = paths.codex_home.to_string_lossy().into_owned();
    let skill_root = paths
        .agents_home
        .join("skills")
        .to_string_lossy()
        .into_owned();
    let Some(executable) = command::find_program("codex", paths.codex_bin.as_deref()) else {
        return HostDetection {
            host: HostKind::Codex,
            installed: false,
            version: None,
            executable: None,
            config_root,
            skill_root: Some(skill_root),
            profiles: vec!["default".into()],
            capabilities: vec![],
            problem: Some(AppError::new(
                ErrorCode::NotFound,
                "Codex CLI was not found; set OSSIA_CODEX_BIN or install the Codex app",
            )),
        };
    };
    let env = command::host_env("CODEX_HOME", &paths.codex_home);
    match command::version(&executable, &env) {
        Ok(version) => {
            let mut capabilities = BTreeSet::new();
            if command::run(&executable, &command::arguments(&["mcp", "--help"]), &env).is_ok() {
                capabilities.insert("mcp");
            }
            if command::run(
                &executable,
                &command::arguments(&["plugin", "--help"]),
                &env,
            )
            .is_ok()
            {
                capabilities.insert("plugin");
            }
            HostDetection {
                host: HostKind::Codex,
                installed: true,
                version: Some(version),
                executable: Some(executable.to_string_lossy().into_owned()),
                config_root,
                skill_root: Some(skill_root),
                profiles: vec!["default".into()],
                capabilities: capabilities.into_iter().map(str::to_string).collect(),
                problem: None,
            }
        }
        Err(problem) => HostDetection {
            host: HostKind::Codex,
            installed: false,
            version: None,
            executable: Some(executable.to_string_lossy().into_owned()),
            config_root,
            skill_root: Some(skill_root),
            profiles: vec!["default".into()],
            capabilities: vec![],
            problem: Some(problem),
        },
    }
}

pub(crate) fn host(integrator: &Integrator, detection: &HostDetection) -> Option<Host> {
    detection.executable.as_ref().map(|executable| Host {
        executable: PathBuf::from(executable),
        env: command::host_env("CODEX_HOME", &integrator.paths().codex_home),
    })
}

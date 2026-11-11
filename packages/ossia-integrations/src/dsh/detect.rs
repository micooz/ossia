use super::*;

#[derive(Debug, Clone)]
pub(crate) struct Host {
    pub(crate) executable: PathBuf,
    pub(crate) env: BTreeMap<String, String>,
}

pub fn detect(integrator: &Integrator) -> HostDetection {
    let paths = integrator.paths();
    let config_root = paths.dsh_home.to_string_lossy().into_owned();
    let skill_root = Some(paths.dsh_home.join("skills").to_string_lossy().into_owned());
    let profiles = profiles(&paths.dsh_home);
    let Some(executable) = command::find_program("dsh", paths.dsh_bin.as_deref()) else {
        return HostDetection {
            host: HostKind::Dsh,
            installed: false,
            version: None,
            executable: None,
            config_root,
            skill_root,
            profiles,
            capabilities: vec![],
            problem: Some(AppError::new(
                ErrorCode::NotFound,
                "DSH CLI was not found; set OSSIA_DSH_BIN or install DeepSeek Harness",
            )),
        };
    };
    let env = command::host_env("DSH_HOME", &paths.dsh_home);
    match command::version(&executable, &env) {
        Ok(version) => {
            let mut capabilities = BTreeSet::new();
            if command::run(
                &executable,
                &command::arguments(&["plugin", "--help"]),
                &env,
            )
            .is_ok()
            {
                capabilities.insert("plugin");
            }
            for profile in &profiles {
                if browser_enabled(&executable, &env, profile).is_some() {
                    capabilities.insert("sidebar-browser");
                }
            }
            HostDetection {
                host: HostKind::Dsh,
                installed: true,
                version: Some(version),
                executable: Some(executable.to_string_lossy().into_owned()),
                config_root,
                skill_root,
                profiles,
                capabilities: capabilities.into_iter().map(str::to_string).collect(),
                problem: None,
            }
        }
        Err(problem) => HostDetection {
            host: HostKind::Dsh,
            installed: false,
            version: None,
            executable: Some(executable.to_string_lossy().into_owned()),
            config_root,
            skill_root,
            profiles,
            capabilities: vec![],
            problem: Some(problem),
        },
    }
}

pub(crate) fn host(integrator: &Integrator, detection: &HostDetection) -> Option<Host> {
    detection.executable.as_ref().map(|executable| Host {
        executable: PathBuf::from(executable),
        env: command::host_env("DSH_HOME", &integrator.paths().dsh_home),
    })
}

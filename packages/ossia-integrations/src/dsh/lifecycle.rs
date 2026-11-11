use super::*;

pub fn apply(
    integrator: &Integrator,
    plan: &IntegrationPlan,
) -> Result<Vec<ManagedEntry>, AppError> {
    let detection = detect(integrator);
    let Some(host) = host(integrator, &detection) else {
        return Err(AppError::new(ErrorCode::NotFound, "DSH CLI disappeared"));
    };
    let profile = &plan.profile;
    let package_name = package_name(plan.mode);
    let skill = integrator.skill()?;
    let browser = browser_enabled(&host.executable, &host.env, profile);
    let files = match plan.mode {
        IntegrationMode::SkillMcp => vec![
            FilePlan {
                path: integrator.paths().dsh_home.join("skills/ossia/SKILL.md"),
                bytes: skill.clone().into_bytes(),
                kind: "skill",
            },
            json_file(
                bundle_dir(integrator, plan.mode).join("package.json"),
                skill_mcp_manifest(integrator),
                "bundle-manifest",
            ),
            FilePlan {
                path: bundle_dir(integrator, plan.mode).join("cordis.patch.yml"),
                bytes: skill_mcp_patch(integrator).into_bytes(),
                kind: "bundle-patch",
            },
        ],
        IntegrationMode::Plugin => plugin_files(integrator, &skill, browser == Some(false))?,
    };
    for file in &files {
        record::atomic_write(&file.path, &file.bytes)?;
    }
    let profile_value = read_json(
        &profile_package(&integrator.paths().dsh_home, profile),
        "DSH profile manifest",
    )?;
    if !installed_bundles(&profile_value)?
        .iter()
        .any(|bundle| bundle == package_name)
    {
        dsh(
            &host,
            &[
                "plugin".into(),
                "--profile".into(),
                profile.clone(),
                "add".into(),
                bundle_dir(integrator, plan.mode)
                    .to_string_lossy()
                    .into_owned(),
            ],
        )?;
    }
    Ok(records_from_plan(plan, &files))
}

pub fn remove(integrator: &Integrator, record: &ManagedRecord) -> Result<(), AppError> {
    remove_entries(integrator, &record.profile, record.mode, &record.entries)
}

pub fn remove_entries(
    integrator: &Integrator,
    profile: &str,
    mode: IntegrationMode,
    entries: &[ManagedEntry],
) -> Result<(), AppError> {
    verify_owned_files(entries)?;
    let detection = detect(integrator);
    if entries.iter().any(|entry| entry.kind == "host-entry") && detection.executable.is_none() {
        return Err(AppError::new(
            ErrorCode::RuntimeUnavailable,
            "DSH CLI is unavailable; the managed files were left unchanged",
        ));
    }
    if let Some(host) = host(integrator, &detection) {
        let package_name = package_name(mode);
        if entries
            .iter()
            .any(|entry| entry.path == format!("dsh:bundle:{package_name}"))
        {
            let profile_path = profile_package(&integrator.paths().dsh_home, profile);
            let installed = profile_path.exists()
                && read_json(&profile_path, "DSH profile manifest")
                    .ok()
                    .is_some_and(|value| {
                        installed_bundles(&value).is_ok_and(|bundles| {
                            bundles.iter().any(|bundle| bundle == package_name)
                        })
                    });
            if installed {
                dsh(
                    &host,
                    &[
                        "plugin".into(),
                        "--profile".into(),
                        profile.into(),
                        "remove".into(),
                        package_name.into(),
                    ],
                )?;
            }
        }
    }
    for entry in entries.iter().filter(|entry| entry.sha256.is_some()) {
        if let Some(hash) = entry.sha256.as_ref() {
            delete_owned(Path::new(&entry.path), hash)?;
        }
    }
    let _ = fs::remove_dir_all(bundle_dir(integrator, mode));
    let _ = fs::remove_dir(integrator.paths().dsh_home.join("skills/ossia"));
    Ok(())
}

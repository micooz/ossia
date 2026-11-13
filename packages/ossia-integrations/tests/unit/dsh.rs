use super::*;
use lifecycle::install;
use status::status;
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, Integrator) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::new(),
    };
    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let integrator = Integrator::with_resource_dir(
        root.path().join("library"),
        std::env::current_exe().unwrap(),
        resources,
        paths,
    )
    .unwrap()
    .with_shared_root(root.path().join("shared"))
    .unwrap();
    (root, integrator)
}

#[test]
fn plugin_package_declares_both_halves_and_absolute_mcp_command() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    assert_eq!(
        Dsh.config_path(&ctx),
        integrator
            .paths()
            .dsh_home
            .join("profiles/desktop/package.json")
    );
    let files = plugin_files(&ctx).unwrap();
    let package = files
        .iter()
        .find(|file| file.kind == "bundle-manifest")
        .unwrap();
    let package: Value = serde_json::from_slice(&package.bytes).unwrap();
    assert_eq!(package["name"], PLUGIN);
    assert_eq!(package["dsh"]["bundle"]["patch"], "./cordis.patch.yml");
    assert_eq!(package["dsh"]["client"]["platform"], "web");
    assert_eq!(package["engines"]["dsh"], ">=0.2.2-rc.1 <0.3.0");
    assert!(package["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file == "config.json"));

    let config = files
        .iter()
        .find(|file| file.kind == "host-config")
        .unwrap();
    let config: Value = serde_json::from_slice(&config.bytes).unwrap();
    assert_eq!(
        config["home"],
        serde_json::json!(integrator.home().to_string_lossy())
    );
    let host = files.iter().find(|file| file.kind == "host-entry").unwrap();
    assert!(String::from_utf8_lossy(&host.bytes).contains("/ossia-selection"));
    let client = files
        .iter()
        .find(|file| file.kind == "client-entry")
        .unwrap();
    assert!(String::from_utf8_lossy(&client.bytes).contains("insertText"));

    let patch = files
        .iter()
        .find(|file| file.kind == "bundle-patch")
        .unwrap();
    let patch = String::from_utf8_lossy(&patch.bytes);
    assert!(patch.contains("@deepseek-ai/dsh-mcp-client"));
    assert!(patch.contains("transport: stdio"));
    assert!(patch.contains(&yaml_string(&integrator.executable().to_string_lossy())));
    assert!(patch.contains("ossia-dsh-plugin/skill"));
    assert!(patch.contains("name: 'ossia-dsh-plugin'"));
}

#[test]
fn generated_skill_loads_with_host_metadata_and_disposes_registration() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let files = plugin_files(&ctx).unwrap();
    let loader = files
        .iter()
        .find(|file| file.kind == "skill-loader")
        .unwrap();
    let script = format!(
        r#"
import assert from 'node:assert/strict';
{}
assert.deepEqual(inject, ['skills']);
let registered;
let dispose;
let disposed = false;
apply({{
  skills: {{ register(skill) {{
    registered = skill;
    return () => {{ disposed = true; }};
  }} }},
  effect(register) {{ dispose = register(); }}
}});
// DSH validates these fields when the model loads a registered skill.
assert.equal(typeof registered.source, 'string');
assert.equal(registered.name, 'ossia');
assert.ok(registered.description.length > 0);
assert.ok(registered.content.includes('ossia_status'));
dispose();
assert.ok(disposed);
"#,
        String::from_utf8_lossy(&loader.bytes)
    );
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .output()
        .expect("Node is required to test the generated DSH plugin");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generated_host_loads_and_parses_runtime_frames() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let files = plugin_files(&ctx).unwrap();
    let host = files.iter().find(|file| file.kind == "host-entry").unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("index.js");
    fs::write(&path, &host.bytes).unwrap();
    fs::write(
        root.path().join("config.json"),
        serde_json::to_vec(&serde_json::json!({ "home": root.path() })).unwrap(),
    )
    .unwrap();
    let script = format!(
        r#"
import assert from 'node:assert/strict';
import {{ EventEmitter }} from 'node:events';
import {{ pathToFileURL }} from 'node:url';
const module = await import(pathToFileURL({}).href);
assert.deepEqual(module.inject, ['webServer']);
assert.equal(typeof module.apply, 'function');
assert.deepEqual(
  module.__test.parseFrame('event: runtime\ndata: {{"type":"selection-changed"}}'),
  {{ event: 'runtime', data: '{{"type":"selection-changed"}}' }},
);
const routes = [];
const effects = [];
await module.apply({{
  webServer: {{ register(route) {{
    routes.push(route);
    return () => {{}};
  }} }},
  effect(register) {{
    effects.push(register());
    return () => {{}};
  }},
}});
assert.equal(routes[0].kind, 'prefix');
assert.equal(routes[0].path, '/ossia-selection');
const request = new EventEmitter();
request.method = 'POST';
request.headers = {{ 'content-type': 'application/json' }};
let body;
const response = {{
  writeHead() {{}},
  end(value) {{ body = value; }},
}};
routes[0].handler(request, response);
request.emit('data', Buffer.from(JSON.stringify({{
  type: 'client-request',
  rpcId: 'r1',
  method: 'latest',
  payload: {{}},
}})));
request.emit('end');
const parsed = JSON.parse(body);
assert.equal(parsed.type, 'server-response');
assert.equal(parsed.rpcId, 'r1');
assert.equal(parsed.result.ok, true);
assert.equal(parsed.result.value.count, null);
for (const dispose of effects) dispose();
"#,
        serde_json::to_string(&path.to_string_lossy()).unwrap()
    );
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .output()
        .expect("Node is required to test the generated DSH plugin");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generated_client_reconciles_the_leading_selection_hint() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = AgentContext {
        integrator: &integrator,
        detected: &detected,
    };
    let files = plugin_files(&ctx).unwrap();
    let client = files
        .iter()
        .find(|file| file.kind == "client-entry")
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("client.js");
    fs::write(&path, &client.bytes).unwrap();
    let script = format!(
        r#"
const assert = require('node:assert/strict');
const fs = require('node:fs');
let loaded;
globalThis.window = {{
  __ModuleLoader__: {{
    load(module) {{
      loaded = module.factory(name => name === 'react' ? {{}} : {{}});
    }},
  }},
}};
eval(fs.readFileSync({}, 'utf8'));
const {{ hintFor, reconcile }} = loaded.__test;
assert.equal(hintFor(1), '/ossia 1 selection selected.');
assert.equal(hintFor(2), '/ossia 2 selections selected.');
assert.equal(loaded.__test.translate('zh', 'hintOne', {{ count: 1 }}), '/ossia 已选择 1 个对象。');
const zhT = (key, params) => loaded.__test.translate('zh', key, params);
assert.equal(hintFor(2, zhT), '/ossia 已选择 2 个对象。');
assert.deepEqual(reconcile('', 2), {{
  start: 0,
  end: 0,
  text: '/ossia 2 selections selected.',
}});
assert.deepEqual(reconcile('Fix this', 1), {{
  start: 0,
  end: 0,
  text: '/ossia 1 selection selected.\n\n',
}});
const old = '/ossia 1 selection selected.\n\nFix this';
assert.deepEqual(reconcile(old, 2), {{
  start: 0,
  end: '/ossia 1 selection selected.'.length,
  text: '/ossia 2 selections selected.',
}});
assert.deepEqual(reconcile('/ossia 2 selections selected.\n\nFix this', 2, zhT), {{
  start: 0,
  end: '/ossia 2 selections selected.'.length,
  text: '/ossia 已选择 2 个对象。',
}});
assert.deepEqual(reconcile('/ossia 已选择 2 个选区。\n\nFix this', 2, zhT), {{
  start: 0,
  end: '/ossia 已选择 2 个选区。'.length,
  text: '/ossia 已选择 2 个对象。',
}});
const clear = '/ossia 2 selections selected.\n\nFix this';
assert.deepEqual(reconcile(clear, 0), {{
  start: 0,
  end: '/ossia 2 selections selected.\n\n'.length,
  text: '',
}});
assert.deepEqual(reconcile('/ossia 1 selection selected.', 0), {{
  start: 0,
  end: '/ossia 1 selection selected.'.length,
  text: '',
}});
assert.equal(reconcile('Fix this', 0), null);
assert.equal(reconcile(clear, 2), null);
"#,
        serde_json::to_string(&path.to_string_lossy()).unwrap()
    );
    let output = std::process::Command::new("node")
        .args(["-e", &script])
        .output()
        .expect("Node is required to test the generated DSH plugin");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn linked_profile(target: &Path) -> Value {
    json!({
        "name": "dsh-profile-desktop",
        "private": true,
        "dsh": {
            "profile": {
                "bundles": [PLUGIN]
            }
        },
        "dependencies": {
            PLUGIN: format!("link:{}", target.display())
        }
    })
}

fn write_profile(integrator: &Integrator, profile: &Value) -> PathBuf {
    let path = integrator
        .paths()
        .dsh_home
        .join("profiles/desktop/package.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(profile).unwrap()).unwrap();
    path
}

fn context<'a>(integrator: &'a Integrator, detected: &'a DetectedAgent) -> AgentContext<'a> {
    AgentContext {
        integrator,
        detected,
    }
}

#[test]
fn deleted_bundle_target_is_broken_and_reinstallable() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = context(&integrator, &detected);
    let expected = bundle_dir(&ctx);
    write_profile(&integrator, &linked_profile(&expected));

    let inspection = status(&ctx, None).unwrap();
    assert_eq!(inspection.state, AgentState::Broken);
    assert_eq!(inspection.message, Some(AgentNotice::DshBundleMissing));
    assert!(!inspection.installed);
    assert!(inspection.conflicts.is_empty());

    let entries = install(&ctx, None).unwrap();
    assert!(expected.join("package.json").is_file());
    assert!(!entries.is_empty());
    let record = ManagedRecord {
        management_id: "ossia:dsh:desktop".into(),
        agent_id: "dsh".into(),
        method: InstallMethod::Plugin,
        profile: "desktop".into(),
        template_version: TEMPLATE_VERSION,
        home: integrator.home().to_string_lossy().into_owned(),
        executable: integrator.executable().to_string_lossy().into_owned(),
        shared_root: integrator.shared_root().to_string_lossy().into_owned(),
        updated_at: "unix:0".into(),
        entries,
    };
    let inspection = status(&ctx, Some(&record)).unwrap();
    assert_eq!(inspection.state, AgentState::Installed);
}

#[test]
fn existing_unmanaged_bundle_remains_a_conflict() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = context(&integrator, &detected);
    let expected = bundle_dir(&ctx);
    write_profile(&integrator, &linked_profile(&expected));
    fs::create_dir_all(&expected).unwrap();

    let inspection = status(&ctx, None).unwrap();
    assert_eq!(inspection.state, AgentState::Conflict);
    assert_eq!(inspection.message, Some(AgentNotice::DshUnmanagedBundle));
    assert_eq!(inspection.conflicts.len(), 1);
    assert!(install(&ctx, None).is_err());
}

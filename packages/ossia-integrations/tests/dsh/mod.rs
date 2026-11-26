use super::*;
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, Integrator) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("library");
    let shared = root.path().join("shared");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&shared).unwrap();
    let executable = root.path().join("ossia.exe");
    fs::write(&executable, b"test executable").unwrap();
    let paths = crate::HostPaths {
        codex_home: root.path().join("codex"),
        dsh_home: root.path().join("dsh"),
        workbuddy_home: root.path().join("home"),
        workbuddy_apps: Vec::new(),
        codex_bin: None,
        dsh_bin: None,
        launch_env: BTreeMap::from([(
            "OSSIA_ENGINE_ROOT".into(),
            format!(r"\\?\{}", root.path().join("engine").display()),
        )]),
    };
    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let integrator =
        Integrator::with_resource_dir(home, executable, resources, paths, DEFAULT_BACKEND_PORT)
            .unwrap()
            .with_shared_root(shared)
            .unwrap();
    (root, integrator)
}

#[test]
fn plugin_package_declares_both_halves_and_absolute_mcp_command() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        command: std::env::current_exe().unwrap(),
        command_args: Vec::new(),
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
    assert!(package["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file == "skills/ossia/references/*.ly"));

    let config = files
        .iter()
        .find(|file| file.kind == "host-config")
        .unwrap();
    let config: Value = serde_json::from_slice(&config.bytes).unwrap();
    assert_eq!(
        config["home"],
        serde_json::json!(path_text(integrator.home()))
    );
    assert!(!config.to_string().contains(r"\\?\"));
    let host = files.iter().find(|file| file.kind == "host-entry").unwrap();
    assert!(String::from_utf8_lossy(&host.bytes).contains("/ossia-selection"));
    let client = files
        .iter()
        .find(|file| file.kind == "client-entry")
        .unwrap();
    assert!(String::from_utf8_lossy(&client.bytes).contains("insertText"));
    assert_eq!(
        files
            .iter()
            .filter(|file| file.kind == "skill-reference")
            .count(),
        4
    );

    let patch = files
        .iter()
        .find(|file| file.kind == "bundle-patch")
        .unwrap();
    let patch = String::from_utf8_lossy(&patch.bytes);
    assert!(patch.contains("@deepseek-ai/dsh-mcp-client"));
    assert!(patch.contains("transport: stdio"));
    assert!(patch.contains(&yaml_string(&path_text(integrator.executable()))));
    assert!(patch.contains(&DEFAULT_BACKEND_PORT.to_string()));
    assert!(!patch.contains(r"\\?\"));
    assert!(patch.contains("ossia-dsh-plugin/skill"));
    assert!(patch.contains("name: 'ossia-dsh-plugin'"));
}

#[test]
fn generated_skill_loads_with_host_metadata_and_disposes_registration() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        command: std::env::current_exe().unwrap(),
        command_args: Vec::new(),
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
        command: std::env::current_exe().unwrap(),
        command_args: Vec::new(),
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
assert.deepEqual(
  module.__test.parseSelectionCounts({{
    type: 'selection-changed',
    documentId: 'document',
    payload: {{ documentTitle: '心湖', targetCount: 2, annotationCount: 3 }},
  }}),
  {{ selectionCount: 2, annotationCount: 3, documentId: 'document', documentTitle: '心湖' }},
);
assert.equal(
  module.__test.parseSelectionCounts({{ type: 'selection-changed', payload: {{ targetCount: -1 }} }}),
  null,
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
assert.equal(parsed.result.value.selectionCount, null);
assert.equal(parsed.result.value.annotationCount, null);
assert.equal(parsed.result.value.documentTitle, null);
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
fn generated_client_reconciles_the_leading_annotation_hint() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        command: std::env::current_exe().unwrap(),
        command_args: Vec::new(),
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
assert.equal(hintFor(0), null);
assert.equal(hintFor(1), '/ossia Added 1 annotation.');
assert.equal(hintFor(2), '/ossia Added 2 annotations.');
assert.equal(hintFor(1, '心湖'), '/ossia 「心湖」 Added 1 annotation.');
assert.equal(loaded.__test.translate('zh', 'selectionOne', {{ count: 1 }}), '/ossia 已选择 1 个对象。');
assert.equal(loaded.__test.translate('zh', 'annotationOne', {{ count: 1 }}), '/ossia 已添加 1 个标注。');
const zhT = (key, params) => loaded.__test.translate('zh', key, params);
assert.equal(hintFor(2, null, zhT), '/ossia 已添加 2 个标注。');
assert.equal(hintFor(1, '心湖', zhT), '/ossia 「心湖」已添加 1 个标注。');
assert.equal(hintFor(2, '心湖', zhT), '/ossia 「心湖」已添加 2 个标注。');
assert.equal(hintFor(0, null, zhT), null);
assert.equal(reconcile('', 0), null);
assert.deepEqual(reconcile('', 2), {{
  start: 0,
  end: 0,
  text: '/ossia Added 2 annotations.',
}});
assert.deepEqual(reconcile('', 1, '心湖'), {{
  start: 0,
  end: 0,
  text: '/ossia 「心湖」 Added 1 annotation.',
}});
assert.deepEqual(reconcile('Fix this', 1), {{
  start: 0,
  end: 0,
  text: '/ossia Added 1 annotation.\n\n',
}});
const old = '/ossia 1 selection selected.\n\nFix this';
assert.deepEqual(reconcile(old, 2, '心湖'), {{
  start: 0,
  end: '/ossia 1 selection selected.'.length,
  text: '/ossia 「心湖」 Added 2 annotations.',
}});
assert.deepEqual(reconcile('/ossia 2 selections selected.\n\nFix this', 2, '心湖', zhT), {{
  start: 0,
  end: '/ossia 2 selections selected.'.length,
  text: '/ossia 「心湖」已添加 2 个标注。',
}});
assert.deepEqual(reconcile('/ossia 已选择 2 个选区。\n\nFix this', 0, null, zhT), {{
  start: 0,
  end: '/ossia 已选择 2 个选区。\n\n'.length,
  text: '',
}});
assert.deepEqual(reconcile('/ossia Selected 2 objects, added 3 annotations.\n\nFix this', 3, '心湖'), {{
  start: 0,
  end: '/ossia Selected 2 objects, added 3 annotations.'.length,
  text: '/ossia 「心湖」 Added 3 annotations.',
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
assert.deepEqual(reconcile('/ossia Added 2 annotations.', 0), {{
  start: 0,
  end: '/ossia Added 2 annotations.'.length,
  text: '',
}});
assert.equal(reconcile('Fix this', 0), null);
assert.deepEqual(reconcile(clear, 2), {{
  start: 0,
  end: '/ossia 2 selections selected.'.length,
  text: '/ossia Added 2 annotations.',
}});
assert.deepEqual(reconcile('/ossia 「心湖」已添加 1 个标注。\n\nFix this', 1, '归途', zhT), {{
  start: 0,
  end: '/ossia 「心湖」已添加 1 个标注。'.length,
  text: '/ossia 「归途」已添加 1 个标注。',
}});
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

fn context<'a>(integrator: &'a Integrator, detected: &'a DetectedAgent) -> AgentContext<'a> {
    AgentContext {
        integrator,
        detected,
    }
}

#[test]
fn dependency_matches_accepts_legacy_verbatim_links() {
    let (_root, integrator) = fixture();
    let detected = DetectedAgent {
        executable: std::env::current_exe().unwrap(),
        command: std::env::current_exe().unwrap(),
        command_args: Vec::new(),
        version: "test".into(),
        config_root: integrator.paths().dsh_home.clone(),
        profile: "desktop",
        env: BTreeMap::new(),
    };
    let ctx = context(&integrator, &detected);
    let expected = bundle_dir(&ctx);
    let expected_text = path_text(&expected);
    let profile = profile_path(&ctx);
    let base = profile.parent().unwrap();
    let verbatim = format!(r"\\?\{}", expected_text);
    let forward = format!("//?/{}", expected_text.replace('\\', "/"));

    assert!(dependency_has_verbatim_prefix(&format!("link:{verbatim}")));
    assert!(dependency_matches(
        &format!("link:{verbatim}"),
        &expected,
        base
    ));
    assert!(dependency_has_verbatim_prefix(&format!("link:{forward}")));
    assert!(dependency_matches(
        &format!("link:{forward}"),
        &expected,
        base
    ));
    assert!(!dependency_has_verbatim_prefix(&format!(
        "link:{expected_text}"
    )));
}

#[path = "lifecycle.rs"]
mod lifecycle_tests;

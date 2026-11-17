//! A Linux pin's program is remembered across an app restart by `pin.json`'s pid, start time and
//! boot id. A fresh engine's session run list is empty, so only the pin's record can find it.

#![cfg(target_os = "linux")]

use craft_anything_engine::workspace::{TargetRef, Workspace};
use craft_anything_engine::{CallOptions, Caller, Consent, Engine, pins};
use craft_anything_platform::process::Launch;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

fn call(engine: &Engine, id: &str, params: Value) -> Value {
    engine.call(id, params, CallOptions::new(Caller::Cli).with_consent(Consent::Granted)).unwrap().to_json()["result"].clone()
}

fn workspace(dir: &Path, script: &str) -> Workspace {
    let ws = Workspace::create(dir, "Game", "a game", TargetRef::None).unwrap();
    let app = ws.app_dir();
    std::fs::create_dir_all(app.join("target/debug")).unwrap();
    std::fs::write(app.join("Cargo.toml"), "[package]\nname = \"game\"\n").unwrap();
    let program = app.join("target/debug/game");
    std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    ws
}

fn boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap().trim().to_string()
}

/// `/proc/<pid>/stat` field 22: the start time the production reader uses.
fn start_token(pid: u32) -> String {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let (_, rest) = text.rsplit_once(')').unwrap();
    rest.split_whitespace().nth(19).unwrap().to_string()
}

fn wait_gone(pid: u32) {
    let path = format!("/proc/{pid}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while Path::new(&path).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!Path::new(&path).exists(), "process {pid} is gone");
}

fn pin_file(ws: &Workspace, id: u32) -> PathBuf {
    pins::get(ws, id).unwrap().dir(ws).join(pins::PIN_FILE)
}

/// A direct-file record: enough to test a mismatch without the production open.
fn pinned_with_run(ws: &Workspace) -> (u32, PathBuf, craft_anything_platform::Launched) {
    let engine = Engine::new(craft_anything_engine::testing::session());
    let made = call(&engine, "project_pin_create", json!({"project": ws.root}));
    let id = made["pin"]["id"].as_u64().unwrap() as u32;
    let pin = pins::get(ws, id).unwrap();
    let file = pin.dir(ws).join(pins::PIN_FILE);
    let running = Launch::new("sh").args(["-c", "sleep 30"]).cwd(pin.app_dir(ws)).start_watched().unwrap();
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    doc["version"] = json!(3);
    doc["runs"] = json!([{"pid": running.pid(), "start": start_token(running.pid()), "boot": boot_id(), "members": []}]);
    std::fs::write(&file, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    (id, file, running)
}

#[test]
fn a_pin_opened_by_the_production_path_survives_a_restart() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "sleep 30");
    let engine = Engine::new(craft_anything_engine::testing::session());
    let made = call(&engine, "project_pin_create", json!({"project": ws.root}));
    let id = made["pin"]["id"].as_u64().unwrap() as u32;
    let opened = call(&engine, "project_pin_open", json!({"project": ws.root, "pin": id}));
    assert_eq!(opened["already_running"], false, "{opened}");
    let pid = opened["pid"].as_u64().unwrap() as u32;
    let file = pin_file(&ws, id);
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["runs"][0]["pid"], pid, "the production open wrote the record: {saved}");
    assert!(!saved["runs"][0]["boot"].as_str().unwrap().is_empty(), "{saved}");

    // A fresh engine stands in for the restarted app: its session run list knows nothing.
    let restarted = Engine::new(craft_anything_engine::testing::session());
    let list = call(&restarted, "project_pins_list", json!({"project": ws.root}));
    assert_eq!(list["pins"][0]["running"], true, "{list}");
    let stopped = call(&restarted, "project_pin_stop", json!({"project": ws.root, "pin": id}));
    assert!(stopped["stopped"].as_u64().unwrap() >= 1, "{stopped}");
    wait_gone(pid);
    let after = call(&restarted, "project_pins_list", json!({"project": ws.root}));
    assert_eq!(after["pins"][0]["running"], false, "{after}");
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["runs"], json!([]), "the stopped run is forgotten");
}

#[test]
fn a_different_start_time_is_neither_believed_nor_stopped() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "sleep 30");
    let (id, file, running) = pinned_with_run(&ws);
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    doc["runs"][0]["start"] = json!("0");
    std::fs::write(&file, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();

    let restarted = Engine::new(craft_anything_engine::testing::session());
    let list = call(&restarted, "project_pins_list", json!({"project": ws.root}));
    assert_eq!(list["pins"][0]["running"], false, "a reused pid isn't the pin's process: {list}");
    let stopped = call(&restarted, "project_pin_stop", json!({"project": ws.root, "pin": id}));
    assert_eq!(stopped["stopped"], 0, "a wrong start token never kills: {stopped}");
    assert!(running.is_running(), "the stand-in is still there");
    running.stop();
}

#[test]
fn a_record_without_a_boot_id_is_not_trusted() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "sleep 30");
    let (id, file, running) = pinned_with_run(&ws);
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    doc["runs"][0].as_object_mut().unwrap().remove("boot");
    std::fs::write(&file, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();

    let restarted = Engine::new(craft_anything_engine::testing::session());
    let list = call(&restarted, "project_pins_list", json!({"project": ws.root}));
    assert_eq!(list["pins"][0]["running"], false, "an old record without a boot is not trusted: {list}");
    let stopped = call(&restarted, "project_pin_stop", json!({"project": ws.root, "pin": id}));
    assert_eq!(stopped["stopped"], 0, "{stopped}");
    assert!(running.is_running(), "the stand-in is still there");
    running.stop();
}

#[test]
fn a_group_member_that_ignores_term_is_killed() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "trap 'exit 0' TERM\nsh -c 'trap \"\" TERM; sleep 30' &\necho $! > child.pid\nwait");
    let engine = Engine::new(craft_anything_engine::testing::session());
    let made = call(&engine, "project_pin_create", json!({"project": ws.root}));
    let id = made["pin"]["id"].as_u64().unwrap() as u32;
    let opened = call(&engine, "project_pin_open", json!({"project": ws.root, "pin": id}));
    let leader = opened["pid"].as_u64().unwrap() as u32;
    let child: u32 = std::fs::read_to_string(pins::get(&ws, id).unwrap().app_dir(&ws).join("child.pid")).unwrap().trim().parse().unwrap();

    let restarted = Engine::new(craft_anything_engine::testing::session());
    let stopped = call(&restarted, "project_pin_stop", json!({"project": ws.root, "pin": id}));
    assert!(stopped["stopped"].as_u64().unwrap() >= 1, "{stopped}");
    wait_gone(child);
    wait_gone(leader);
}

#[test]
fn a_live_child_keeps_a_pin_recorded_after_its_leader_exits() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "sh -c 'trap \"\" TERM; sleep 30' &\necho $! > child.pid\nsleep 3");
    let engine = Engine::new(craft_anything_engine::testing::session());
    let made = call(&engine, "project_pin_create", json!({"project": ws.root}));
    let id = made["pin"]["id"].as_u64().unwrap() as u32;
    let opened = call(&engine, "project_pin_open", json!({"project": ws.root, "pin": id}));
    let leader = opened["pid"].as_u64().unwrap() as u32;
    let child: u32 = std::fs::read_to_string(pins::get(&ws, id).unwrap().app_dir(&ws).join("child.pid")).unwrap().trim().parse().unwrap();

    // The leader leaves on its own after the open; the child ignores TERM and stays in the group.
    wait_gone(leader);
    let restarted = Engine::new(craft_anything_engine::testing::session());
    let list = call(&restarted, "project_pins_list", json!({"project": ws.root}));
    assert_eq!(list["pins"][0]["running"], true, "the live child keeps the run recorded: {list}");
    let stopped = call(&restarted, "project_pin_stop", json!({"project": ws.root, "pin": id}));
    assert!(stopped["stopped"].as_u64().unwrap() >= 1, "{stopped}");
    wait_gone(child);
    let after = call(&restarted, "project_pins_list", json!({"project": ws.root}));
    assert_eq!(after["pins"][0]["running"], false, "{after}");
    let file = pin_file(&ws, id);
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["runs"], json!([]), "the stopped group is forgotten");
}

#[test]
fn a_quick_exit_leader_with_a_live_child_is_recorded() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "sh -c 'trap \"\" TERM; sleep 30' &\necho $! > child.pid\nsleep 0.2\nexit 0");
    let engine = Engine::new(craft_anything_engine::testing::session());
    let made = call(&engine, "project_pin_create", json!({"project": ws.root}));
    let id = made["pin"]["id"].as_u64().unwrap() as u32;
    let opened = call(&engine, "project_pin_open", json!({"project": ws.root, "pin": id}));
    assert_eq!(opened["closed_at_once"]["code"], 0, "the leader quit at once: {opened}");
    let child: u32 = std::fs::read_to_string(pins::get(&ws, id).unwrap().app_dir(&ws).join("child.pid")).unwrap().trim().parse().unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(pin_file(&ws, id)).unwrap()).unwrap();
    assert!(saved["runs"][0]["members"].as_array().unwrap().iter().any(|m| m["pid"] == json!(child)), "the discovered child is stored in the run: {saved}");

    let restarted = Engine::new(craft_anything_engine::testing::session());
    let list = call(&restarted, "project_pins_list", json!({"project": ws.root}));
    assert_eq!(list["pins"][0]["running"], true, "the child keeps the quick-exit run recorded: {list}");
    let stopped = call(&restarted, "project_pin_stop", json!({"project": ws.root, "pin": id}));
    assert!(stopped["stopped"].as_u64().unwrap() >= 1, "{stopped}");
    wait_gone(child);
}

#[test]
fn two_engines_opening_the_same_pin_keep_both_runs() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "sleep 5");
    let setup = Engine::new(craft_anything_engine::testing::session());
    let made = call(&setup, "project_pin_create", json!({"project": ws.root}));
    let id = made["pin"]["id"].as_u64().unwrap() as u32;
    let e1 = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let e2 = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let barrier = Arc::new(Barrier::new(2));
    let open = |engine: Arc<Engine>, barrier: Arc<Barrier>| {
        let root = ws.root.clone();
        std::thread::spawn(move || {
            barrier.wait();
            call(&engine, "project_pin_open", json!({"project": root, "pin": id, "another": true}))
        })
    };
    let h1 = open(e1.clone(), barrier.clone());
    let h2 = open(e2, barrier);
    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();
    assert_eq!(r1["already_running"], false, "{r1}");
    assert_eq!(r2["already_running"], false, "{r2}");
    let file = pin_file(&ws, id);
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["runs"].as_array().unwrap().len(), 2, "both opens kept their run: {saved}");
    let stopped = call(&e1, "project_pin_stop", json!({"project": ws.root, "pin": id}));
    assert!(stopped["stopped"].as_u64().unwrap() >= 1, "{stopped}");
}

#[test]
fn a_newer_pin_is_refused_before_launch_or_save() {
    let d = tempfile::tempdir().unwrap();
    let ws = workspace(d.path(), "printf marker > marker.txt");
    let engine = Engine::new(craft_anything_engine::testing::session());
    let made = call(&engine, "project_pin_create", json!({"project": ws.root}));
    let id = made["pin"]["id"].as_u64().unwrap() as u32;
    let pin = pins::get(&ws, id).unwrap();
    let file = pin.dir(&ws).join(pins::PIN_FILE);
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    doc["version"] = json!(9);
    doc["colour"] = json!("red");
    std::fs::write(&file, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();

    let opts = || CallOptions::new(Caller::Cli).with_consent(Consent::Granted);
    let err = engine.call("project_pin_open", json!({"project": ws.root, "pin": id}), opts()).unwrap_err();
    assert!(err.message.contains("newer"), "{err:?}");
    assert!(!pin.app_dir(&ws).join("marker.txt").exists(), "no launch before the version check");
    let err = engine.call("project_pin_rename", json!({"project": ws.root, "pin": id, "name": "X"}), opts()).unwrap_err();
    assert!(err.message.contains("newer"), "{err:?}");
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["version"], 9);
    assert_eq!(saved["colour"], "red", "the newer file is preserved");
}

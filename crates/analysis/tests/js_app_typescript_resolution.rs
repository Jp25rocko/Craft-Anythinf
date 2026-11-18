//! Regression tests for TypeScript declaration filtering (`.d.mts`, `.d.cts`), NodeNext
//! `.js`/`.mjs`/`.cjs`/`.jsx` → `.ts`/`.tsx`/`.mts`/`.cts` source fallback, `.mts`/`.cts`/directory
//! `index` imports, and Electron preload TypeScript path resolution in `js_app`.

use craft_anything_analysis::js_app::{AppFile, AppGraph, AppInput, AppOptions, analyze_application, is_script};

/// Build an `AppGraph` from in-memory `(path, content)` pairs and optional extra listing entries.
fn build_graph(files: &[(&str, &str)], extra_listed: &[&str]) -> AppGraph {
    let app_files: Vec<AppFile> = files.iter().map(|(p, c)| AppFile { path: (*p).to_string(), bytes: c.as_bytes().to_vec() }).collect();
    let mut listing: Vec<(String, u64)> = app_files.iter().map(|f| (f.path.clone(), f.bytes.len() as u64)).collect();
    for p in extra_listed {
        listing.push(((*p).to_string(), 64));
    }
    let input = AppInput { label: "ts-fixture".into(), format: "directory".into(), files: app_files, listing, notes: Vec::new() };
    analyze_application(&input, &AppOptions::default(), &|| false).expect("analysis should not cancel")
}

/// `.d.ts`, `.d.mts`, and `.d.cts` declaration files are excluded from script modules and never resolved as import targets.
#[test]
fn declaration_files_d_mts_and_d_cts_are_excluded_from_scripts_and_resolved_edges() {
    assert!(!is_script("types.d.ts"));
    assert!(!is_script("TYPES.D.TS"));
    assert!(!is_script("esm.d.mts"));
    assert!(!is_script("ESM.D.MTS"));
    assert!(!is_script("cjs.d.cts"));
    assert!(!is_script("CJS.D.CTS"));
    assert!(is_script("real.mts"));
    assert!(is_script("real.cts"));

    let g = build_graph(
        &[
            ("package.json", r#"{"name":"decl-check","main":"./main.mts"}"#),
            (
                "main.mts",
                r#"
                import "./types.d.ts";
                import "./esm.d.mts";
                import "./cjs.d.cts";
                import "./UPPER.D.MTS";
                import "./shim.d.js";
                export const ok = 1;
                "#,
            ),
            (
                "index.html",
                r#"<!doctype html>
                <link rel="modulepreload" href="./cjs.d.cts">
                <link rel="modulepreload" href="./preloaded.js">
                <script type="module" src="./esm.d.mts"></script>
                "#,
            ),
            ("preloaded.ts", "export const p = 1;\n"),
            ("types.d.ts", "export interface Note { id: string; }\n"),
            ("esm.d.mts", "export declare function load(): void;\n"),
            ("cjs.d.cts", "export declare const version: string;\n"),
            ("UPPER.D.MTS", "export declare const upper: boolean;\n"),
            ("shim.d.ts", "export declare const shim: number;\n"),
            ("esm.d.mts.map", r#"{"version":3,"sources":["esm.mts"],"mappings":""}"#),
        ],
        &[],
    );

    let mut module_paths: Vec<&str> = g.modules.iter().map(|m| m.path.as_str()).collect();
    module_paths.sort_unstable();
    assert_eq!(module_paths, ["index.html", "main.mts", "preloaded.ts"]);
    let main_mod = g.module("main.mts").expect("main.mts module");
    assert!(main_mod.imports.iter().all(|imp| imp.resolved.is_none()), "declaration imports must not resolve: {:?}", main_mod.imports);
    let html_mod = g.module("index.html").expect("index.html module");
    assert_eq!(html_mod.imports[0].resolved, None, "html-script to .d.mts must not resolve");
    assert_eq!(html_mod.imports[1].resolved, None, "modulepreload to .d.cts must not resolve");
    assert_eq!(html_mod.imports[2].resolved.as_deref(), Some("preloaded.ts"));
    assert!(g.edges.iter().all(|e| !e.to.to_ascii_lowercase().contains(".d.")), "edges must not target declaration files: {:?}", g.edges);
}

/// TypeScript NodeNext `.js`, `.mjs`, `.cjs`, and `.jsx` specifiers resolve to `.ts`/`.tsx`/`.mts`/`.cts` sources while preferring exact matches and `.ts` before `.tsx`.
#[test]
fn typescript_nodenext_js_mjs_cjs_and_jsx_specifiers_resolve_to_source_counterparts() {
    let g = build_graph(
        &[
            ("package.json", r#"{"name":"ts-nodenext","main":"./src/main.js"}"#),
            (
                "src/main.ts",
                r#"
                import "./service.js";
                import "./View.js";
                import "./helper.mjs";
                import "./Widget.jsx";
                import "./both.js";
                import "./prefer_ts.js";
                const legacy = require("./legacy.cjs");
                new Worker(new URL("./bg.mjs", import.meta.url));
                "#,
            ),
            ("src/service.ts", "export const svc = 1;\n"),
            ("src/View.tsx", "export const View = () => <div/>;\n"),
            ("src/helper.mts", "export const h = 2;\n"),
            ("src/legacy.cts", "module.exports = { ok: true };\n"),
            ("src/Widget.tsx", "export const Widget = () => <span/>;\n"),
            ("src/both.js", "export const built = true;\n"),
            ("src/both.ts", "export const unbuilt = true;\n"),
            ("src/prefer_ts.ts", "export const tsFirst = 1;\n"),
            ("src/prefer_ts.tsx", "export const tsxSecond = 2;\n"),
            ("src/bg.mts", "self.onmessage = () => {};\n"),
        ],
        &[],
    );

    assert_eq!(g.entry_points.main.as_deref(), Some("src/main.ts"));
    assert_eq!(g.entry_points.workers, vec!["src/bg.mts"]);

    for expected_pkg in
        ["src/main.ts", "src/service.ts", "src/View.tsx", "src/helper.mts", "src/legacy.cts", "src/Widget.tsx", "src/both.js", "src/prefer_ts.ts"]
    {
        assert_eq!(g.module(expected_pkg).expect(expected_pkg).role, "package", "role mismatch on {expected_pkg}");
    }
    assert_eq!(g.module("src/both.ts").expect("src/both.ts").role, "unknown", "exact src/both.js must win over src/both.ts");
    assert_eq!(g.module("src/prefer_ts.tsx").expect("src/prefer_ts.tsx").role, "unknown", ".ts must win before .tsx");
    assert_eq!(g.module("src/bg.mts").expect("src/bg.mts").role, "worker");
}

/// Extensionless and directory `index` relative imports resolve `.mts`, `.cts`, and `index.*` while keeping compiled `index.mjs` ahead of `index.ts`.
#[test]
fn extensionless_and_directory_index_imports_resolve_mts_cts_tsx_jsx_and_cjs() {
    let g = build_graph(
        &[
            ("package.json", r#"{"name":"extless","main":"./entry.ts"}"#),
            (
                "entry.ts",
                r#"
                import "./bare_mts";
                const b = require("./bare_cts");
                const c = require("./dir_cjs");
                import "./dir_tsx";
                import "./dir_mts";
                const d = require("./dir_cts");
                import "./dir_jsx";
                import "./dir_both";
                "#,
            ),
            ("bare_mts.mts", "export const a = 1;\n"),
            ("bare_cts.cts", "module.exports = 2;\n"),
            ("dir_cjs/index.cjs", "module.exports = 3;\n"),
            ("dir_tsx/index.tsx", "export const C = () => <p/>;\n"),
            ("dir_mts/index.mts", "export const m = 4;\n"),
            ("dir_cts/index.cts", "module.exports = 5;\n"),
            ("dir_jsx/index.jsx", "export const J = () => <b/>;\n"),
            ("dir_both/index.mjs", "export const compiled = 6;\n"),
            ("dir_both/index.ts", "export const unbuilt = 7;\n"),
        ],
        &[],
    );

    for target in [
        "bare_mts.mts",
        "bare_cts.cts",
        "dir_cjs/index.cjs",
        "dir_tsx/index.tsx",
        "dir_mts/index.mts",
        "dir_cts/index.cts",
        "dir_jsx/index.jsx",
        "dir_both/index.mjs",
    ] {
        assert_eq!(g.module(target).expect(target).role, "package", "missing package role for {target}");
        assert!(
            g.edges.iter().any(|e| e.from == "module:entry.ts" && e.to == format!("module:{target}")),
            "missing dependency edge to {target}: {:?}",
            g.edges
        );
    }
    assert_eq!(g.module("dir_both/index.ts").expect("dir_both/index.ts").role, "unknown");
}

/// Electron `webPreferences.preload` prefers exact paths across `module_dir` and `root` before TypeScript fallbacks and rejects exact/cross-extension tail ambiguity and declarations.
#[test]
fn electron_preload_resolves_compiled_paths_to_typescript_sources() {
    let g = build_graph(
        &[
            ("package.json", r#"{"name":"el-ts","main":"./src/main/index.js","devDependencies":{"electron":"^33.0.0"}}"#),
            (
                "src/main/index.ts",
                r#"
                import { BrowserWindow, ipcMain } from "electron";
                import path from "node:path";

                ipcMain.handle("notes:ping", () => "pong");

                const w1 = new BrowserWindow({
                    webPreferences: { preload: path.join(__dirname, "../preload/index.js") }
                });
                w1.loadFile("renderer/index.html");

                const w2 = new BrowserWindow({
                    webPreferences: { preload: "bridge.mjs" }
                });
                const w3 = new BrowserWindow({
                    webPreferences: { preload: "legacy_preload.cjs" }
                });
                const w4 = new BrowserWindow({
                    webPreferences: { preload: "../preload/both_preload.js" }
                });
                const w5 = new BrowserWindow({
                    webPreferences: { preload: "root_exact.js" }
                });
                const w6 = new BrowserWindow({
                    webPreferences: { preload: "ambig.js" }
                });
                const w7 = new BrowserWindow({
                    webPreferences: { preload: "cross_ambig.js" }
                });
                const w8 = new BrowserWindow({
                    webPreferences: { preload: "exact_dup.js" }
                });
                const w9 = new BrowserWindow({
                    webPreferences: { preload: "local_decl.d.js" }
                });
                const w10 = new BrowserWindow({
                    webPreferences: { preload: "decl_only.d.js" }
                });
                const w11 = new BrowserWindow({
                    webPreferences: { preload: "UPPER_DECL.D.CTS" }
                });
                "#,
            ),
            (
                "src/preload/index.ts",
                r#"
                import { contextBridge, ipcRenderer } from "electron";
                contextBridge.exposeInMainWorld("api", {
                    ping: () => ipcRenderer.invoke("notes:ping"),
                });
                "#,
            ),
            ("packages/bridge/bridge.mts", "import { contextBridge } from 'electron'; contextBridge.exposeInMainWorld('b', {});\n"),
            ("src/main/legacy_preload.cts", "const { contextBridge } = require('electron'); contextBridge.exposeInMainWorld('c', {});\n"),
            ("src/preload/both_preload.js", "const { contextBridge } = require('electron'); contextBridge.exposeInMainWorld('d', {});\n"),
            ("src/preload/both_preload.ts", "import { contextBridge } from 'electron'; contextBridge.exposeInMainWorld('d_ts', {});\n"),
            ("root_exact.js", "const { contextBridge } = require('electron'); contextBridge.exposeInMainWorld('root_js', {});\n"),
            ("src/main/root_exact.ts", "import { contextBridge } from 'electron'; contextBridge.exposeInMainWorld('local_ts', {});\n"),
            ("a/ambig.ts", "export const a = 1;\n"),
            ("b/ambig.ts", "export const b = 2;\n"),
            ("a/cross_ambig.ts", "export const ca = 1;\n"),
            ("b/cross_ambig.tsx", "export const cb = () => <div/>;\n"),
            ("a/exact_dup.js", "export const e1 = 1;\n"),
            ("b/exact_dup.js", "export const e2 = 2;\n"),
            ("c/exact_dup.ts", "export const e3 = 3;\n"),
            ("src/main/local_decl.d.ts", "export declare const localDecl: number;\n"),
            ("nested/decl_only.d.ts", "export declare const x: number;\n"),
            ("nested/UPPER_DECL.D.CTS", "export declare const u: number;\n"),
            ("renderer/index.html", r#"<!doctype html><script type="module" src="./app.jsx"></script>"#),
            ("renderer/app.tsx", "window.api.ping();\n"),
        ],
        &[],
    );

    assert_eq!(g.app_kind, "electron");
    assert_eq!(g.entry_points.main.as_deref(), Some("src/main/index.ts"));
    assert_eq!(
        g.entry_points.preloads,
        vec!["src/preload/index.ts", "packages/bridge/bridge.mts", "src/main/legacy_preload.cts", "src/preload/both_preload.js", "root_exact.js",]
    );
    assert_eq!(g.module("src/preload/index.ts").expect("preload ts").role, "preload");
    assert_eq!(g.module("packages/bridge/bridge.mts").expect("bridge mts").role, "preload");
    assert_eq!(g.module("src/main/legacy_preload.cts").expect("legacy cts").role, "preload");
    assert_eq!(g.module("src/preload/both_preload.js").expect("both preload js").role, "preload");
    assert_eq!(g.module("src/preload/both_preload.ts").expect("both preload ts").role, "unknown");
    assert_eq!(g.module("root_exact.js").expect("root exact js").role, "preload");
    assert_eq!(g.module("src/main/root_exact.ts").expect("module_dir ts").role, "unknown");
    assert_eq!(g.module("renderer/app.tsx").expect("renderer tsx").role, "renderer");
    for (idx, win) in g.windows.iter().enumerate().take(11).skip(5) {
        assert!(win["preload"].is_null(), "window {idx} preload must stay unresolved: {win:?}");
    }

    let ch = g.ipc.iter().find(|c| c.channel == "notes:ping").expect("notes:ping channel");
    assert!(ch.paired);
}

/// Ordinary JavaScript and JSON resolution behavior remains unchanged.
#[test]
fn ordinary_javascript_and_json_resolution_control() {
    let g = build_graph(
        &[
            ("package.json", r#"{"name":"plain-js","main":"index.js"}"#),
            ("index.js", "require('./lib');\nrequire('./config.json');\n"),
            ("lib/index.js", "module.exports = 1;\n"),
            ("config.json", r#"{"ok":true}"#),
        ],
        &[],
    );

    assert_eq!(g.entry_points.main.as_deref(), Some("index.js"));
    assert_eq!(g.module("lib/index.js").expect("lib/index.js").role, "package");
    let main_mod = g.module("index.js").expect("index.js");
    assert_eq!(main_mod.imports[0].resolved.as_deref(), Some("lib/index.js"));
    assert_eq!(main_mod.imports[1].resolved.as_deref(), Some("config.json"));
}

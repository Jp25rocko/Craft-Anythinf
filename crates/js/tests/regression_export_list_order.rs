//! `export { f }` marks the function it names as exported. The list was matched against the
//! functions seen so far, by name, so a list before the declaration marked nothing and one after it
//! also marked nested functions that only shared the name.

use craft_anything_js::{Analysis, Limits, ParseMode, analyze};

fn parsed(src: &str) -> Analysis {
    let analysis = analyze("app.mjs", src, &Limits::default());
    assert_eq!(analysis.facts.mode, ParseMode::Parsed, "{:?}", analysis.facts.mode_reason);
    analysis
}

/// `(name, parent, exported)` for every function.
fn functions(a: &Analysis) -> Vec<(String, Option<String>, bool)> {
    a.facts.functions.iter().map(|f| (f.name.clone(), f.parent.clone(), f.exported)).collect()
}

fn exported(a: &Analysis, name: &str) -> bool {
    a.facts.functions.iter().any(|f| f.name == name && f.parent.is_none() && f.exported)
}

#[test]
fn a_list_before_the_declaration() {
    let a = parsed("export { f };\nfunction f() { return 1; }\nfunction g() {}\n");
    assert!(exported(&a, "f"), "{:?}", functions(&a));
    assert!(!exported(&a, "g"), "{:?}", functions(&a));
}

#[test]
fn a_renamed_list_before_bound_expressions() {
    let a = parsed("export { f as run, h as default };\nconst f = () => 1;\nvar h = function named() {};\n");
    assert!(exported(&a, "f"), "{:?}", functions(&a));
    assert!(exported(&a, "h"), "{:?}", functions(&a));
}

/// Only the module-level `f` is exported; a nested function of the same name is a different one.
#[test]
fn nested_functions_with_the_same_name_are_not_exported() {
    for src in [
        "function outer() { function f() {} return f; }\nfunction f() {}\nexport { f };\n",
        "export { f };\nfunction outer() { const f = () => 2; return f; }\nfunction f() {}\n",
        "function outer() { return [1].map(function f() {}); }\nfunction f() {}\nexport { f };\n",
    ] {
        let a = parsed(src);
        let all = functions(&a);
        assert!(exported(&a, "f"), "{all:?}");
        let nested = all.iter().find(|(n, p, _)| n == "f" && p.is_some()).expect("the nested f");
        assert!(!nested.2, "{src}: {all:?}");
    }
}

/// Lists after the declaration, and functions given to a name later, read as before.
#[test]
fn lists_after_the_declaration_still_work() {
    let a = parsed("function f() {}\nconst h = function () {};\nexport { f, h };\n");
    assert!(exported(&a, "f") && exported(&a, "h"), "{:?}", functions(&a));
    let a = parsed("let f;\nf = function () {};\nexport { f };\n");
    assert!(exported(&a, "f"), "{:?}", functions(&a));
    let a = parsed("export function f() {}\nfunction g() {}\n");
    assert!(exported(&a, "f") && !exported(&a, "g"), "{:?}", functions(&a));
}

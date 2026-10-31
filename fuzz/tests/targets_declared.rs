//! Every fuzz target source is a declared cargo-fuzz binary, and every
//! declared binary has a source (pvlqq.10.1.3). An undeclared target never
//! builds, so it can silently keep importing crates that no longer exist.

use std::collections::BTreeSet;
use std::path::Path;

fn declared_paths(manifest: &str) -> BTreeSet<String> {
    manifest
        .lines()
        .filter_map(|line| line.trim().strip_prefix("path = \"fuzz_targets/"))
        .filter_map(|rest| rest.strip_suffix('"'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_fuzz_target_source_is_a_declared_binary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("fuzz manifest");
    let declared = declared_paths(&manifest);
    let sources: BTreeSet<String> = std::fs::read_dir(root.join("fuzz_targets"))
        .expect("fuzz_targets directory")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".rs"))
        .collect();
    assert!(!declared.is_empty(), "no [[bin]] fuzz targets declared");
    assert_eq!(
        sources.difference(&declared).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "fuzz target sources not declared as [[bin]] in fuzz/Cargo.toml"
    );
    assert_eq!(
        declared.difference(&sources).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "declared fuzz targets without a source file"
    );
}

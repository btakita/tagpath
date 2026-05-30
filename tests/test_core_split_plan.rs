#[test]
fn core_split_plan_documents_required_boundaries() {
    let spec = include_str!("../SPEC.md");
    for required in [
        "## 18. Workspace split",
        "`tagpath-core`",
        "`tagpath::parser::parse`",
        "`tagpath::alias::generate_aliases`",
        "`tagpath::family::generate_family`",
        "`tagpath::prose::to_prose`",
        "`tagpath::query::normalize_query`",
        "`tagpath::compression::build_report`",
        "It must not depend on `clap`",
        "Publish `tagpath-core`",
    ] {
        assert!(spec.contains(required), "SPEC.md missing `{required}`");
    }
}

#[test]
fn core_split_plan_inventory_mentions_native_facade_modules() {
    let spec = include_str!("../SPEC.md");
    for module in [
        "`tagpath::config`",
        "`tagpath::extract`",
        "`tagpath::search`",
        "`tagpath::lint`",
        "`tagpath::index` / `tagpath::meta_index`",
        "`tagpath::ontology`",
        "`tagpath::graph`",
        "`tagpath::rename`",
        "`tagpath::mcp`",
        "`tagpath::treesitter`",
        "`tagpath::watch`",
        "`tagpath::wasm`",
    ] {
        assert!(spec.contains(module), "SPEC.md missing `{module}`");
    }
}

#[test]
fn workspace_manifest_contains_core_dependency_boundary() {
    let root_manifest = include_str!("../Cargo.toml");
    assert!(root_manifest.contains("members = [\".\", \"crates/tagpath-core\"]"));
    assert!(
        root_manifest
            .contains("tagpath-core = { version = \"=0.12.0\", path = \"crates/tagpath-core\" }")
    );

    let core_manifest = include_str!("../crates/tagpath-core/Cargo.toml");
    assert!(core_manifest.contains("name = \"tagpath-core\""));
    assert!(core_manifest.contains("serde = { version = \"1\", features = [\"derive\"] }"));
    for forbidden in [
        "clap",
        "regex",
        "toml",
        "tree-sitter",
        "petgraph",
        "walkdir",
        "sha2",
        "bincode",
        "wasm-bindgen",
        "notify",
        "libc",
    ] {
        assert!(
            !core_manifest.contains(forbidden),
            "tagpath-core must not depend on `{forbidden}`"
        );
    }
}

#[test]
fn facade_reexports_core_modules_by_name() {
    let lib = include_str!("../src/lib.rs");
    assert!(
        lib.contains("pub use tagpath_core::{alias, compression, family, parser, prose, query};")
    );
}

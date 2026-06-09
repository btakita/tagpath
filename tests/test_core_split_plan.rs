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
    assert!(root_manifest
        .contains("tagpath-core = { version = \"=0.12.2\", path = \"crates/tagpath-core\" }"));

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
fn root_manifest_preserves_facade_package_shape_and_feature_defaults() {
    let root_manifest = include_str!("../Cargo.toml");
    for required in [
        "name = \"tagpath\"",
        "version = \"0.12.2\"",
        "crate-type = [\"cdylib\", \"rlib\"]",
        "name = \"tagpath\"\npath = \"src/main.rs\"",
        "required-features = [\"treesitter\"]",
        "default = [\"lang-rust\", \"lang-python\", \"lang-javascript\", \"lang-typescript\", \"lang-go\", \"lang-c\", \"lang-cpp\", \"lang-java\", \"lang-ruby\", \"lang-php\", \"lang-csharp\", \"lang-swift\", \"lang-kotlin\", \"mcp\", \"watch\"]",
        "mcp = []",
        "wasm = [\"dep:wasm-bindgen\", \"dep:serde-wasm-bindgen\", \"dep:js-sys\"]",
        "watch = [\"dep:notify\", \"dep:libc\"]",
        "dyn-grammar = [\"treesitter\", \"dep:libloading\", \"dep:tree-sitter-language\"]",
    ] {
        assert!(root_manifest.contains(required), "Cargo.toml missing `{required}`");
    }
}

#[test]
fn facade_reexports_core_modules_by_name() {
    let lib = include_str!("../src/lib.rs");
    assert!(
        lib.contains("pub use tagpath_core::{alias, compression, family, parser, prose, query};")
    );
}

#[test]
fn ci_workflows_cover_split_release_checks() {
    let ci = include_str!("../.github/workflows/ci.yml");
    for required in [
        "cargo clippy --workspace --all-targets -- -D warnings",
        "cargo test --workspace",
        "cargo test -p tagpath-core --no-default-features",
        "cargo test -p tagpath --lib --no-default-features",
        "cargo build --target wasm32-unknown-unknown --no-default-features --features wasm",
        "scripts/check-release.sh",
        "targets: wasm32-unknown-unknown",
    ] {
        assert!(ci.contains(required), "ci.yml missing `{required}`");
    }

    let wasm = include_str!("../.github/workflows/wasm-build.yml");
    for required in ["./scripts/build-wasm.sh", "node pkg-smoke/smoke.mjs"] {
        assert!(
            wasm.contains(required),
            "wasm-build.yml missing `{required}`"
        );
    }

    let release = include_str!("../.github/workflows/release.yml");
    for required in [
        "needs: checks",
        "cargo clippy --workspace --all-targets -- -D warnings",
        "cargo test --workspace",
        "cargo test -p tagpath-core --no-default-features",
        "cargo test -p tagpath --lib --no-default-features",
        "cargo build --target wasm32-unknown-unknown --no-default-features --features wasm",
        "scripts/check-release.sh",
    ] {
        assert!(
            release.contains(required),
            "release.yml missing `{required}`"
        );
    }

    let release_check = include_str!("../scripts/check-release.sh");
    for required in [
        "cargo publish --dry-run --allow-dirty -p tagpath-core",
        "cargo publish --dry-run --allow-dirty -p tagpath",
        "no matching package named.*tagpath-core",
        "TAGPATH_RELEASE_CHECK_STRICT_FACADE",
        "Publish order: cargo publish -p tagpath-core",
    ] {
        assert!(
            release_check.contains(required),
            "check-release.sh missing `{required}`"
        );
    }
}

#[test]
fn adapter_crate_decisions_are_documented() {
    let spec = include_str!("../SPEC.md");
    for required in [
        "### 18.7 Adapter crate decision",
        "do not add separate Rust crates named",
        "`tagpath-wasm`, `tagpath-mcp`, or `tagpath-project`",
        "`tagpath::wasm` remains the wasm-bindgen adapter",
        "`@btakita/tagpath-wasm` remains an npm package produced by",
        "`tagpath::mcp` stays in the root facade",
        "`tagpath-project` is deferred",
        "Revisit adapter crates only after `tagpath-core` 0.12.x",
    ] {
        assert!(spec.contains(required), "SPEC.md missing `{required}`");
    }

    let readme = include_str!("../README.md");
    for required in [
        "No separate Rust crates named `tagpath-wasm`, `tagpath-mcp`, or",
        "`tagpath-project` are part of the first split",
        "The wasm-bindgen adapter,",
        "MCP server, and project/index/search surfaces remain in the root facade",
        "The npm package `@btakita/tagpath-wasm`",
        "`scripts/build-wasm.sh`",
    ] {
        assert!(readme.contains(required), "README.md missing `{required}`");
    }

    let versions = include_str!("../VERSIONS.md");
    for required in [
        "Documented the adapter-crate decision",
        "`tagpath-wasm`, `tagpath-mcp`,",
        "`tagpath-project` stay deferred",
        "surfaces remain rooted in the `tagpath` facade",
    ] {
        assert!(
            versions.contains(required),
            "VERSIONS.md missing `{required}`"
        );
    }
}

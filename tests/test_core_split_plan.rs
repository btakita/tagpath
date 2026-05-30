#[test]
fn core_split_plan_documents_required_boundaries() {
    let spec = include_str!("../SPEC.md");
    for required in [
        "## 18. Workspace split plan",
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

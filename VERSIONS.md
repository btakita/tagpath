# tagpath versions

## v0.12.0 — Unreleased

### Internal

- Split the pure identifier semantics modules into a new `tagpath-core`
  workspace crate: parser, alias, family, prose, query normalization, and
  compression. The root `tagpath` crate remains the CLI/library facade
  and preserves existing imports such as `tagpath::parser::parse`.
- Added facade regression coverage for root package shape, feature
  defaults, old public type paths, and CLI commands backed by
  `tagpath-core`.

## v0.11.1 — 2026-05-25

Backwards-compat patch for the agent-doc dialect.

### Lint rule changes

- `agent-doc/unknown-component` no longer fires on the legacy component names that agent-doc still accepts as migration inputs: `agent:pending`, `agent:pending-done`, `agent:backlog-done`. The fix-hint still suggests canonical names; legacy forms are accepted silently so legacy session docs do not fail closed before `agent-doc migrate` runs (`p6adftestfix`).

Unblocks agent-doc CI run `26385513733` (and successors) which were red on ~14 `finalize_*` integration tests using legacy `agent:pending` fixtures.

## v0.11.0 — 2026-05-25

First release since v0.10.0 (2025). Versions 0.11–0.18 were tagged locally in `Cargo.toml` during incremental development but never published; this release consolidates the work into a single 0.11.0 publish on crates.io.

### Features

- `tagpath alias` — cross-convention identifier translation (snake_case ↔ camelCase ↔ PascalCase ↔ Ada_Case ↔ kebab-case ↔ UPPER_SNAKE_CASE).
- `tagpath prose` — natural-language descriptions of identifiers.
- `tagpath graph` — tag co-occurrence graph with DOT/JSON output (petgraph).
- `tagpath index` — persistent `.naming/index.json` for instant queries; bincode sidecar cache for incremental updates (`p5inc`, `p5xfast`).
- `tagpath ontology` — `.naming/tags/*.md` domain vocabulary.
- `tagpath watch` — NDJSON file event stream (`p5wat`).
- `tagpath search` with stable family handles + JSONL emit for consumers (`p5tsi`).
- `tagpath lint --dialect agent-doc` — validates agent-doc session-document tags (`p5sln`).
- `tagpath mcp install` — config installer for Claude Desktop, Claude Code, Codex, OpenCode, and Cursor (`p6mcr`).
- `tagpath mcp` (stdio JSON-RPC 2.0 server) — exposes parse / normalize_query / lint / search / ontology_lookup / indexed_project_query.
- Dynamic `.so` grammar loading via `libloading`, opt-in `dyn-grammar` feature.
- WASM build via `wasm` feature (tree-sitter gated behind `cfg(not(target_arch = "wasm32"))`).

### Lint rule changes

- `agent-doc/malformed-boundary` accepts `<hex>` or `<hex>:<slug>` (lowercase a-z0-9-) — matches what `agent_doc::new_boundary_id_with_summary` emits (`p6lintbnd`).

### Internal

- 14 tree-sitter grammars behind `lang-*` feature flags.
- `cargo install` works on all 5 release targets via GitHub Actions.
- LICENSE-MIT added (`p5lic`).
- WASM package surface at `@btakita/tagpath-wasm` (`p5wnp`).
- MCP convention helpers for tsift/agent-doc consumers (`p5mcs`).
- Cross-references tsift as a reference consumer (`p6tsi`).

## v0.10.0 and earlier

See git tag history (`v0.1.0` through `v0.10.0`).

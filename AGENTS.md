# tagpath

Tag Path — parse, lint, and search tag-based identifiers across languages.

## Architecture

```
src/
  main.rs              CLI entrypoint (clap)
  parser/mod.rs        Convention detection, tokenization, role/shape detection
  config/mod.rs        .naming.toml schema, loading, extends resolution
  lint/mod.rs          Lint engine — validates identifiers against config rules
  extract/mod.rs       Identifier extraction from source files (regex + tree-sitter)
  search/mod.rs        Cross-convention semantic search over extracted identifiers
  treesitter/mod.rs    Tree-sitter grammar loading and AST walking
lang/                  Language presets (39 languages, TOML format)
presets/               Convention presets (immutable-tag.toml)
```

## Conventions

- **Rust edition 2024**
- **Dependencies:** clap (CLI), serde + toml (config), serde_json (output), regex (extraction), walkdir (file traversal), tree-sitter + grammar crates (AST extraction)
- **No async** — all operations are synchronous
- **Tabs for indentation** (match existing code)
- Run `cargo test` before committing
- Run `cargo clippy` for lint checks

## Module Responsibilities

- **parser** — Stateless functions. Input: string + optional convention. Output: `ParsedName` with tags, namespaces, role, shape. No I/O.
- **config** — .naming.toml schema types and deserialization. `load()` reads from disk. `generate_config()` produces TOML from presets. `extends` resolution merges parent configs with overrides.
- **lint** — Validates identifiers against .naming.toml rules. Uses parser + config. Reports violations per file with context and expected convention.
- **extract** — Walks source files and extracts identifiers. Regex-based extraction for all languages, tree-sitter AST extraction for 8 supported languages. Outputs identifier + file location + context.
- **search** — Semantic search across extracted identifiers. Decomposes query into canonical tags, matches against all extracted identifiers regardless of naming convention.
- **treesitter** — Loads tree-sitter grammars, parses source into AST, walks nodes to extract identifiers with context classification (function, type, variable, etc.).
- **main** — CLI dispatch only. No business logic.

## Key Design Decisions

- Tags are always lowercase in output (normalization happens in `parse()`)
- Convention detection is heuristic: underscore → snake, dash → kebab, leading uppercase → Pascal, else → camel
- Mixed conventions (e.g., `createContext_auth`) split on underscores first, then apply camelCase splitting per segment
- `__` is the namespace separator; dimensions are extracted only for snake_case/UPPER_SNAKE_CASE
- `extends` resolution: extending config fields override parent fields; context-level keys merge (only specified fields replace parent values)
- AST extraction preferred over regex when tree-sitter grammar is available — provides context classification and avoids false positives from strings/comments
- Search uses canonical tag matching — query is parsed into tags, then matched against extracted identifiers by tag subsequence

## Release Process

1. Bump version in `Cargo.toml`
2. `cargo test && cargo clippy`
3. `git tag v<version> && git push --tags`
4. `cargo publish`

## Phases

- **Phase 1** ✅: Parse, detect, semantic equivalence, CLI
- **Phase 2** ✅: tree-sitter integration, lint command, extract identifiers, semantic search, composable configs
- **Phase 3**: Graph building, alias generation, prose variant

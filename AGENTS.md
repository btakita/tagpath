# tagpath

Tag Path — parse, lint, and search tag-based identifiers across languages.

## Architecture

```
src/
  main.rs          CLI entrypoint (clap)
  parser/mod.rs    Convention detection, tokenization, role/shape detection
  config/mod.rs    .naming.toml schema and loading
  lint/mod.rs      Lint engine (Phase 2)
lang/              Language presets (39 languages, TOML format)
presets/           Convention presets (immutable-tag.toml)
```

## Conventions

- **Rust edition 2024**
- **Dependencies:** clap (CLI), serde + toml (config), serde_json (output)
- **No async** — all operations are synchronous
- **Tabs for indentation** (match existing code)
- Run `cargo test` before committing
- Run `cargo clippy` for lint checks

## Module Responsibilities

- **parser** — Stateless functions. Input: string + optional convention. Output: `ParsedName` with tags, namespaces, role, shape. No I/O.
- **config** — .naming.toml schema types and deserialization. `load()` reads from disk. `generate_config()` produces TOML from presets.
- **lint** — (Phase 2) Validates identifiers against .naming.toml rules. Uses parser + config.
- **main** — CLI dispatch only. No business logic.

## Key Design Decisions

- Tags are always lowercase in output (normalization happens in `parse()`)
- Convention detection is heuristic: underscore → snake, dash → kebab, leading uppercase → Pascal, else → camel
- Mixed conventions (e.g., `createContext_auth`) split on underscores first, then apply camelCase splitting per segment
- `__` is the namespace separator; dimensions are extracted only for snake_case/UPPER_SNAKE_CASE

## Release Process

1. Bump version in `Cargo.toml`
2. `cargo test && cargo clippy`
3. `git tag v<version> && git push --tags`
4. `cargo publish`

## Phases

- **Phase 1** (current): Parse, detect, semantic equivalence, CLI
- **Phase 2**: tree-sitter integration, lint command, extract identifiers from source
- **Phase 3**: Graph building, search, alias generation, prose variant

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
  alias/mod.rs         Cross-convention alias generation for identifiers
  prose/mod.rs         Human-readable prose descriptions of identifiers
  graph/mod.rs         Tag co-occurrence graph (petgraph) with DOT/JSON output
lang/                  Language presets (39 languages, TOML format)
presets/               Convention presets (immutable-tag.toml)
```

## Conventions

- **Rust edition 2024**
- **Dependencies:** clap (CLI), serde + toml (config), serde_json (output), regex (extraction), walkdir (file traversal), tree-sitter + grammar crates (AST extraction), petgraph (graph building)
- **No async** — all operations are synchronous
- **Tabs for indentation** (match existing code)
- Run `cargo test` before committing
- Run `cargo clippy` for lint checks

## Module Responsibilities

- **parser** — Stateless functions. Input: string + optional convention. Output: `ParsedName` with tags, namespaces, role, shape. No I/O.
- **config** — .naming.toml schema types and deserialization. `load()` reads from disk. `generate_config()` produces TOML from presets. `extends` resolution merges parent configs with overrides.
- **lint** — Validates identifiers against .naming.toml rules. Uses parser + config. Reports violations per file with context and expected convention.
- **extract** — Walks source files and extracts identifiers. Regex-based extraction for all languages, tree-sitter AST extraction for 14 supported languages. Outputs identifier + file location + context.
- **search** — Semantic search across extracted identifiers. Decomposes query into canonical tags, matches against all extracted identifiers regardless of naming convention.
- **treesitter** — Loads tree-sitter grammars, parses source into AST, walks nodes to extract identifiers with context classification (function, type, variable, etc.).
- **alias** — Cross-convention alias generation. Parses an identifier into canonical tags, then reconstructs it in all 6 naming conventions (or a single target convention).
- **prose** — Human-readable prose conversion. Strips role prefixes and shape suffixes from tags, then generates natural English descriptions (e.g., `create_user_profile` -> "Creates a user profile").
- **graph** — Tag co-occurrence graph using petgraph. Builds a directed graph where nodes are tags and edges connect sequential tag pairs within identifiers. Supports DOT and JSON output, with optional query-based subgraph filtering.
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

1. Bump the lockstep version in `Cargo.toml` and
   `crates/tagpath-core/Cargo.toml`.
2. Run `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo test -p tagpath-core
   --no-default-features`, `cargo test -p tagpath --lib
   --no-default-features`, and the wasm target build.
3. Run `scripts/check-release.sh`.
4. Publish `tagpath-core` first.
5. Rerun `scripts/check-release.sh`, then publish `tagpath`.
6. Tag the release only after both crates publish successfully.

## Phases

- **Phase 1** ✅: Parse, detect, semantic equivalence, CLI
- **Phase 2** ✅: tree-sitter integration, lint command, extract identifiers, semantic search, composable configs
- **Phase 3** ✅: Alias generation (`tagpath alias`), prose conversion (`tagpath prose`), tag co-occurrence graph (`tagpath graph`)


## Library Context Policy

This library follows the agent-loop library-context policy. Contributors
authoring `AGENTS.md`, `SKILL.md`, or runbooks in this repo must read:

[Library Context Policy](../instruction-files/LIBRARY_CONTEXT_POLICY.md)

before making changes.

<!-- tsift:code-navigation v=0.1.64 -->
## Code Navigation

Keep this block self-contained for Codex/OpenCode prompt reuse. If this repository also ships current `.claude/skills/tsift/SKILL.md` or `runbooks/code-navigation.md`, use those deeper runbooks for command detail instead of expanding this block.

Run `tsift status` at session start from the owning repo root. If the task or file lives under a git submodule (for example `src/tsift/...`), switch to that submodule root first so the harness loads the narrower local instructions and repo state instead of the superproject root. If status prints a `run:` recommendation for stale or missing tsift state, run `tsift status --fix` before relying on tsift results; when the harness cannot perform write commands, ask the user to run the printed command instead. Codex projects can install a prompt-time auto-reindex hook with `tsift init --codex`; OpenCode projects can install per-project tsift command shortcuts with `tsift init --opencode`.

Use the commands listed in its `use:` output:
- `tsift --envelope source-read <file> --budget normal` — AST-symbol projection with span metadata and source-window expansion commands (prefer over cat/head for source code files)
- `tsift --envelope symbol-read <symbol> --budget normal` — token-budgeted symbol body, AST span metadata, child refs, and graph/source expansion commands
- `tsift --envelope search <query> --budget normal` — AST-aware hybrid search preview (prefer over grep/rg)
- `tsift --envelope explain <symbol> --budget normal` — callers, callees, community preview
- `tsift graph <symbol> --callers` / `--callees` — call graph navigation
- `tsift summarize <symbol>` — cached summary (only when listed in `use:`)
- `tsift workflow search` — ordered exact/search/explain/summarize/digest recipe that preserves result handles across expansions

When a search envelope includes `report.scale_guard`, run one of its `narrow_commands` before dispatching parallel agents. The guard means the original result set or corpus is broad enough that fan-out should start from a narrower cited handle, path, or exact query.

Prefer bounded digest commands over raw transcript, diff, and verbose-log reads:
- `tsift --envelope session-review <path> --next-context --budget normal` or `tsift --envelope context-pack <path> --budget normal` instead of replaying long session docs, JSONL transcripts, or agent-doc runtime logs with `cat`, `tail`, or `sed`.
- `tsift diff-digest [path]` (`--cached`, `--revision <rev>`) instead of `git diff`, `git show`, or patch-style `git log`.
- `tsift --envelope digest-runner --kind test --path . --shell-command '<test command>'` / `tsift --envelope digest-runner --kind log --path . --shell-command '<build command>'` for noisy test/build/install output, or let the rewrite/hooks create those artifact-backed envelopes for `cargo test`, `pytest`, and verbose cargo commands.
- If RTK is installed, digest-runner delegates supported generic command families through `rtk rewrite` and records the chosen compact filter in `report.filter` while preserving tsift artifact handles.
- Codex, OpenCode, and other harnesses without Claude-style `PreToolUse` hooks should run `tsift rewrite --run '<command>'` before broad `rg`/recursive grep, raw transcript/session/log reads, `git diff`/`git show`/single-patch `git log`, `cargo test`/`pytest`, and cargo build/check/clippy/install commands so the same search, session-digest, diff-digest, and digest-runner rewrites apply manually. OpenCode can install this path as `/tsift-rewrite-run` with `tsift init --opencode`.

For local verification, run `make check` before committing. After local changes, check the latest GitHub Actions CI run with `gh run list --workflow CI --limit 1` and fix any failing tests before calling the work complete.

Only read full source files when tsift results are insufficient.
<!-- /tsift:code-navigation -->

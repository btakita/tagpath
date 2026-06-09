# Tag Path Specification

## 1. Overview

Tag Path is a system for decomposing identifiers into canonical tag sequences, enabling semantic equivalence across naming conventions and languages.

Every identifier is a **path** — an ordered sequence of **tags** separated by convention-specific delimiters. The same concept expressed in different conventions produces the same canonical tag list.

## 2. Conventions

Tag Path recognizes six naming conventions:

| Convention | Example | Delimiter |
|-----------|---------|-----------|
| snake_case | `person_name` | `_` |
| camelCase | `personName` | case boundary |
| PascalCase | `PersonName` | case boundary |
| kebab-case | `person-name` | `-` |
| UPPER_SNAKE_CASE | `PERSON_NAME` | `_` |
| Ada_Case | `Person_Name` | `_` |

### 2.1 Convention Detection

Detection is heuristic, applied in order:

1. Contains `_` or `__` AND all uppercase → UPPER_SNAKE_CASE
2. Contains `_` or `__` AND every segment starts with uppercase → Ada_Case
3. Contains `_` or `__` → snake_case
4. Contains `-` → kebab-case
5. Starts with uppercase letter → PascalCase
6. Otherwise → camelCase

### 2.2 Mixed Conventions

Identifiers may mix conventions (e.g., `createContext_auth`). Tokenization always splits on underscores first, then applies camelCase splitting to each segment.

## 3. Tokenization

### 3.1 snake_case / UPPER_SNAKE_CASE

Split on `_` (single underscore). Double underscore `__` is a namespace separator (see Section 5).

### 3.2 camelCase / PascalCase

Split on case boundaries:
- Lowercase → uppercase: `personName` → `[person, Name]`
- Uppercase run → lowercase (acronym boundary): `HTMLElement` → `[HTML, Element]`

### 3.3 kebab-case

Split on `-`.

### 3.4 Normalization

All tags are lowercased in the output. The canonical form is the lowercase tag list joined by `_`.

## 4. Semantic Equivalence

Two identifiers are semantically equivalent if they produce the same canonical tag list:

```
person_name  → [person, name]
personName   → [person, name]
PersonName   → [person, name]
person-name  → [person, name]
PERSON_NAME  → [person, name]
Person_Name  → [person, name]
```

All six are equivalent.

## 5. Namespace Dimensions

In snake_case and UPPER_SNAKE_CASE, `__` (double underscore) separates namespace dimensions:

```
auth0__user__validate → dimensions: [[auth0], [user], [validate]]
highest_net_worth__company_person_name → dimensions: [[highest, net, worth], [company, person, name]]
```

Namespace dimensions are not extracted for camelCase, PascalCase, or kebab-case.

## 6. Role Detection

Roles are detected from prefix/suffix patterns:

| Pattern | Role |
|---------|------|
| `create_*`, `make_*`, `new_*`, `build_*` | factory |
| `use_*` | hook |
| `set_*` | setter |
| `get_*` | getter |
| `is_*`, `has_*`, `can_*`, `should_*` | predicate |
| `on_*` | handler |
| `validate_*`, `check_*`, `verify_*` | validator |
| `*_validate`, `*_check`, `*_verify` | validator (suffix) |

## 7. Shape Detection

Data shapes are detected from the last tag:

| Suffix | Shape |
|--------|-------|
| `a`, `a1`, `a2`, `a3`, `list`, `array` | array |
| `r`, `record` | record |
| `m`, `map` | map |
| `set` (when first tag is not `set`) | set |
| `$` (trailing) | signal |

## 8. Configuration (.naming.toml)

### 8.1 Schema

```toml
version = 1              # Schema version (required)
name = "<string>"        # Project/config name (required)
extends = ["<string>"]   # Parent configs to inherit from
convention = "<string>"  # Default convention
immutable = <bool>       # Tags never mutate when composing
singular = <bool>        # Tags are always singular form

[vectors]
join = "_"               # Tag join character
namespace = "__"         # Namespace separator

[patterns]
<role> = "<template>"    # Role-specific name templates

[externals]
preserve_casing = <bool>   # Keep external library casing
join_with = "<string>"     # How to join external names

[packages]
separator = "<string>"    # Package name separator
pattern = "<template>"    # Package naming template

[lint]
allow_mixed_within_identifier = <bool>  # Compare mixed-surface identifiers by tag equivalence

[contexts.<context_name>]
convention = "<string>"   # Convention for this context
prefix = "<string>"       # Optional prefix
suffix = "<string>"       # Optional suffix

[tags]
open = <bool>             # Allow undeclared tags

[tags.declared.<tag_name>]
level = "<string>"        # abstraction level
domain = "<string>"       # domain classification
shape = "<string>"        # data shape
role = "<string>"         # functional role
```

### 8.2 Resolution

When multiple `.naming.toml` files exist in a directory hierarchy, they merge bottom-up (closest to the file wins). The `extends` field pulls in named presets.

## 9. CLI Interface

```
tagpath parse <NAME> [--convention <CONV>] [--format text|json]
tagpath init [--lang <LANG>] [--preset <PRESET>]
tagpath extract <PATH> [--format text|json|family|family-json] [--ast]
tagpath search <QUERY> <PATH> [--format text|json|family|family-json]
tagpath lint [<PATH>]
tagpath alias <NAME> [--convention <CONV>] [--format text|json]
tagpath family <NAME> [--format text|json]
tagpath compression-report [<JSON>|-] [--format text|json] [--example-limit <N>]
tagpath prose <NAME> [--format text|json]
tagpath normalize-query <QUERY> [--format text|json]
tagpath ontology [<PATH>] [--format text|json]
tagpath graph [<PATH>] [--format text|dot|json] [--query <QUERY>]
tagpath index [<PATH>] [--check] [--force] [--emit json|jsonl] [--schema-version]
tagpath search <QUERY> <PATH> [--index]
tagpath rename <OLD> <NEW> [<PATH>] [--dry-run] [--format text|json]
tagpath meta-index [<WORKSPACE_ROOT>] [--output <PATH>] [--format text|json]
```

### 9.1 parse

Decomposes an identifier into its tag structure. Auto-detects convention unless overridden.

### 9.2 init

Generates a `.naming.toml` from a language or convention preset.

### 9.3 extract

Extracts identifiers from source files under `<PATH>`.

- Recursively walks directories, selecting files by known language extensions.
- **Regex mode** (default): Uses regex patterns to extract identifiers from source text. Works for all 39 supported languages.
- **AST mode** (`--ast`): Uses tree-sitter to parse source files into an AST and extract identifiers with context classification. Available for 14 languages (Rust, Python, JavaScript, TypeScript, TSX, Go, C, C++, Java, Ruby, PHP, C#, Swift, Kotlin). Falls back to regex for unsupported languages.
- Each extracted identifier includes: name, file path, line number, detected convention, canonical tags, and context (when using `--ast`).
- `--format text` (default) outputs one identifier per line. `--format json` outputs a JSON array of identifier records.
- `--format family` groups identifiers by canonical tag sequence and outputs one compact family per group with the occurrence count, tags, role/shape metadata, and up to three representative examples.
- `--format family-json` outputs the same grouped family summaries as JSON. This is intended for agent-facing callers that need a compact overview instead of every spelling and source location.

### 9.4 search

Performs cross-convention semantic search over source files under `<PATH>`.

- The `<QUERY>` is parsed into canonical tags using the same tokenization rules as `parse`.
- All identifiers in `<PATH>` are extracted and decomposed into canonical tags.
- An identifier matches if the query's canonical tags appear as a subsequence of the identifier's canonical tags.
- Matches across all naming conventions: searching for `"user"` finds `user_name`, `userName`, `UserName`, `user-name`, and `USER_NAME`.
- Searching for `"validate_user"` finds `validateUser`, `ValidateUser`, `validate_user`, etc.
- `--format text` (default) outputs matching identifiers with file location. `--format json` outputs a JSON array.
- `--format family` groups matching identifiers by canonical tag sequence and outputs the occurrence count plus a few representative examples per family.
- `--format family-json` outputs the same grouped search-family summaries as JSON.

### 9.5 lint

Validates source file identifiers against `.naming.toml` rules.

- Loads the nearest `.naming.toml` (with `extends` resolution) for each file.
- Extracts identifiers from source files (uses tree-sitter AST when available).
- Checks each identifier's convention against the expected convention for its context.
- Reports violations with file path, line number, identifier name, expected convention, and actual convention.

### 9.6 alias

Generates cross-convention aliases for an identifier.

- Parses the input identifier into canonical tags using the same tokenization rules as `parse`.
- Reconstructs the identifier in all 6 naming conventions: snake_case, camelCase, PascalCase, kebab-case, UPPER_SNAKE_CASE, Ada_Case.
- Optional `--convention` flag to produce only a single target convention.
- `--format text` (default) outputs one convention per line. `--format json` outputs a JSON object with `tags` and `aliases` fields.

### 9.7 family

Generates a stable semantic tag family for an identifier.

- Parses the input identifier into canonical tags using the same tokenization rules as `parse`.
- Emits one stable canonical handle for the whole tag sequence, using lowercase tags joined by `_`.
- Preserves namespace dimensions from `__` as ordered dimension records with per-dimension canonical handles.
- Includes detected role and shape metadata when present.
- Generates surface spelling examples in all 6 naming conventions: snake_case, camelCase, PascalCase, kebab-case, UPPER_SNAKE_CASE, Ada_Case.
- `--format text` (default) outputs the canonical handle, tags, dimensions, role/shape metadata, and examples. `--format json` outputs a JSON object with `original`, `canonical`, `tags`, `dimensions`, `role`, `shape`, `aliases`, and `examples` fields.

This schema is intended for callers such as `tsift` that need to collapse repeated identifier variants into one compact, stable semantic handle.

### 9.8 compression-report

Groups raw symbol preview rows into compact tag families and reports deterministic savings metrics for downstream tools such as `tsift`.

- Input is a JSON array of raw symbol rows, or an object containing `raw_symbols` or `rows`.
- Each row requires `identifier`, `file`, and `line`; `column` defaults to `0`, and `context` is optional.
- The report parses each identifier through convention detection, groups rows by canonical tags, generates all six aliases per family, and keeps up to `--example-limit` representative examples per family.
- The text output prints raw symbol count, family count, byte savings, token-estimate savings, and the compact family preview rows. The JSON output includes the grouped `families`, `raw_preview`, `compact_preview`, and `metrics`.
- Token estimates use `ceil(utf8_bytes / 4)`, matching the tsift token-savings fixture without depending on a model-specific tokenizer.

### 9.9 prose

Generates a human-readable prose description of an identifier.

- Parses the input identifier into canonical tags.
- Strips role prefixes (`create`, `get`, `is`, etc.) and shape suffixes (`a`, `list`, `map`, etc.) from the core tags.
- Builds a natural English phrase based on role and shape:
  - Factory: "Creates a {noun}" (e.g., `create_user_profile` -> "Creates a user profile")
  - Predicate: "Checks if {subject} {predicate} {modifiers}" (e.g., `is_valid_email` -> "Checks if email is valid")
  - Array shape: "Array of {noun}s" (e.g., `user_name_a` -> "Array of user names")
  - No role/shape: capitalizes the noun phrase (e.g., `PersonName` -> "Person name")
- `--format text` (default) outputs the prose string. `--format json` outputs a JSON object with `original`, `prose`, `tags`, `role`, and `shape` fields.

### 9.10 normalize-query

Normalizes a free-text agent prompt or search phrase into ordered, weighted canonical tags.

- Tokenizes ordinary prose and identifier-shaped fragments from the query.
- Parses identifier-shaped tokens (`raw_symbol`, `session-review`, `validateUser`, `auth0__user__validate`) through the same convention detection and tag parsing rules as `parse`.
- Keeps ordinary non-stopword prose terms as lowercase tags.
- Aggregates repeated tag mentions, giving identifier-shaped source tokens higher weight than ordinary prose terms.
- Emits tags sorted by descending weight, then first appearance. This gives callers such as `tsift` a compact semantic query signal before lexical or hybrid fallback.
- `--format text` (default) outputs one tag per line with weight, occurrence count, and contributing source tokens. `--format json` outputs `{ original, tags }`, where each tag includes `tag`, `weight`, `occurrences`, `first_position`, and `sources`.

### 9.11 ontology

Loads and validates project tag ontology files from `.naming/tags/*.md`.

- `<PATH>` defaults to `.`. Tagpath searches upward for `.naming.toml`; when found, `.naming/tags` is resolved relative to that config.
- Each markdown file defines one stable domain tag. The filename stem is the default tag key (`.naming/tags/session.md` -> `session`).
- Files may start with TOML frontmatter delimited by `+++` lines. Supported fields: `tag`, `title`, `summary`, `domain`, `level`, `shape`, `role`, and `aliases`.
- When `summary` is omitted, the first non-heading body paragraph is used as the compact definition.
- Validation checks that ontology keys normalize to one canonical tag, warns on missing summaries, warns when `[tags.declared]` entries have no markdown file, and errors on undeclared ontology tags when `[tags].open = false`.
- The JSON output includes stable tag records and validation diagnostics. Downstream tools such as `tsift` can use these records to reference domain vocabulary by tag and path instead of repeating long prose definitions in every summary.

### 9.12 graph

Builds a tag co-occurrence graph from extracted identifiers.

- Extracts all identifiers from source files under `<PATH>` (defaults to `.`).
- Nodes represent individual tags (lowercase, deduplicated).
- Edges connect sequential tag pairs within identifiers (e.g., `create_user` produces edge `create -> user`).
- Edge weights count how many identifiers share that tag pair.
- Optional `--query` flag filters to a subgraph: seed nodes matching query tags plus their direct 1-hop neighbors.
- `--format dot` (default for `text`) outputs Graphviz DOT format.
- `--format json` outputs a JSON object with `nodes` (sorted array) and `edges` (array of `{from, to, weight}` objects).

### 9.13 index

Builds a persistent project snapshot at `.naming/index.json`, alongside the resolved `.naming.toml`.

- `<PATH>` defaults to `.`. Tagpath walks upward until it finds `.naming.toml`; that directory is the project root and the index is written to `<root>/.naming/index.json`.
- The on-disk schema is JSON with stable key order (`schema_version`, `generated_at`, `config_fingerprint`, `tool_version`, `sources`, `families`, `ontology_refs`). `schema_version` is the integer `1` for this release.
- `config_fingerprint` is `sha256:` of the canonical TOML serialization of the resolved config (post-`extends` merge). Any change to `.naming.toml` or any extended file flips the fingerprint.
- `sources` lists every source file scanned (the same set `tagpath extract` would walk: known language extensions, skipping `.git`, `node_modules`, `target`, `__pycache__`, `.venv`, `vendor`, and other hidden directories). Each entry records the project-relative path, a `sha256:` hash of the file bytes, the filesystem mtime in UNIX seconds, and the file size in bytes. Entries are sorted by path for determinism.
- `families` groups the extracted identifiers by canonical tag sequence using the same parser as `tagpath family`. Each family has a stable `family_id`, the lowercase canonical `tags`, and an ordered list of `members` (each with `name`, `convention`, project-relative `path`, and 1-based `line`). Members are sorted by `(path, line, name)` and deduplicated.
- `ontology_refs` mirrors the records from `tagpath ontology` — one entry per `.naming/tags/*.md` file, with the canonical `tag`, project-relative `path`, and a `sha256:` hash of the markdown bytes. Sorted by `tag`.
- `--check`: recomputes the fingerprint and per-source hashes and exits `0` if the on-disk index is still fresh, `1` otherwise. The stale report lists each reason: `index_missing`, `index_unreadable`, `schema_version`, `config_changed`, `tool_version`, `source_added`, `source_removed`, or `source_modified`. `--check` never writes.
- `--force`: rebuilds even when the on-disk index would still pass a freshness check.
- `--update`: incrementally updates the on-disk index by reusing cached source entries and re-extracting only files whose content actually changed. Falls back to a full rebuild (with a one-line stderr notice naming the reason) when the on-disk index is missing or unreadable, when the `schema_version` differs from the running binary, or when the resolved `config_fingerprint` differs from the on-disk value. On success, prints a one-line stderr digest: `[tagpath] incremental update: <changed> changed, <added> added, <removed> removed, <unchanged> unchanged (<ms>ms)`. Suppressible via `TAGPATH_QUIET=1`. The result is byte-identical to a full rebuild modulo `generated_at`. Writes are atomic via `.naming/index.json.tmp` → `rename(2)` so an interrupted update never produces a partially-written file. A binary sidecar cache (`.naming/index.bincache`, see §15.6) is written alongside and consulted on the next `--update` to short-circuit JSON parsing on the no-op fast path.
- `--update --force-full`: forces a full rebuild but keeps the digest summary format (`[tagpath] full rebuild: <sources> sources, <families> families (<ms>ms)`).
- `--update --emit jsonl`: streams NDJSON with a leading `{"type":"update_plan","changed":N,"added":N,"removed":N,"unchanged":N}` record before the standard `header` / `source` / `family` / `member` / `footer` records.
- Without flags, `tagpath index` rebuilds only when stale, otherwise prints `index is already fresh`.
- **Recommended `.gitignore` snippet:** repos that commit `.naming/index.json` should still ignore the auxiliary build artifacts:
  ```
  /target
  .naming/index.json.tmp
  .naming/index.bincache
  .naming/index.bincache.tmp
  ```
  The `.bincache` sidecar is a per-machine cache; never commit it.

### 9.14 search --index

When `--index` is passed, `tagpath search` reads `.naming/index.json` instead of rescanning the source tree.

- Tagpath first locates the project root by walking up for `.naming.toml`, then expects the index at `<root>/.naming/index.json`.
- The index is freshness-checked before use. If it is missing, unreadable, or stale (config fingerprint mismatch, schema mismatch, or any source added/removed/modified), `tagpath search --index` exits `2` with a clear error telling the user to run `tagpath index`.
- When the index is fresh, results come directly from the persisted families (no rescanning). Match semantics are identical to live search: every query tag must appear in the family's tag list.

### 9.15 rename

`tagpath rename <OLD> <NEW> [<PATH>]` performs an index-backed family rename. `<OLD>` may be any member spelling from the indexed family; `<NEW>` is parsed into tags and rendered in each member's original convention.

- Tagpath locates the project root by walking up for `.naming.toml`, loads or refreshes `.naming/index.json`, and finds the family whose canonical tags match `<OLD>`.
- Every indexed family member contributes a source-local rewrite pair. For example, renaming `create_user_profile` to `update_account_record` rewrites `create_user_profile`, `createUserProfile`, `CreateUserProfile`, `create-user-profile`, `CREATE_USER_PROFILE`, and `Create_User_Profile` to the corresponding new spelling.
- Rewrites are token-boundary aware and are applied in-place to project source files. Substrings inside longer identifiers are not rewritten.
- After a successful non-dry-run rename, Tagpath rebuilds `.naming/index.json` and its sidecar so subsequent indexed queries see the new family.
- `--dry-run` prints the same text or JSON report without writing source files or index artifacts.
- `--format json` emits `{ project_root, old, new, old_family_id, new_family_id, dry_run, files_changed, replacements, edits }`, where each edit records `{ path, old, new, replacements }`.

### 9.16 meta-index

`tagpath meta-index <WORKSPACE_ROOT>` aggregates existing per-project `.naming/index.json` files under a workspace into a top-level `.naming/meta-index.json` registry. It does not rebuild child indexes; run `tagpath index` inside each project first.

- `<WORKSPACE_ROOT>` defaults to `.`. The scan follows normal directories but skips build/cache/vendor directories such as `.git`, `.agent-doc`, `.tsift`, `target`, `node_modules`, `.venv`, and `vendor`.
- The output schema is JSON with stable key order: `schema_version`, `generated_at`, `tool_version`, `workspace_root`, `indexes`, and `families`. `schema_version` is `1`.
- `indexes` records each contributing project with workspace-relative `project_root`, workspace-relative `index_path`, child `schema_version`, child `tool_version`, `source_count`, and `family_count`.
- `families` flattens every child index family. Each entry keeps the child `project_family_handle`, `family_id`, `tags`, workspace-relative `project_root`, and a workspace-scoped `handle` of the form `meta:fam:<hash>`.
- Family members keep the child `project_member_handle`, `name`, `convention`, workspace-relative `path`, `line`, and a workspace-scoped `handle` of the form `meta:mem:<hash>`.
- `--output <PATH>` writes somewhere other than `<workspace-root>/.naming/meta-index.json`.
- `--format text` (default) prints a write summary. `--format json` prints the full generated payload after writing it.

## 10. MCP Server

`tagpath mcp` starts a stdio-based MCP (Model Context Protocol) server so coding agents can call tagpath tools over line-delimited JSON-RPC 2.0. The server is gated behind the default-on `mcp` Cargo feature.

### 10.1 Wire format

- Transport: stdio (`stdin`/`stdout`). One JSON object per `\n`-terminated line.
- Encoding: JSON-RPC 2.0.
- Requests carry `id`; notifications omit `id` and receive no response.
- Errors use the JSON-RPC error envelope. Codes used: `-32700` (parse), `-32600` (invalid request), `-32601` (method not found), `-32602` (invalid params).
- Tool errors (bad arguments, missing config) are returned inside a normal `tools/call` result with `isError: true` and a descriptive `text` body, not as JSON-RPC errors.

### 10.2 Methods

- **`initialize`** → `{ protocolVersion: "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "tagpath", version } }`.
- **`tools/list`** → `{ tools: [...] }` — each tool advertises `name`, `description`, and a JSON Schema `inputSchema`.
- **`tools/call`** with `{ name, arguments }` → `{ content: [{ type: "text", text: "<json>" }], isError }`. The text body is the JSON-serialized output from the matching library call.
- **`ping`** → `{}`.
- **`notifications/*`** are accepted silently.

### 10.3 Tools

| Tool | Inputs | Wraps |
|------|--------|-------|
| `parse` | `name`, optional `convention` | `parser::parse` + `parser::detect_convention` |
| `normalize_query` | `query` | `query::normalize_query` |
| `lint` | `path` | `lint::lint` against the nearest `.naming.toml` |
| `search` | `query`, `path`, optional `use_index` | `search::search`, or `index::search_index` when `use_index: true` (auto-rebuilds the index when stale or missing; logs a `notice:` to stderr) |
| `ontology_lookup` | `path`, optional `tag` | `ontology::load_project`, filtered to a single tag when provided |
| `indexed_project_query` | `path`, optional `tag`, `convention`, `role`, `shape` | Opens `.naming/index.json` (auto-builds if missing, auto-rebuilds if stale) and filters families/members by the supplied facets |
| `family_by_path` | `path`, optional `project_root`, `auto_build` (default `true`) | Opens `.naming/index.json` and returns every family with a member whose `path` matches the input (canonicalized relative to the project root). Output: `{ families: [{ family_handle, family_id, tags, ontology_refs, members: [{ name, convention, line, member_handle }] }] }`. When the file is not tracked: `{ families: [], diagnostic: "path_not_in_index" }`. Auto-builds the index when missing/stale and `auto_build: true`. |
| `lint_session_doc` | `path`, optional `fs_checks` (default `false`), `rules` | Wraps `lint::agent_doc::lint_agent_doc`. Output: `{ findings: [LintFinding...], exit_code: 0\|1 }` — `exit_code` mirrors the CLI (0 clean, 1 errors present). `rules` restricts findings to the listed rule IDs. |
| `index_handle` | `handle` (`fam:...` or `mem:...`), optional `project_root`, `auto_build` (default `true`) | Resolves a stable handle against the current index. For `fam:` returns `{ found: true, kind: "family", family }`; for `mem:` returns `{ found: true, kind: "member", member, family }`. When the handle no longer matches (rename, retag, deletion): `{ found: false, diagnostic: "handle_stale" }`. Invalid handle format (no `fam:`/`mem:` prefix) yields `isError: true`. |

### 10.4 Example transcript

```
$ printf '%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize"}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"parse","arguments":{"name":"createUser"}}}' \
  | tagpath mcp
{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"tools":{}},"protocolVersion":"2024-11-05","serverInfo":{"name":"tagpath","version":"0.8.0"}}}
{"jsonrpc":"2.0","id":2,"result":{"content":[{"text":"{\n  \"original\": \"createUser\",\n  \"convention\": \"camel_case\",\n  \"tags\": [\"create\",\"user\"], ...","type":"text"}],"isError":false}}
```

### 10.5 Feature flag

Disable with `--no-default-features --features lang-rust,...` to build without `mcp`. A binary built without `mcp` still accepts the `mcp` subcommand but exits with a "built without the `mcp` feature" error so callers fail closed.

### 10.6 Installer

`tagpath mcp install` generates ready-to-paste MCP server config blocks for the five major harnesses Claude Desktop, Claude Code, Codex, OpenCode, and Cursor. The installer never starts the server — it only emits or writes config text.

The bare `tagpath mcp` invocation (no subcommand) still starts the stdio server; `tagpath mcp serve` is an explicit alias. `tagpath mcp install ...` is the installer entrypoint.

#### 10.6.1 Subcommands

| Flag | Behavior |
|------|---------|
| `--list` | Print known harness names and their default config-file paths on the current platform. |
| `--print <harness>` | Emit the harness config block to stdout. No file writes. Safe default — pipe to `pbcopy` / `xclip` / `>` a file. |
| `--apply <harness>` | Merge the `tagpath` entry into the harness's config file. Without `--yes`, prints the resolved path + preview as a dry-run; with `--yes`, performs an atomic write. |
| `--uninstall <harness>` | Remove the `tagpath` entry from the harness config. Idempotent; honors `--yes` like `--apply`. |
| `--binary <path>` | Override the `command` field (default `"tagpath"` — resolved via PATH). |
| `--project <path>` | Write to `<path>/.claude/settings.json` or `<path>/.cursor/mcp.json` instead of the user-level config. Only Claude Code and Cursor honor this; other harnesses return an error when `--project` is paired with them. |
| `--config-dir-override <dir>` | Replace the resolved home/config base directory. Primarily for tests; advanced users with non-standard config layouts can use it too. |

#### 10.6.2 Harness matrix

| Harness | Format | Default user path | Config key |
|---------|--------|-------------------|------------|
| `claude-desktop` | JSON | macOS `~/Library/Application Support/Claude/claude_desktop_config.json`, Windows `%APPDATA%\Claude\claude_desktop_config.json`, Linux `~/.config/Claude/claude_desktop_config.json` | `mcpServers.tagpath` |
| `claude-code` | JSON | `~/.claude/settings.json` (or `<project>/.claude/settings.json` with `--project`) | `mcpServers.tagpath` |
| `codex` | TOML | `~/.codex/config.toml` | `[mcp_servers.tagpath]` |
| `opencode` | JSON | `~/.config/opencode/config.json` (XDG_CONFIG_HOME-aware) | `mcp.tagpath` (with `"type": "local"`) |
| `cursor` | JSON | `~/.cursor/mcp.json` (or `<project>/.cursor/mcp.json` with `--project`) | `mcpServers.tagpath` |

Path resolution uses the `dirs` crate, which honors `XDG_CONFIG_HOME` on Linux, `%APPDATA%` on Windows, and `~/Library/Application Support` on macOS.

#### 10.6.3 Merge semantics

The installer ONLY touches the `tagpath` entry under each harness's canonical map (`mcpServers.tagpath`, `mcp.tagpath`, or `mcp_servers.tagpath`). All other keys — unrelated MCP servers, top-level fields like `telemetry`, `model`, `version` — are preserved verbatim. Re-running `--apply --yes` is idempotent: the output file is byte-equivalent on the second call.

Writes are atomic via `<path>.tmp` then `rename(2)`, mirroring the index writer in `src/index/mod.rs`.

#### 10.6.4 Security note

`--apply --yes` modifies a file in the user's home directory. The default `--print` workflow is the audit-friendly path: the user sees the exact JSON/TOML block before pasting it. `--apply` without `--yes` is a dry-run that prints the resolved path + the merged preview without writing. Tests and CI should always pair `--apply` with `--config-dir-override` to avoid touching real config.

## 11. Tsift Token-Savings Benchmark Fixture

The repo includes `fixtures/tsift-token-savings.json`, a downstream-facing fixture for measuring how compact tagpath family/alias previews are compared with raw symbol previews in tsift workflows.

- The fixture covers six tsift preview surfaces: `search`, `explain`, `session-review`, `context-pack`, `normalize-query`, and `ontology-refs`.
- Each case contains `raw_symbols`, representing ungrouped symbol preview rows with identifier, file, line, and context.
- Each case contains `tagpath_families`, representing compact grouped output by canonical family. Every family records `canonical`, `count`, and the six generated aliases (`snake_case`, `camelCase`, `PascalCase`, `kebab-case`, `UPPER_SNAKE_CASE`, and `Ada_Case`).
- The `context-pack` case also contains `context_pack_inputs`: representative raw next-context, diff, test, and log refs with `summary_refs`, `ontology_refs`, artifact handles, and expansion commands. This lets downstream tests compare raw handoff refs against compact family rows and section handles without inlining repeated ontology prose.
- The `normalize-query` case contains `normalize_query_input` and `expected_query_tags`, so downstream tests can verify that query text normalizes to compact tag handles before lexical fallback.
- The `ontology-refs` case contains `ontology_refs` with stable handles and `.naming/tags/*.md` paths. Raw fixture previews may include long summaries, but compact previews must keep only handle/tag/path rows so context packs can reference ontology entries instead of expanding prose.
- The deterministic token estimate is `ceil(utf8_bytes / 4)`. This is intentionally tokenizer-independent so tsift and tagpath tests can compare the same fixture without a model-specific tokenizer dependency.
- `minimum_savings_percent` records the required reduction for the compact family/alias preview compared with the raw symbol preview.
- Tests must verify that raw symbols parse back to the fixture family counts through the shared compression-report helper, aliases are generated by tagpath rather than hand-maintained drift, `context-pack` carries ontology/summary/artifact handles, `normalize-query` emits the expected compact tags, `ontology-refs` keeps `.naming/tags` references as handles, and compact previews satisfy each case's savings threshold.

## 12. Extends Resolution

The `extends` field in `.naming.toml` enables composable configuration.

### 11.1 Syntax

```toml
extends = ["rust"]          # extend a single language preset
extends = ["rust", "custom"] # extend multiple presets (applied left to right)
```

### 11.2 Resolution Rules

1. Presets are resolved by name from the `lang/` and `presets/` directories.
2. When extending multiple presets, they are applied left to right — later presets override earlier ones.
3. The extending config's fields override all inherited fields at the same level.
4. Context-level merging: `[contexts.<name>]` sections merge with inherited contexts. Only the fields specified in the extending config replace the parent values; unspecified fields are retained from the parent.
5. Top-level fields (`convention`, `immutable`, `singular`, etc.) are fully replaced if present in the extending config.
6. `[tags.declared]` entries merge additively — the extending config can add new tag declarations without removing inherited ones.

## 13. WASM

Tagpath compiles to `wasm32-unknown-unknown` and exposes a small JS-friendly
API via `wasm-bindgen`. The wasm build is filesystem-free: tree-sitter, the
`extract` walker, `search`, `lint`, `index`, `graph`, `ontology`, and `mcp`
are all gated out of the wasm target so the resulting `.wasm` artifact stays
small and has no host syscalls.

### 13.1 Feature flag

Enable with `--no-default-features --features wasm`. The `wasm` feature pulls
in `wasm-bindgen`, `serde-wasm-bindgen`, and `js-sys` as optional dependencies
and is mutually exclusive with the `lang-*` tree-sitter features in practice
(the wasm build deliberately drops them).

### 13.2 Exposed surface

All functions are exported as `#[wasm_bindgen]` from `tagpath::wasm`:

| Function | Purpose |
|----------|---------|
| `parse(name, convention?)` | Returns the `ParsedName` shape as a JS object. If `convention` is omitted or unknown, the convention is auto-detected. |
| `alias(name, target_convention?)` | Returns an `AliasResult { tags, aliases }` object. With no target, every supported convention is emitted. |
| `prose(name)` | Returns the human-readable prose string. |
| `normalize_query(query)` | Returns a `NormalizedQuery { original, tags[] }` object with ranked canonical tags. |
| `search_over_rows(query, rows_json)` | Filters a caller-supplied JSON array of `{ name, path?, line? }` rows. Returns the matching rows enriched with `convention`, `tags`, `role`, and `shape`. |

Convention strings use the canonical Display form: `snake_case`, `camelCase`,
`PascalCase`, `kebab-case`, `UPPER_SNAKE_CASE`, `Ada_Case`. Alias map keys use
the same Display form, not the serde `snake_case` rename.

### 13.3 No-filesystem rule for `search_over_rows`

The wasm build intentionally does **not** scan the filesystem. The host
environment (Node, a browser, ts-morph, or an agent harness) is responsible
for collecting candidate identifiers and passing them in as a JSON array.
This keeps the wasm artifact decoupled from tree-sitter grammars and avoids
needing a virtual filesystem shim.

### 13.4 Build command

```sh
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm

# With wasm-pack (Node target):
wasm-pack build --target nodejs --no-default-features --features wasm
```

The `[lib] crate-type = ["cdylib", "rlib"]` setting is required so `wasm-pack`
can emit a cdylib alongside the regular rlib used by native consumers.

### 13.5 Publish flow (`@btakita/tagpath-wasm`)

The wasm-pack output ships to npm as a single package, `@btakita/tagpath-wasm`,
with three target-specific entry points so the same install works across Node,
bundlers (webpack/vite), and direct browser usage.

**Build script:** `scripts/build-wasm.sh` runs `wasm-pack build` three times
against the `wasm` feature set, then merges the outputs into a single `pkg/`:

```
pkg/
  package.json         single, with "exports" map routing per consumer
  README.md            copied from repo root
  LICENSE-MIT          copied from repo root if present
  tagpath.d.ts         merged type defs (named after [lib].name = "tagpath")
  bundler/             output from `wasm-pack build --target bundler`
  nodejs/              output from `wasm-pack build --target nodejs`
  web/                 output from `wasm-pack build --target web`
```

The `package.json` `name` and `version` are derived from `Cargo.toml` on every
run so the npm version always tracks the crate version. The `exports` map:

| Consumer        | Subpath              | File                          |
|-----------------|----------------------|-------------------------------|
| Node ESM        | `@btakita/tagpath-wasm` | `./nodejs/tagpath.js`  |
| Browser direct  | `@btakita/tagpath-wasm` | `./web/tagpath.js`     |
| Bundler default | `@btakita/tagpath-wasm` | `./bundler/tagpath.js` |
| Explicit        | `@btakita/tagpath-wasm/nodejs` / `/web` / `/bundler` | matching shim |

**Smoke test:** `pkg-smoke/smoke.mjs` imports the Node entry via a relative
path (`../pkg/nodejs/tagpath.js`) and exercises every wasm binding
(`parse`, `alias`, `prose`, `normalize_query`, `search_over_rows`). The CI
workflow `.github/workflows/wasm-build.yml` runs the build script and the
smoke test on every push and PR to `main`, then uploads `pkg/` as a workflow
artifact.

**Manual publish gate:** npm publish is **not** automated. After the CI build
goes green, `npm publish` is run manually after `npm login`. The build path
itself is proven by CI; the publish step is a human decision.

## 14. Dynamic grammar loading

Tagpath can load tree-sitter grammars from compiled shared libraries
(`.so` / `.dylib` / `.dll`) at runtime, modelled after the Helix and Neovim
approaches. This lives behind an opt-in feature flag and is **native-only** —
the loader and its config surface are unavailable on WASM.

### 14.1 Feature flag

```sh
cargo install tagpath --features dyn-grammar
# or, for a dev build:
cargo build --features dyn-grammar
```

`dyn-grammar` is **not** part of the default feature set. It coexists with the
compile-time `lang-*` features: each is independently enable-able, and the
dynamic loader wins on collisions (see Precedence).

### 14.2 Config surface

```toml
# .naming.toml
[grammars]
# Directories scanned for compiled tree-sitter grammars.
# Relative paths resolve against the .naming.toml directory;
# a leading `~/` expands to the user's home directory.
load_dirs = ["./grammars", "~/.config/tagpath/grammars"]

# Pin specific grammars by language key.
[grammars.languages.zig]
path = "./grammars/tree-sitter-zig.so"
# `symbol` defaults to `tree_sitter_{lang}` — set it only if your grammar
# uses a non-standard entrypoint name.
symbol = "tree_sitter_zig"
extensions = ["zig"]
```

When tagpath is built **without** `dyn-grammar`, the `[grammars]` section
deserializes successfully but is ignored at runtime — configs stay portable
across builds.

### 14.3 Precedence

When a file's extension is handled by both a compile-time `lang-*` grammar
and a configured dynamic grammar, the **dynamic grammar wins**. This lets
users override a bundled grammar with a freshly compiled local copy without
rebuilding tagpath.

When multiple dynamic grammars claim the same extension, the first match in
`[grammars.languages]` ordering wins (BTreeMap iteration → lexicographic by
language key).

### 14.4 ABI compatibility

Every loaded grammar must report an ABI version inside the
`MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION` range exposed by the
linked `tree-sitter` runtime. ABI mismatches surface as
`LoadError::AbiMismatch { path, actual, expected_min, expected_max }`. Fix
by rebuilding the grammar against a tree-sitter CLI within the supported
range, then re-running `tagpath grammars check`.

### 14.5 Error envelopes

The loader returns one of the following variants on failure; CLI commands
render them with the offending path and a remediation hint:

| Variant | Trigger | Hint surfaced |
|---|---|---|
| `MissingFile` | `path` does not exist | Check the `path` value under `[grammars.languages.*]` |
| `DlOpen` | `dlopen` / equivalent fails | Verify the file is a real shared library for this platform |
| `MissingSymbol` | `dlsym` of the entry symbol fails | Set `symbol = "..."` if the grammar uses a non-default entrypoint |
| `AbiMismatch` | grammar ABI outside supported range | Rebuild the grammar against a compatible tree-sitter CLI |

### 14.6 CLI

```sh
tagpath grammars list      # show configured + discovered grammars (with status)
tagpath grammars list --format json
tagpath grammars check     # exit 0 if every configured grammar loads; exit 1 otherwise
```

When tagpath is built **without** `dyn-grammar`, both subcommands remain
visible in `--help` and exit with a clear "built without --features
dyn-grammar" hint so users can detect a misconfigured install quickly.

The first time a dynamic grammar is used in a process, tagpath emits a
single stderr notice:

```
[tagpath] using dynamic grammar for zig from ./grammars/tree-sitter-zig.so
```

Set `TAGPATH_QUIET=1` to suppress this notice.

### 14.7 Security note

`dlopen`-loading an arbitrary shared library is **executing arbitrary native
code in the tagpath process**. Only point `load_dirs` and
`[grammars.languages.*].path` at directories you control or trust. The
loader does no signature checking and cannot — by design — distinguish a
real tree-sitter grammar from a malicious cdylib with a matching symbol
name. This is the same trust model as Helix's `runtime/grammars/` and
Neovim's `nvim-treesitter` install directory.

## 15. Consumer contract (tsift / agent-doc / external)

`tagpath index` is meant to be consumed as a symbol-graph adapter by
external tools — primarily `tsift` (hybrid BM25 + vector search) but the
same contract serves any downstream that wants stable family handles.
This section is the wire and semantic contract those consumers can
target.

### 15.1 Schema version

The index payload carries an integer `schema_version`. Current value: `2`.
Bump rules:

- Backward-compatible field additions (new optional fields on `Family`,
  `FamilyMember`, etc.) bump `schema_version`. Older permissive readers
  keep working; strict readers must rebuild.
- Field renames, semantic changes to existing fields, or removals are
  major changes and also bump `schema_version`.

Consumers can feature-detect cheaply via:

```
tagpath index --schema-version
```

This command prints the integer (no trailing prose) and exits 0.
Consumers should treat `schema_version` < their minimum supported value
as "tagpath too old" and `schema_version` > their maximum as "tagpath
too new" and ask the user to align versions.

`tagpath index --check` against an older on-disk schema returns the
`schema_changed` stale reason (not the harder `schema_version`
mismatch). Tagpath treats this as a silent migration trigger and
rebuilds without surfacing an error. A `schema_version` *higher* than
the running tagpath stays as the hard `schema_version` mismatch — we
cannot upgrade upward.

### 15.2 Stable handles

Every `Family` carries a content-addressable `handle` of the form
`fam:<sha256[0..16]>`. Derivation:

```
sha256(
  project_root_canonical_path  + "\n" +
  sorted(tags).join(",")        + "\n" +
  sorted(ontology_tags_used_by_family).join(",")
).hex()[0..16]
```

`project_root_canonical_path` is the canonicalized absolute path of the
project root (the directory containing `.naming.toml`), with backslashes
replaced by forward slashes so handles stay portable across operating
systems. If canonicalization fails (e.g. the directory was just removed),
tagpath falls back to the lexical path it was given.

`sorted(ontology_tags_used_by_family)` is the list of ontology tags
(from `.naming/tags/*.md`) that match any tag in this family, sorted
ascending. If a project has no ontology, this becomes the empty string.

Critically, this derivation does **not** include source paths, member
counts, or member lines. Adding a new member to an existing family does
not change the family handle, so tsift's citations survive ordinary
edits.

Every `FamilyMember` carries a content-addressable `handle` of the form
`mem:<sha256[0..16]>`. Derivation:

```
sha256(
  family_handle + "\n" +
  member_name   + "\n" +
  path_relative_to_project_root
).hex()[0..16]
```

Line numbers are intentionally excluded — moving a definition inside a
file does not break the member handle. Renames and file moves *do*
break the handle on purpose: the symbol's identity changed.

Both fields also carry `family_handle` on each `FamilyMember` so
consumers can join member rows back to families without a separate
lookup.

### 15.3 Freshness contract

`tagpath index --check` reports freshness against the on-disk
`.naming/index.json`. The exit code is `0` for fresh, non-zero for
stale. The structured stale reasons are:

| Reason | Meaning |
|---|---|
| `index_missing` | No on-disk index yet — first build. |
| `index_unreadable` | Index file present but cannot be parsed. |
| `schema_version` | On-disk schema does not match and we cannot migrate (e.g. on-disk is newer than us). |
| `schema_changed` | On-disk schema is older than the current `SCHEMA_VERSION`. Silent rebuild; not an error. |
| `tool_version` | Compiled tagpath version changed since the index was written. |
| `config_changed` | `.naming.toml` or its `extends` chain fingerprint changed. |
| `source_added` / `source_removed` / `source_modified` | Per-file delta against the on-disk source set. |

Recommended consumer pattern:

1. On boot, run `tagpath index --schema-version` once and refuse to
   start if the version is outside the supported range.
2. Before each query, run `tagpath index --check`. If non-zero, run
   `tagpath index --update` first — it reuses cached entries and
   re-extracts only files whose content actually changed, and the
   result is byte-identical to a full rebuild. `--update` falls back
   to a full rebuild on its own if the schema or config fingerprint
   no longer matches, so consumers do not need to chain the fallback
   themselves. Plain `tagpath index` (or `tagpath index --emit jsonl`)
   remains available for an unconditional rebuild.
3. Read `.naming/index.json` (or consume NDJSON) and use the family
   handles as citation keys in your own envelopes.

### 15.4 NDJSON wire format (`--emit jsonl`)

`tagpath index --emit jsonl` streams NDJSON to stdout instead of writing
`.naming/index.json`. Same data, different framing — useful for pipeline
consumers that don't want to round-trip through disk.

Record order is stable:

1. Exactly one `header` line, first.
2. Zero or more `source` lines.
3. Zero or more `family` lines.
4. Zero or more `member` lines.
5. Exactly one `footer` line, last.

Each line is a single JSON object terminated by `\n`. No blank lines.
Record shapes:

```json
{"type":"header","schema_version":2,"tool_version":"0.11.0","config_fingerprint":"sha256:...","generated_at":"2026-05-24T22:00:00Z"}
{"type":"source","path":"src/foo.rs","hash":"sha256:...","mtime":1700000000,"size":42}
{"type":"family","handle":"fam:abcdef0123456789","family_id":"create_user","tags":["create","user"],"ontology_refs":["user"]}
{"type":"member","handle":"mem:0123456789abcdef","family_handle":"fam:abcdef0123456789","name":"create_user","convention":"snake_case","path":"src/foo.rs","line":1}
{"type":"footer","counts":{"sources":1,"families":1,"members":1,"ontology_refs":0}}
```

`footer.counts` must equal the count of records of each type actually
emitted, so consumers can detect truncation.

`--emit jsonl` can combine with `--check`. In check mode the stream is:

1. A `header` line carrying `schema_version`, `tool_version`,
   `index_path`, `project_root`, and a `fresh` flag.
2. Zero or more `stale` lines, each `{"type":"stale","reason":{...}}`
   carrying the structured stale-reason variant.
3. A `footer` with `counts.stale_reasons`.

In check mode, no `.naming/index.json` is written and tagpath exits
non-zero when `fresh: false` — matching the text-mode behavior.

`--emit jsonl --force` rebuilds and streams. The default `--emit json`
preserves the prior on-disk behavior verbatim.

### 15.5 Recommended consumer pattern (tsift et al.)

- Poll `tagpath index --check` before each batch of work. On stale,
  rebuild with `tagpath index --emit jsonl` and stream into your
  consumer pipeline in one pass.
- Cite families by `handle` (not `family_id`), and members by `handle`
  (not `path:line`). These survive ordinary edits.
- Treat handle changes as deliberate signals: a renamed handle means
  the underlying symbol identity changed.
- When the on-disk schema is older than what your consumer compiled
  against, treat `schema_changed` as a silent rebuild trigger; only
  surface `schema_version` as a hard error.

**Reference consumer:** tsift adopted this contract in 0.1.47. See
[`src/tsift/SPEC.md` § Tagpath integration](../tsift/SPEC.md#tagpath-integration)
for the implementation: auto-detect, `--no-tagpath` opt-out, `--tagpath-strict`
fail-closed mode, and the `tagpath_handle` field on `SymbolHit`.

### 15.6 Sidecar cache (`.naming/index.bincache`)

Tagpath writes an auxiliary binary file `.naming/index.bincache`
alongside the canonical `.naming/index.json`. It is a bincode-encoded
copy of the same `Index` payload, split into a small "head" section
(schema + config + sources) and a larger "tail" section (families +
ontology refs) so the no-op fast path can decode only the head.

**Consumers must not depend on this file.** It is a tagpath-internal
build artifact, not part of the wire contract:

- Format details (magic bytes, framing, bincode version) may change
  without notice across tagpath patch releases. Read `.naming/index.json`
  if you need a stable shape.
- Tagpath verifies the sidecar's integrity (sha256 of each section
  against the framed header, schema-version baked into the wrapper) on
  every read. On any mismatch — missing, corrupt, schema skew, wrapper
  version skew — tagpath silently falls back to JSON read and
  regenerates the sidecar.
- The sidecar is renamed *after* the JSON rename. A mid-write crash
  between the two renames leaves the JSON authoritative and the
  sidecar absent or stale; the next `--update` cycle detects this via
  the integrity check and recovers without surfacing an error.
- Cloning a repo without the sidecar is fine; the first `--update`
  rebuilds it from the JSON.

Performance contract: on a 1000-source synthetic repo, the sidecar
fast path takes `--update` no-op cycles to ~1-3 ms (vs ~100-180 ms
for a full rebuild) — roughly 50× faster, comfortably past the ≥10×
bar the sidecar was introduced to hit.

## 16. Agent-doc dialect

The `tagpath lint` command supports a second dialect, `agent-doc`, that
validates HTML-comment directives used by session documents
(`agent-doc`, Claude Code / Codex / OpenCode / direct harnesses). The
identifier-naming lint is unchanged; this is a sibling dialect.

### 16.1 CLI

```
tagpath lint <file_or_dir>                          # auto-detect
tagpath lint <file> --dialect agent-doc
tagpath lint <file> --dialect agent-doc --fs-checks
tagpath lint <file> --dialect agent-doc --format json
tagpath lint <file> --dialect agent-doc --rule agent-doc/malformed-attr
```

- `--dialect` accepts `identifier`, `agent-doc`, or `auto` (default).
  In auto mode, a markdown file containing `<!-- agent:exchange` is
  routed through the agent-doc dialect; otherwise the identifier lint
  runs.
- `--fs-checks` enables rules that require disk reads (archive target
  existence, done-id cross-reference).
- `--rule <id>` is repeatable and restricts findings to specific rule
  IDs.
- Exit codes: `0` clean, `1` findings present (any error severity),
  `2` internal error.

### 16.2 Tag families recognized

Component tags (open + close):
`agent:exchange`, `agent:status`, `agent:backlog`, `agent:done`,
`agent:icebox`, `agent:queue`, `agent:review`.

Single-instance directives: `agent:boundary:<hex>`,
`no-pending-done-guard`.

Patch markers (paired): `patch:exchange`, `patch:status`,
`patch:backlog`, `patch:review`.

### 16.3 Rules

| Rule ID                                | Severity | Description                                                      |
| -------------------------------------- | -------- | ---------------------------------------------------------------- |
| `agent-doc/unknown-component`          | error    | `<!-- agent:foo -->` where `foo` is not a known component        |
| `agent-doc/unclosed-component`         | error    | open component without a matching close                          |
| `agent-doc/orphan-close`               | error    | `<!-- /agent:foo -->` without a preceding open                   |
| `agent-doc/duplicate-component`        | error    | same component reopened before its prior close                   |
| `agent-doc/malformed-attr`             | error    | attribute token missing `=` (e.g. `archive PATH`)                |
| `agent-doc/empty-attr-value`           | error    | `key=` with no value (or `key=""`)                               |
| `agent-doc/unknown-attr`               | warning  | attribute not allowed on this component                          |
| `agent-doc/invalid-attr-value`         | warning  | recognized attribute with an unrecognized value (e.g. backlog `queue=nope`) |
| `agent-doc/queue-mode-token`           | error    | `agent:queue mode=auto` instead of bare `agent:queue auto`       |
| `agent-doc/malformed-boundary`         | error    | `agent:boundary:` not in `<hex>` or `<hex>:<slug>` form          |
| `agent-doc/unknown-patch-marker`       | warning  | `patch:<name>` not in `exchange/status/backlog/review`           |
| `agent-doc/patch-marker-outside-cycle` | warning  | `patch:exchange` outside an `agent:exchange` block               |
| `agent-doc/backlog-id-collision`       | error    | duplicate `[#id]` inside one backlog block                       |
| `agent-doc/done-archive-missing-target`| error    | `archive=<path>` is missing on disk or not `.done.md` (fs-checks)|
| `agent-doc/done-id-not-in-backlog`     | warning  | done item id never appeared in backlog (fs-checks)               |

Rules `done-archive-missing-target` and `done-id-not-in-backlog`
require `--fs-checks`. The done-id check is suppressed for blocks
whose first line is `<!-- migrated -->` (or `# migrated`).

### 16.4 Output

Text format mirrors the identifier-lint style:

```
tasks/foo.md:42:1 error: attribute `archive` on `agent:done` is missing `=value` [agent-doc/malformed-attr]
  hint: try `archive=<value>`
```

JSON format (`--format json`) emits an array of:

```jsonc
{
  "path": "tasks/foo.md",
  "line": 42,
  "col": 1,
  "rule": "agent-doc/malformed-attr",
  "severity": "error",
  "message": "...",
  "fix_hint": "try `archive=<value>`"
}
```

### 16.5 Motivating bug

`<!-- agent:done archive PATH -->` (missing `=`) was silently parsed as
`archive=""` by older session-document tooling and only failed deep
inside the agent-doc `finalize` step. The
`agent-doc/malformed-attr` rule fires on this form so the failure
surfaces at lint time, before the directive can reach `finalize`.

## 17. Watch mode

`tagpath watch [<path>]` is a long-running process that observes
filesystem changes in a project rooted at `<path>` (defaults to the
current directory) and emits newline-delimited JSON events on stdout.
Consumers — tsift, agent-doc editor surfaces, ad-hoc `tail -f | jq`
pipelines — read the stream to react in real time without polling
`tagpath index --update`.

The feature is gated behind the default-on `watch` Cargo feature and
is native-only (`#[cfg(all(feature = "watch", not(target_arch =
"wasm32")))]`). WASM builds and `--no-default-features` builds stay
clean.

### 17.1 Wire format (stdout)

Stdout is machine-only: every line is exactly one JSON object
terminated by `\n`. Stderr carries human-readable status
(`[tagpath watch] reindexed N files in Mms`); `TAGPATH_QUIET=1`
suppresses stderr.

The first line is always `hello`:

```json
{"type":"hello","schema_version":1,"tool_version":"0.15.0","project_root":"/abs/path","index_schema_version":2,"watcher":"notify-6"}
```

- `schema_version` versions the watch wire format itself
  (`watch::WATCH_SCHEMA_VERSION`).
- `index_schema_version` mirrors the on-disk index schema
  (`index::SCHEMA_VERSION`).
- `watcher` documents the backend (`notify-6` for the v6 notify crate).

Subsequent event types:

| `type`          | Fields                                                                                                                                                                                                         |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ready`         | none — initial reindex done, watcher armed.                                                                                                                                                                     |
| `index_update`  | `summary.{changed,added,removed,unchanged,elapsed_ms}` and `changed_handles` (sorted unique union of added + removed `fam:…`/`mem:…` handles, computed by diffing the new index against the previous snapshot). |
| `lint_finding`  | `dialect`, `path`, `line`, `col`, `rule`, `severity`, `message`, `fix_hint`. Emitted per finding when a changed `.md` file contains the agent-doc trigger string.                                                |
| `error`         | `stage` (`index_update`, `lint`, `watcher`), `reason`. Non-fatal; watcher stays alive.                                                                                                                          |
| `shutdown`      | `reason` (`sigint`, `sigterm`, …). Final line before exit.                                                                                                                                                      |

`--emit-shape compact` drops `changed_handles` and most `lint_finding`
detail (keeps `type`, `rule`, `path`) so consumers that only need a
heartbeat can save bytes. Default is `full`.

### 17.2 Debounce + ignore rules

Filesystem events are coalesced inside a sliding **150 ms** quiet
window before a reindex pass fires (`watch::DEFAULT_DEBOUNCE_MS`).
The window catches the typical editor "atomic save" burst (rename +
chmod + content) and the noisy `inotify` fanout from build tools.

The watcher ignores any path whose components include `.naming`,
`.git`, `target`, `node_modules`, `__pycache__`, `.venv`, or `vendor`
— the same set used by `extract::list_source_files`. The `.naming`
exclusion is load-bearing: it prevents the watcher firing on its own
`.naming/index.json` writes.

Lint runs only on `.md` files in the changed batch that pass the
`looks_like_agent_doc` content check.

### 17.3 Single-instance lock

On start, the watcher writes its PID to `.naming/watch.pid`. If the
file already exists and names a live process, the second `tagpath
watch` invocation exits 1 with a clear stderr message. On clean
shutdown (any path that returns from `watch::run`) the lockfile is
removed; the RAII `PidLock` guard guarantees this even on panic.

On non-unix targets the liveness check is conservative — a stale
lockfile from a crashed previous run must be removed manually.

### 17.4 Signal handling

`SIGINT` and `SIGTERM` are caught via `sigaction`. An async-signal-safe
handler sets a process-wide atomic; a tiny watcher thread translates
that into the shared shutdown flag and emits the `shutdown` event from
the main loop before returning. The watcher targets exit-within-250 ms
after receiving a signal.

### 17.5 CLI surface

- `tagpath watch [<path>]` — start the long-running watcher.
- `tagpath watch --once` — perform one reindex + lint pass, emit the
  events, exit. Useful for editor save hooks.
- `tagpath watch --no-lint` — skip the agent-doc lint pass.
- `tagpath watch --emit-shape full|compact` — output verbosity knob.

### 17.6 Recommended consumer pattern

Consumers should use line-buffered stdout reads and a structured
event dispatch keyed on `type`:

```bash
tagpath watch &
tail -f /dev/null | tagpath watch | while IFS= read -r line; do
  case "$(echo "$line" | jq -r .type)" in
    index_update) echo "reindex: $line" ;;
    lint_finding) echo "lint: $line" ;;
  esac
done
```

For long-lived agent integrations, treat `hello` as the schema-
negotiation handshake, persist the `project_root` + `tool_version` for
log correlation, and reset any local handle cache when
`schema_version` differs from the last-seen value.

### 17.7 Performance baseline matrix

Before introducing a project-session cache or a lazily-evaluated
invalidation layer, measure the current public behavior with:

```bash
scripts/benchmark-current-performance.sh
```

The script builds the current `target/debug/tagpath` binary unless
`TAGPATH_BIN` points at an existing executable, creates a synthetic
1000-file Rust project, primes `.naming/index.json` and
`.naming/index.bincache`, and prints CSV timing rows. `--plan` prints
the command matrix without building or running the benchmark; tests use
that mode to keep this section synchronized.

Target budgets are intentionally conservative for the debug binary on a
developer workstation. Lazier project/session work may change the
implementation, but it should not regress these median timings without a
documented tradeoff:

| Case | Target median | Command path |
|---|---:|---|
| `noop_sidecar_update` | <= 10 ms | `TAGPATH_QUIET=1 tagpath index --update "$fixture"` after the sidecar exists. |
| `one_changed_file_update` | <= 125 ms | Append one Rust function, then run `TAGPATH_QUIET=1 tagpath index --update "$fixture"`. |
| `full_reindex` | <= 450 ms | `TAGPATH_QUIET=1 tagpath index --update --force-full "$fixture"`. |
| `watch_save_burst` | <= 500 ms | Start `tagpath watch "$fixture" --no-lint --emit-shape compact`, write a five-save burst, and wait for the next `index_update`. |
| `mcp_indexed_project_query` | <= 150 ms | Send a `tools/call` request for `indexed_project_query` through `tagpath mcp`. |
| `mcp_family_by_path_read` | <= 150 ms | Send a `tools/call` request for `family_by_path` through `tagpath mcp`. |

The watch budget measures wall time from the first write in the save
burst until the next `index_update`, so it includes the configured
150 ms debounce window. The other rows measure process wall time for
the stated command path, including CLI or MCP process startup.

## 18. Workspace split

The first crate split is a conservative `tagpath-core` extraction. The
goal is to give agents, wasm hosts, and other libraries a small stable
identifier semantics crate without changing the existing `tagpath`
binary, CLI defaults, or public import paths.

This is a one-core-crate plan, not a broad workspace breakup. Tree-sitter
languages, MCP, watch mode, indexing, config resolution, filesystem
search, and rename support stay in the existing `tagpath` facade until
there is concrete consumer pressure to split them further.

### 18.1 Workspace shape

The repository root is a workspace:

```toml
[workspace]
members = [".", "crates/tagpath-core"]
resolver = "3"
```

`crates/tagpath-core` owns the pure semantic library. The root `tagpath`
package remains the published binary/library facade and depends on the
core crate with a path dependency during development and a crates.io
version during publish:

```toml
tagpath-core = { version = "=<same-version>", path = "crates/tagpath-core" }
```

Use lockstep versions for the first split. Publishing `tagpath-core`
under the same version as `tagpath` avoids a second compatibility matrix
while the public core API is still being validated.

### 18.2 Public API inventory

These public modules and functions live in `tagpath-core`:

| Current path | Core API to preserve | Reason |
|---|---|---|
| `tagpath::parser` | `Convention`, `ParsedName`, `ALL_CONVENTIONS`, `detect_convention`, `parse`, `join_tags`, `capitalize` | Foundational identifier semantics; no filesystem, CLI, or tree-sitter dependency. |
| `tagpath::alias` | `AliasResult`, `generate_aliases` | Pure wrapper around parser + convention rendering. |
| `tagpath::family` | `TagFamily`, `TagDimension`, `SurfaceExample`, `FamilyOccurrence`, `TagFamilySummary`, `FamilySummaryExample`, `generate_family`, `generate_family_with_convention`, `summarize_occurrences` | Canonical family model used by agents and compact previews. |
| `tagpath::prose` | `ProseResult`, `to_prose` | Pure natural-language projection from parsed tags. |
| `tagpath::query` | `NormalizedQuery`, `QueryTag`, `normalize_query`, `normalize_query_tags` | Pure prompt/query normalization for agent-facing callers. |
| `tagpath::compression` | `RawSymbolRow`, `CompressionFamilyPreview`, `CompressionFamilyExample`, `CompressionMetrics`, `CompressionReport`, `build_report`, `build_report_with_example_limit`, `render_raw_symbol_preview`, `render_compact_family_preview`, `estimate_tokens` | Pure row-to-family compression over caller-supplied symbols. |

Keep these modules in the root `tagpath` crate for now:

| Current path | Keep in facade because |
|---|---|
| `tagpath::config` | It owns `.naming.toml` loading, extends resolution, bundled preset generation, home-dir expansion, and TOML parsing. A later `tagpath-config` split can separate schema-only types if needed. |
| `tagpath::extract` | It walks the filesystem and optionally calls tree-sitter. |
| `tagpath::search` | It scans source paths and returns filesystem-backed matches. |
| `tagpath::lint` | It combines config, extraction, and agent-doc file linting. |
| `tagpath::index` / `tagpath::meta_index` | They own on-disk schemas, hashes, sidecars, and update/write flows. |
| `tagpath::ontology` | It loads `.naming/tags/*.md` from disk. |
| `tagpath::graph` | It depends on project extraction and `petgraph`. |
| `tagpath::rename` | It plans and writes source edits. |
| `tagpath::mcp` | It is a stdio server plus harness installer surface. |
| `tagpath::treesitter` | It owns native parser bindings and dynamic grammar loading. |
| `tagpath::watch` | It owns native filesystem watching, PID locks, signals, and NDJSON events. |
| `tagpath::wasm` | It remains the wasm-bindgen adapter that reuses `tagpath-core`; do not publish it as a separate crate during the first split. |

### 18.3 Dependency boundary

`tagpath-core` starts with only:

- `serde = { version = "1", features = ["derive"] }`
- Rust standard library collections/path types

It must not depend on `clap`, `regex`, `toml`, `tree-sitter`,
`tree-sitter-*`, `petgraph`, `walkdir`, `sha2`, `bincode`,
`wasm-bindgen`, `serde-wasm-bindgen`, `js-sys`, `libloading`,
`tree-sitter-language`, `dirs`, `notify`, or `libc`.

The root `tagpath` crate keeps the native dependency graph and may reduce
duplicates later. `cargo test -p tagpath-core --no-default-features`
proves the pure crate without native features, while the existing root
`tagpath` test suite still proves the facade.

### 18.4 Compatibility re-exports

The split preserves existing imports. Root `src/lib.rs` re-exports core
modules by module name:

```rust
pub use tagpath_core::{alias, compression, family, parser, prose, query};
```

Existing downstream code such as `tagpath::parser::parse`,
`tagpath::alias::generate_aliases`, `tagpath::family::generate_family`,
`tagpath::prose::to_prose`, `tagpath::query::normalize_query`, and
`tagpath::compression::build_report` must continue to compile. Keep the
current `tests/lib_api.rs` coverage in the facade crate, and add core
crate tests for the moved modules so both layers have direct proof.

Do not re-export the entire core crate as an undifferentiated prelude in
the first pass. Named module re-exports keep the old public surface
obvious and make accidental API expansion easier to review.

The root package is also the compatibility facade for runtime behavior:
it keeps the `tagpath` binary name and `src/main.rs` entrypoint, the
`treesitter` binary feature gate, the `cdylib`/`rlib` library outputs,
and the existing default feature set for language grammars, MCP, and
watch mode. Facade tests must exercise both the old `tagpath::...`
library paths and CLI commands backed by core modules (`parse`, `alias`,
`family`, `prose`, and `normalize-query`).

### 18.5 Implementation checklist

1. `crates/tagpath-core` has its own `Cargo.toml` and `src/lib.rs`.
2. `parser`, `alias`, `family`, `prose`, `query`, and `compression`
   live in the core crate with preserved module names and unit tests.
3. The root `tagpath` crate depends on `tagpath-core` and re-exports the
   moved modules by name from `tagpath_core`.
4. Native modules continue to use stable `crate::{parser, family, ...}`
   paths through those re-exports.
5. `tests/lib_api.rs` remains facade compatibility coverage, and
   `cargo test -p tagpath-core --no-default-features` is part of the
   verification checklist.

### 18.6 Release impact

The first split should be a minor `tagpath` release, not a breaking
release, because the existing crate name, binary name, feature defaults,
and module import paths stay intact.

Publish order:

1. Publish `tagpath-core`.
2. Publish `tagpath` after the core crate is visible on crates.io.
3. Tag the release only after both packages publish successfully.

Release verification must include:

- `cargo test`
- `cargo clippy`
- `cargo test -p tagpath-core --no-default-features`
- `cargo test -p tagpath --lib --no-default-features`
- `cargo build --target wasm32-unknown-unknown --no-default-features --features wasm`
- `cargo install --path . --force`
- A crates.io dry run for both packages before the real publish.

CI must keep those checks split-aware:

- `.github/workflows/ci.yml` runs workspace clippy/tests, the
  `tagpath-core` no-default-features test, the root facade library
  no-default-features test, the plain wasm target build, and
  `scripts/check-release.sh`.
- `.github/workflows/wasm-build.yml` runs `scripts/build-wasm.sh` and
  `pkg-smoke/smoke.mjs` so the merged npm package remains covered.
- `.github/workflows/release.yml` gates release artifacts behind the same
  workspace clippy/test/no-default/wasm/publish-order smoke checks.

`scripts/check-release.sh` is intentionally publish-order aware. It must
fail if `tagpath-core` cannot dry-run package successfully. The root
`tagpath` dry run may pass, or it may report that crates.io cannot yet
resolve the same-version `tagpath-core`; that dependency-resolution
blocker is accepted until `tagpath-core` has been published, and any
other facade packaging error stays fatal.

### 18.7 Adapter crate decision

Current decision: do not add separate Rust crates named
`tagpath-wasm`, `tagpath-mcp`, or `tagpath-project` during the first
split. The only new crate in this release train is `tagpath-core`.

`tagpath::wasm` remains the wasm-bindgen adapter in the root facade, and
`@btakita/tagpath-wasm` remains an npm package produced by
`scripts/build-wasm.sh`, not a Rust crate. That adapter reuses
`tagpath-core` behind the existing `wasm` feature and continues to be
validated by the wasm target build plus `.github/workflows/wasm-build.yml`.

`tagpath::mcp` stays in the root facade because it owns JSON-RPC stdio,
harness config installation, project config loading, indexed search, and
filesystem-backed lint/search behavior. Those concerns still depend on
the native facade modules listed in 18.2, so splitting a `tagpath-mcp`
crate now would either duplicate facade dependencies or freeze adapter
APIs before the core boundary has been exercised by downstream users.

`tagpath-project` is deferred. Project, config, index, search, ontology,
graph, and rename surfaces stay in the root facade until published
`tagpath-core` consumers prove a separate project-model crate would reduce
real dependency weight without fragmenting the public contract.

Revisit adapter crates only after `tagpath-core` 0.12.x has shipped and a
downstream consumer needs a narrower crate boundary than the existing root
facade plus `tagpath-core` provides.

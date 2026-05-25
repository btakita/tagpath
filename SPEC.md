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
- `--update`: incrementally updates the on-disk index by reusing cached source entries and re-extracting only files whose content actually changed. Falls back to a full rebuild (with a one-line stderr notice naming the reason) when the on-disk index is missing or unreadable, when the `schema_version` differs from the running binary, or when the resolved `config_fingerprint` differs from the on-disk value. On success, prints a one-line stderr digest: `[tagpath] incremental update: <changed> changed, <added> added, <removed> removed, <unchanged> unchanged (<ms>ms)`. Suppressible via `TAGPATH_QUIET=1`. The result is byte-identical to a full rebuild modulo `generated_at`. Writes are atomic via `.naming/index.json.tmp` → `rename(2)` so an interrupted update never produces a partially-written file.
- `--update --force-full`: forces a full rebuild but keeps the digest summary format (`[tagpath] full rebuild: <sources> sources, <families> families (<ms>ms)`).
- `--update --emit jsonl`: streams NDJSON with a leading `{"type":"update_plan","changed":N,"added":N,"removed":N,"unchanged":N}` record before the standard `header` / `source` / `family` / `member` / `footer` records.
- Without flags, `tagpath index` rebuilds only when stale, otherwise prints `index is already fresh`.

### 9.14 search --index

When `--index` is passed, `tagpath search` reads `.naming/index.json` instead of rescanning the source tree.

- Tagpath first locates the project root by walking up for `.naming.toml`, then expects the index at `<root>/.naming/index.json`.
- The index is freshness-checked before use. If it is missing, unreadable, or stale (config fingerprint mismatch, schema mismatch, or any source added/removed/modified), `tagpath search --index` exits `2` with a clear error telling the user to run `tagpath index`.
- When the index is fresh, results come directly from the persisted families (no rescanning). Match semantics are identical to live search: every query tag must appear in the family's tag list.

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

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

## 10. Tsift Token-Savings Benchmark Fixture

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

## 11. Extends Resolution

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

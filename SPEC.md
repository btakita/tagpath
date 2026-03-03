# Tag Path Specification

## 1. Overview

Tag Path is a system for decomposing identifiers into canonical tag sequences, enabling semantic equivalence across naming conventions and languages.

Every identifier is a **path** — an ordered sequence of **tags** separated by convention-specific delimiters. The same concept expressed in different conventions produces the same canonical tag list.

## 2. Conventions

Tag Path recognizes five naming conventions:

| Convention | Example | Delimiter |
|-----------|---------|-----------|
| snake_case | `person_name` | `_` |
| camelCase | `personName` | case boundary |
| PascalCase | `PersonName` | case boundary |
| kebab-case | `person-name` | `-` |
| UPPER_SNAKE_CASE | `PERSON_NAME` | `_` |

### 2.1 Convention Detection

Detection is heuristic, applied in order:

1. Contains `_` or `__` → snake_case (or UPPER_SNAKE_CASE if all uppercase + contains `_`)
2. Contains `-` → kebab-case
3. Starts with uppercase letter → PascalCase
4. Otherwise → camelCase

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
```

All five are equivalent.

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
tagpath extract <PATH> [--format text|json] [--ast]
tagpath search <QUERY> <PATH> [--format text|json]
tagpath lint [<PATH>]
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

### 9.4 search

Performs cross-convention semantic search over source files under `<PATH>`.

- The `<QUERY>` is parsed into canonical tags using the same tokenization rules as `parse`.
- All identifiers in `<PATH>` are extracted and decomposed into canonical tags.
- An identifier matches if the query's canonical tags appear as a subsequence of the identifier's canonical tags.
- Matches across all naming conventions: searching for `"user"` finds `user_name`, `userName`, `UserName`, `user-name`, and `USER_NAME`.
- Searching for `"validate_user"` finds `validateUser`, `ValidateUser`, `validate_user`, etc.
- `--format text` (default) outputs matching identifiers with file location. `--format json` outputs a JSON array.

### 9.5 lint

Validates source file identifiers against `.naming.toml` rules.

- Loads the nearest `.naming.toml` (with `extends` resolution) for each file.
- Extracts identifiers from source files (uses tree-sitter AST when available).
- Checks each identifier's convention against the expected convention for its context.
- Reports violations with file path, line number, identifier name, expected convention, and actual convention.

## 10. Extends Resolution

The `extends` field in `.naming.toml` enables composable configuration.

### 10.1 Syntax

```toml
extends = ["rust"]          # extend a single language preset
extends = ["rust", "custom"] # extend multiple presets (applied left to right)
```

### 10.2 Resolution Rules

1. Presets are resolved by name from the `lang/` and `presets/` directories.
2. When extending multiple presets, they are applied left to right — later presets override earlier ones.
3. The extending config's fields override all inherited fields at the same level.
4. Context-level merging: `[contexts.<name>]` sections merge with inherited contexts. Only the fields specified in the extending config replace the parent values; unspecified fields are retained from the parent.
5. Top-level fields (`convention`, `immutable`, `singular`, etc.) are fully replaced if present in the extending config.
6. `[tags.declared]` entries merge additively — the extending config can add new tag declarations without removing inherited ones.

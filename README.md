# tagpath

Parse, lint, and search tag-based identifiers across languages and naming conventions.

**Tag Path** treats every identifier as a path through an ordered sequence of tags. `auth0__user__validate`, `personName`, `PersonName`, and `person_name` all decompose into canonical tag lists — enabling semantic equivalence across conventions.

## Install

```sh
cargo install tagpath
```

## Quick Start

```sh
# Parse an identifier into tags
tagpath parse person_name
# name:       person_name
# convention: snake_case
# tags:       [person, name]
# canonical:  person_name

# Auto-detects convention
tagpath parse personName
# tags:       [person, name]

tagpath parse PersonName
# tags:       [person, name]

# Namespace dimensions (__ separator)
tagpath parse "auth0__user__validate"
# tags:       [auth0, user, validate]
# dimension 0: [auth0]
# dimension 1: [user]
# dimension 2: [validate]
# role:       validator

# JSON output
tagpath parse createContext_auth --format json

# Extract identifiers from source files
tagpath extract src/ --format text
tagpath extract src/ --ast    # AST-aware (tree-sitter)

# Cross-language semantic search
tagpath search "user" src/    # finds user_name, userName, UserName, user-name
tagpath search "validate_user" src/  # finds across all conventions

# Lint against .naming.toml rules
tagpath lint src/

# Generate cross-convention aliases
tagpath alias person_name
# snake_case:      person_name
# camelCase:       personName
# PascalCase:      PersonName
# kebab-case:      person-name
# UPPER_SNAKE_CASE: PERSON_NAME
# Ada_Case:        Person_Name

tagpath alias person_name --convention camelCase
# camelCase:       personName

# Human-readable prose descriptions
tagpath prose create_user_profile
# Creates a user profile

tagpath prose is_valid_email
# Checks if email is valid

# Build tag co-occurrence graphs
tagpath graph src/ --format dot       # DOT output for Graphviz
tagpath graph src/ --format json      # JSON nodes + edges
tagpath graph src/ --query "user"     # subgraph around "user" tag

# Initialize a .naming.toml
tagpath init --lang typescript
tagpath init --preset immutable-tag
```

## Features

- **Convention detection** — auto-detects snake_case, camelCase, PascalCase, kebab-case, UPPER_SNAKE_CASE, Ada_Case
- **Semantic equivalence** — `person_name` = `personName` = `PersonName` → `[person, name]`
- **Role detection** — `create_*` (factory), `use_*` (hook), `set_*` (setter), `is_*` (predicate), etc.
- **Shape detection** — `*_a` (array), `*_r` (record), `*_m` (map), `*$` (signal)
- **Namespace dimensions** — `__` separates semantic dimensions
- **Mixed convention support** — handles `createContext_auth` (camelCase + underscore extension)
- **Language presets** — 39 languages with per-context conventions
- **Configurable** — `.naming.toml` for project-specific conventions
- **Composable configs** — `extends` inherits from language presets with per-context overrides
- **Identifier extraction** — extract identifiers from source files with regex or tree-sitter AST
- **Semantic search** — find identifiers across naming conventions by canonical tag matching
- **Lint** — validate naming conventions against `.naming.toml` rules
- **Tree-sitter integration** — AST-aware extraction for 14 languages with context classification
- **Alias generation** — convert any identifier to all 6 naming conventions
- **Prose conversion** — human-readable descriptions with role/shape awareness
- **Tag graph** — co-occurrence graph of tag relationships across a codebase (petgraph, DOT/JSON output)

## Language Presets

| Language | Default Convention | Key Contexts |
|----------|-------------------|-------------|
| C | snake_case | `_t` suffix types, UPPER_SNAKE macros |
| C++ | snake_case | STL-style, PascalCase classes |
| C# | PascalCase | `I` prefix interfaces, camelCase locals |
| Clojure | kebab-case | `*earmuffs*`, `:keywords`, `?` predicates, `/` namespaces |
| Common Lisp | kebab-case | `*earmuffs*`, `+constants+`, `defun`/`defvar` |
| Crystal | snake_case | PascalCase types, `?`/`!` suffixes, Ruby-inspired |
| CSS | kebab-case | `--` custom properties, BEM patterns |
| D | camelCase | PascalCase types, camelCase constants, `opCall` |
| Dart | camelCase | `_` prefix private, camelCase constants, `factory` keyword |
| Elixir | snake_case | PascalCase modules, `?`/`!` suffixes |
| Erlang | snake_case | PascalCase modules, atoms, `is_` guards |
| F# | camelCase | PascalCase types/modules, `|>` pipeline |
| Gleam | snake_case | PascalCase types/constructors, labeled arguments |
| Go | camelCase | PascalCase exported, `New{Name}` factory |
| Haskell | camelCase | PascalCase types/modules, `mk`/`un` prefixes |
| Java | camelCase | PascalCase classes, `get`/`set`/`is` prefixes |
| JavaScript | camelCase | PascalCase classes, kebab-case files |
| Julia | snake_case | PascalCase types, `!` mutating, Unicode identifiers |
| Kotlin | camelCase | PascalCase classes/objects |
| Lua | snake_case | PascalCase classes, `__` metamethods |
| Nim | camelCase | PascalCase types, style-insensitive, `new{Name}` factory |
| Objective-C | camelCase | PascalCase classes, 2-3 letter prefixes (`NS`, `UI`) |
| OCaml | snake_case | PascalCase modules, `'` type variables |
| Odin | snake_case | Ada_Case types, rich allocation patterns |
| Perl | snake_case | `$`/`@`/`%` sigils, `::` packages, PascalCase classes |
| PHP | camelCase | `$` prefix vars, PascalCase classes |
| Python | snake_case | PascalCase classes, `__dunder__` |
| R | snake_case | dot.case legacy (`is.na`), `<-` replacement functions |
| Racket | kebab-case | `define`, `?` predicates, `!` mutation |
| Ruby | snake_case | PascalCase classes, `?`/`!`/`=` suffixes |
| Rust | snake_case | PascalCase types/traits, `'` lifetime prefix |
| Scala | camelCase | PascalCase constants, `apply`/`unapply` factories |
| Scheme | kebab-case | `define`, `?` predicates, `!` mutation, `set!` |
| Shell | snake_case | UPPER_SNAKE env vars |
| SQL | snake_case | UPPER_SNAKE keywords |
| Swift | camelCase | PascalCase types/protocols |
| TypeScript | camelCase | PascalCase types/interfaces |
| V | snake_case | PascalCase types, Go-inspired, `C.` interop prefix |
| Zig | camelCase | camelCase functions, snake_case variables, PascalCase types |

## Configuration

Create a `.naming.toml` in your project root:

```toml
version = 1
name = "my-project"
convention = "snake_case"
immutable = true
singular = true

[vectors]
join = "_"
namespace = "__"

[patterns]
factory = "create_{name}"
hook = "use_{name}"
setter = "set_{name}"

[tags]
open = true
```

### Composable Configs (`extends`)

Configs can extend language presets and override specific contexts:

```toml
# Extend a language preset
version = 1
name = "my-project"
extends = ["rust"]

[contexts.function]
convention = "camelCase"  # override function convention
```

The `extends` field accepts an array of preset names. Fields from the extending config override inherited values. Context-level overrides merge with the parent — only the specified fields are replaced.

## Tree-sitter Integration

Tagpath uses tree-sitter for AST-aware identifier extraction in 14 languages:

| Language | Grammar Crate | Feature Flag |
|----------|--------------|-------------|
| Rust | tree-sitter-rust | `lang-rust` |
| Python | tree-sitter-python | `lang-python` |
| JavaScript | tree-sitter-javascript | `lang-javascript` |
| TypeScript | tree-sitter-typescript | `lang-typescript` |
| TSX | tree-sitter-typescript | `lang-typescript` |
| Go | tree-sitter-go | `lang-go` |
| C | tree-sitter-c | `lang-c` |
| C++ | tree-sitter-cpp | `lang-cpp` |
| Java | tree-sitter-java | `lang-java` |
| Ruby | tree-sitter-ruby | `lang-ruby` |
| PHP | tree-sitter-php | `lang-php` |
| C# | tree-sitter-c-sharp | `lang-csharp` |
| Swift | tree-sitter-swift | `lang-swift` |
| Kotlin | tree-sitter-kotlin-ng | `lang-kotlin` |

All 14 languages are enabled by default. Disable grammars you don't need to reduce binary size:

```sh
# Install with only Rust and Python grammars
cargo install tagpath --no-default-features --features lang-rust,lang-python

# Install without any tree-sitter (regex-only extraction)
cargo install tagpath --no-default-features
```

All other supported languages (25 of 39 presets) use regex-based identifier extraction.

- Use `--ast` flag with `tagpath extract` to enable tree-sitter mode
- AST extraction classifies identifiers by context (function, type, variable, etc.)

## Roadmap

- **Phase 1** ✅ — Parse, detect conventions, semantic equivalence
- **Phase 2** ✅ — tree-sitter integration, lint command, extract identifiers, semantic search, composable configs
- **Phase 3** ✅ — Alias generation, prose conversion, tag co-occurrence graph

## License

MIT

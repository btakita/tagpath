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
# convention: SnakeCase
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

# Initialize a .naming.toml
tagpath init --lang typescript
tagpath init --preset immutable-tag
```

## Features

- **Convention detection** — auto-detects snake_case, camelCase, PascalCase, kebab-case, UPPER_SNAKE_CASE
- **Semantic equivalence** — `person_name` = `personName` = `PersonName` → `[person, name]`
- **Role detection** — `create_*` (factory), `use_*` (hook), `set_*` (setter), `is_*` (predicate), etc.
- **Shape detection** — `*_a` (array), `*_r` (record), `*_m` (map), `*$` (signal)
- **Namespace dimensions** — `__` separates semantic dimensions
- **Mixed convention support** — handles `createContext_auth` (camelCase + underscore extension)
- **Language presets** — 39 languages with per-context conventions
- **Configurable** — `.naming.toml` for project-specific conventions

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

## Roadmap

- **Phase 1** (current) — Parse, detect conventions, semantic equivalence
- **Phase 2** — tree-sitter integration, lint command, extract identifiers from source
- **Phase 3** — Graph building, search, alias generation, prose conversion

## License

MIT

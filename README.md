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

# Initialize a .naming.yml
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
- **Language presets** — TypeScript, Python, Rust (more coming)
- **Configurable** — `.naming.yml` for project-specific conventions

## Language Presets

| Language | Conventions |
|----------|------------|
| TypeScript | camelCase (vars/functions), PascalCase (types/classes), UPPER_SNAKE_CASE (constants) |
| Python | snake_case (vars/functions), PascalCase (classes), UPPER_SNAKE_CASE (constants) |
| Rust | snake_case (vars/functions/modules), PascalCase (types/traits/enums), UPPER_SNAKE_CASE (constants) |

## Configuration

Create a `.naming.yml` in your project root:

```yaml
version: 1
name: my-project
convention: snake_case
immutable: true
singular: true

vectors:
  join: "_"
  namespace: "__"

patterns:
  factory: "create_{name}"
  hook: "use_{name}"
  setter: "set_{name}"

tags:
  open: true
```

## Roadmap

- **Phase 1** (current) — Parse, detect conventions, semantic equivalence
- **Phase 2** — tree-sitter integration, lint command, extract identifiers from source
- **Phase 3** — Graph building, search, alias generation, prose conversion

## License

MIT

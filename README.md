# tagpath

Parse, lint, and search tag-based identifiers across languages and naming conventions.

**Tag Path** treats every identifier as a path through an ordered sequence of tags. `auth0__user__validate`, `personName`, `PersonName`, and `person_name` all decompose into canonical tag lists — enabling semantic equivalence across conventions.

## Install

```sh
cargo install tagpath
```

The `tagpath` package is the CLI and compatibility facade. Pure
identifier semantics are also available as the `tagpath-core` crate for
library, agent, and wasm-host consumers that do not need filesystem,
tree-sitter, MCP, watch, or CLI dependencies.

No separate Rust crates named `tagpath-wasm`, `tagpath-mcp`, or
`tagpath-project` are part of the first split. The wasm-bindgen adapter,
MCP server, and project/index/search surfaces remain in the root facade
while `tagpath-core` stabilizes. The npm package `@btakita/tagpath-wasm`
is still produced from that facade by `scripts/build-wasm.sh`.

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
tagpath extract src/ --format family-json  # compact grouped families

# Cross-language semantic search
tagpath search "user" src/    # finds user_name, userName, UserName, user-name
tagpath search "validate_user" src/  # finds across all conventions
tagpath search "validate_user" src/ --format family

# Lint against .naming.toml rules
tagpath lint src/

# Lint agent-doc session-document HTML-comment tags (sibling dialect)
tagpath lint --dialect agent-doc tasks/
tagpath lint --dialect agent-doc tasks/foo.md --fs-checks --format json

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

# Stable semantic families for compact callers
tagpath family "auth0__user__validate"
# canonical: auth0_user_validate
# tags:      [auth0, user, validate]
# dimension 0: [auth0] (auth0)
# dimension 1: [user] (user)
# dimension 2: [validate] (validate)
# role:      validator

# Compare raw symbol rows with compact family previews
tagpath compression-report symbols.json --format json

# Human-readable prose descriptions
tagpath prose create_user_profile
# Creates a user profile

tagpath prose is_valid_email
# Checks if email is valid

# Normalize free-text agent prompts into weighted tags
tagpath normalize-query "Find session-review previews for raw_symbol output"
# session   weight:2.0 occurrences:1 sources:[session-review]
# review    weight:2.0 occurrences:1 sources:[session-review]
# raw       weight:2.0 occurrences:1 sources:[raw_symbol]
# symbol    weight:2.0 occurrences:1 sources:[raw_symbol]

# Load and validate stable domain tag docs
tagpath ontology . --format json

# Build tag co-occurrence graphs
tagpath graph src/ --format dot       # DOT output for Graphviz
tagpath graph src/ --format json      # JSON nodes + edges
tagpath graph src/ --query "user"     # subgraph around "user" tag

# Build a persistent project index (.naming/index.json)
tagpath index                          # build or refresh when stale
tagpath index --check                  # exit 0 if fresh, 1 if stale
tagpath index --force                  # rebuild even when fresh
tagpath index --update                 # incremental update — re-extract only files that changed
tagpath search "user" . --index        # query the persisted index instead of rescanning
tagpath rename create_user update_user # rewrite the indexed family across conventions
tagpath meta-index .                   # aggregate sibling .naming/index.json files

# Initialize a .naming.toml
tagpath init --lang typescript
tagpath init --preset immutable-tag

# Run as an MCP (Model Context Protocol) stdio server for coding agents
tagpath mcp
```

### MCP server

`tagpath mcp` speaks line-delimited JSON-RPC 2.0 on stdio. It exposes nine tools — `parse`, `normalize_query`, `lint`, `search`, `ontology_lookup`, `indexed_project_query`, `family_by_path`, `lint_session_doc`, and `index_handle` — that wrap the corresponding library entrypoints. See [SPEC.md §10](SPEC.md) for the full wire format.

Smoke test:

```bash
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | tagpath mcp
```

Generate a ready-to-paste config block for any of the five supported harnesses (Claude Desktop, Claude Code, Codex, OpenCode, Cursor):

```bash
tagpath mcp install --list                       # show known harnesses + paths
tagpath mcp install --print claude-code          # print JSON for Claude Code
tagpath mcp install --apply codex --yes          # merge into ~/.codex/config.toml
```

`--apply` without `--yes` is a dry-run that prints the resolved path and merged preview. `--uninstall <harness> --yes` removes the `tagpath` entry. See [SPEC.md §10.6](SPEC.md#106-installer) for the full harness matrix.

For Claude Desktop specifically (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS, `~/.config/Claude/claude_desktop_config.json` on Linux, `%APPDATA%\Claude\claude_desktop_config.json` on Windows), the block is:

```json
{
  "mcpServers": {
    "tagpath": {
      "command": "tagpath",
      "args": ["mcp"]
    }
  }
}
```

## Features

- **Convention detection** — auto-detects snake_case, camelCase, PascalCase, kebab-case, UPPER_SNAKE_CASE, Ada_Case
- **Small core crate** — `tagpath-core` exposes parser, alias, family, prose, query normalization, and compression APIs without native or CLI dependencies
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
- **Tag family schema** — compact canonical handles with dimensions, role, shape, aliases, and spelling examples
- **Compact family summaries** — group extract/search output by canonical tags with counts and representative examples
- **Prose conversion** — human-readable descriptions with role/shape awareness
- **Query normalization** — turn free-text agent prompts into ordered, weighted canonical tags
- **Tag ontology** — load `.naming/tags/*.md` domain tags and validate them against `.naming.toml`
- **Tag graph** — co-occurrence graph of tag relationships across a codebase (petgraph, DOT/JSON output)
- **Persistent index** — `tagpath index` snapshots sources, families, ontology refs, and a config fingerprint to `.naming/index.json` for cheap freshness checks (`--check`) and fast indexed search (`search --index`)
- **Workspace meta-index** — `tagpath meta-index <workspace-root>` aggregates sibling `.naming/index.json` files into `.naming/meta-index.json` with workspace-scoped handles for cross-repo lookup.
- **Live watcher** — `tagpath watch` emits NDJSON filesystem events on stdout (`hello`, `index_update`, `lint_finding`, `shutdown`) so tsift, agent-doc editor surfaces, and ad-hoc shell pipelines can react in real time without polling. See [SPEC.md §17](SPEC.md) for the wire format.

```bash
# Start the watcher and grep for lint findings as they happen.
tagpath watch &
tagpath watch | jq -c 'select(.type == "lint_finding")'
```

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

[lint]
allow_mixed_within_identifier = true

[tags]
open = true
```

### Tag Ontology

Projects can define stable domain vocabulary in `.naming/tags/*.md`. Each markdown file represents one canonical tag; the filename stem is the default tag key.

```text
.naming/
  tags/
    session.md
    review.md
```

```md
+++
title = "Session"
summary = "Interactive document session."
domain = "agent-doc"
aliases = ["conversation"]
+++

# Session
```

Run `tagpath ontology .` to validate the files. If `[tags].open = false`, every ontology file must have a matching `[tags.declared.<tag>]` entry in `.naming.toml`. JSON output includes stable tag records that downstream tools can reference by tag and markdown path.

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

## WASM build

Tagpath compiles to `wasm32-unknown-unknown` and ships a small JS-friendly
API. The wasm build has no filesystem dependency — callers extract
identifiers on the host (for example via ts-morph) and pass them in.

```sh
# Plain cargo build (verifies the wasm target compiles)
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm

# Build a Node-targeted pkg/ with wasm-pack
wasm-pack build --target nodejs --no-default-features --features wasm
```

Then from JS:

```js
import { parse, alias, prose, normalize_query, search_over_rows } from "./pkg/tagpath.js";

console.log(parse("createUserProfile"));
console.log(alias("create_user_profile"));
console.log(prose("create_user_profile"));
console.log(normalize_query("create user profile"));

const rows = JSON.stringify([
  { name: "createUser", path: "src/a.ts", line: 10 },
  { name: "deleteUser", path: "src/a.ts", line: 20 },
  { name: "createPost", path: "src/b.ts", line: 30 },
]);
console.log(search_over_rows("user", rows));
```

See `SPEC.md` § 13 for the exposed surface and the no-filesystem rule for
`search_over_rows`.

### npm: `@btakita/tagpath-wasm`

The wasm-pack output is published to npm as a single package with three
target-specific entry points:

```sh
npm install @btakita/tagpath-wasm
```

Node:

```js
import { parse, alias, prose, normalize_query, search_over_rows }
  from "@btakita/tagpath-wasm/nodejs";

console.log(parse("createUserProfile"));
```

Bundler (webpack/vite) or browser-direct — the default export auto-selects:

```js
import { parse } from "@btakita/tagpath-wasm";
// explicit subpaths: "@btakita/tagpath-wasm/bundler", "/web", "/nodejs"
```

Build locally with `scripts/build-wasm.sh` (requires `wasm-pack`). The script
produces a publishable `pkg/` directory and `pkg-smoke/smoke.mjs` exercises
every binding against it.

## Release checks

The workspace is split into `tagpath-core` and the root `tagpath`
CLI/library facade. Before publishing a lockstep release, run:

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p tagpath-core --no-default-features
cargo test -p tagpath --lib --no-default-features
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm
scripts/check-release.sh
```

Publish order matters: publish `tagpath-core` first, then rerun
`scripts/check-release.sh` and publish `tagpath` after crates.io can
resolve the same-version core crate.

## Advanced: dynamic tree-sitter grammars

Tagpath can load tree-sitter grammars from compiled shared libraries at
runtime (similar to Helix / nvim-treesitter). The feature is opt-in and
**native-only** — it is not compiled into the WASM build.

```sh
cargo install tagpath --features dyn-grammar
```

Configure under `.naming.toml`:

```toml
[grammars]
load_dirs = ["./grammars", "~/.config/tagpath/grammars"]

[grammars.languages.zig]
path = "./grammars/tree-sitter-zig.so"
extensions = ["zig"]
```

Then inspect with:

```sh
tagpath grammars list    # show configured + discovered grammars
tagpath grammars check   # exit non-zero if anything fails to load
```

When a dynamic grammar and a compile-time `lang-*` grammar both claim the
same extension, the dynamic grammar wins. See `SPEC.md` § 14 for the full
contract, ABI compatibility rules, and the security note (loading arbitrary
shared libraries is a code-execution boundary — point `load_dirs` only at
trusted directories).

## Consumer integration

External tools (tsift, agent-doc, custom pipelines) can consume the
`.naming/index.json` snapshot as a symbol-graph adapter:

- Every family carries a content-addressable `handle` (`fam:<hex>`) that
  is stable across reindexing and unaffected by member churn.
- Every member carries a `handle` (`mem:<hex>`) that is stable across
  in-file line moves but breaks on rename.
- `tagpath index --schema-version` prints the integer schema version for
  feature detection.
- `tagpath index --emit jsonl` streams the same data as NDJSON for
  pipeline consumers (header / sources / families / members / footer).
- `tagpath index --check --emit jsonl` streams structured stale reasons.

See SPEC.md §15 for the full wire contract, freshness model, and
recommended consumer pattern.

## Roadmap

- **Phase 1** ✅ — Parse, detect conventions, semantic equivalence
- **Phase 2** ✅ — tree-sitter integration, lint command, extract identifiers, semantic search, composable configs
- **Phase 3** ✅ — Alias generation, prose conversion, tag co-occurrence graph
- **Phase 4** ✅ — Index, MCP server, WASM packaging, dynamic grammar loading
- **Phase 5** ✅ — Stable family handles, NDJSON emit, consumer contract (SPEC §15)

## License

MIT

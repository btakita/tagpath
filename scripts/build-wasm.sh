#!/usr/bin/env bash
# Build the @btakita/tagpath-wasm npm package via wasm-pack.
#
# Produces three target-specific outputs (bundler, nodejs, web), then merges
# them into a single publishable pkg/ directory with a single package.json
# whose "exports" map routes each consumer to the correct shim.
#
# Usage:
#   scripts/build-wasm.sh
#
# Does NOT publish to npm — that step is a manual gate.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v wasm-pack >/dev/null 2>&1; then
	echo "error: wasm-pack not found on PATH" >&2
	echo "  install: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh" >&2
	exit 1
fi

# Extract version + metadata from Cargo.toml so package.json always tracks the
# crate version deterministically.
VERSION="$(awk -F\" '/^version = /{print $2; exit}' Cargo.toml)"
DESCRIPTION="$(awk -F\" '/^description = /{print $2; exit}' Cargo.toml)"
LICENSE="$(awk -F\" '/^license = /{print $2; exit}' Cargo.toml)"
REPOSITORY="$(awk -F\" '/^repository = /{print $2; exit}' Cargo.toml)"

PKG_NAME="@btakita/tagpath-wasm"
HOMEPAGE="${REPOSITORY}#readme"

echo "==> tagpath-wasm v${VERSION}"

# Clean previous outputs so stale files don't sneak into the merged pkg/.
rm -rf pkg pkg-bundler pkg-nodejs pkg-web

build_target() {
	local target="$1"
	local outdir="$2"
	echo "==> wasm-pack build --target ${target} -> ${outdir}"
	# wasm-pack's own options (--target, --out-dir, --release) must come
	# BEFORE the crate path; anything after gets forwarded as EXTRA_OPTIONS
	# to `cargo build`. We pass cargo feature flags as extra options so
	# `--no-default-features --features wasm` reach cargo correctly.
	# wasm-pack defaults to --release on stable; passing --release again on
	# >=0.13 errors with "the argument '--release' cannot be used multiple
	# times", so we omit it.
	wasm-pack build \
		--target "$target" \
		--out-dir "$outdir" \
		. \
		-- \
		--no-default-features \
		--features wasm
}

build_target bundler pkg-bundler
build_target nodejs  pkg-nodejs
build_target web     pkg-web

echo "==> merging into pkg/"
mkdir -p pkg/bundler pkg/nodejs pkg/web

# Copy each target's generated files into the merged layout. We deliberately
# exclude each target's own package.json because we author the merged one
# below.
copy_target() {
	local src="$1"
	local dst="$2"
	find "$src" -mindepth 1 -maxdepth 1 \
		! -name 'package.json' \
		! -name 'README.md' \
		! -name 'LICENSE*' \
		! -name '.gitignore' \
		-exec cp -R {} "$dst/" \;
}

copy_target pkg-bundler pkg/bundler
copy_target pkg-nodejs  pkg/nodejs
copy_target pkg-web     pkg/web

# wasm-pack's nodejs target emits CommonJS (`module.exports = ...`) while
# bundler+web targets emit ESM. Our merged pkg/package.json declares
# "type": "module" so Node's loader needs a per-directory override to keep
# treating nodejs/*.js as CJS. Bundler+web stay ESM via the inherited type.
cat > pkg/nodejs/package.json <<'JSON'
{
  "type": "commonjs"
}
JSON

# Use the bundler target's .d.ts as the single top-level type definition.
# All three targets emit the same WASM-bindgen surface; the file is named
# after the crate's [lib].name (which defaults to the package name).
# Our [lib].name is `tagpath` so the generated file is `tagpath.d.ts`.
if [[ -f pkg-bundler/tagpath.d.ts ]]; then
	cp pkg-bundler/tagpath.d.ts pkg/tagpath.d.ts
elif [[ -f pkg-nodejs/tagpath.d.ts ]]; then
	cp pkg-nodejs/tagpath.d.ts pkg/tagpath.d.ts
else
	echo "error: no tagpath.d.ts produced by wasm-pack" >&2
	exit 1
fi

# Carry README + LICENSE from the repo root for npm.
cp README.md pkg/README.md
if [[ -f LICENSE-MIT ]]; then
	cp LICENSE-MIT pkg/LICENSE-MIT
elif [[ -f LICENSE ]]; then
	cp LICENSE pkg/LICENSE-MIT
fi

# Author the merged package.json deterministically (so version matches
# Cargo.toml on every run).
cat > pkg/package.json <<JSON
{
  "name": "${PKG_NAME}",
  "version": "${VERSION}",
  "description": "${DESCRIPTION}",
  "license": "${LICENSE}",
  "repository": {
    "type": "git",
    "url": "git+${REPOSITORY}.git"
  },
  "homepage": "${HOMEPAGE}",
  "keywords": [
    "naming",
    "convention",
    "identifier",
    "parser",
    "lint",
    "wasm",
    "tagpath"
  ],
  "type": "module",
  "main": "./nodejs/tagpath.js",
  "module": "./bundler/tagpath.js",
  "browser": "./web/tagpath.js",
  "types": "./tagpath.d.ts",
  "exports": {
    ".": {
      "node": "./nodejs/tagpath.js",
      "browser": "./web/tagpath.js",
      "default": "./bundler/tagpath.js"
    },
    "./nodejs": "./nodejs/tagpath.js",
    "./web": "./web/tagpath.js",
    "./bundler": "./bundler/tagpath.js"
  },
  "files": [
    "nodejs",
    "web",
    "bundler",
    "tagpath.d.ts",
    "LICENSE-MIT",
    "README.md"
  ],
  "sideEffects": [
    "./nodejs/*",
    "./web/*",
    "./bundler/*"
  ]
}
JSON

echo "==> pkg/ contents:"
find pkg -maxdepth 2 -type f | sort

echo
echo "==> pkg/ size:"
du -sh pkg

echo
echo "build-wasm: done (v${VERSION})"

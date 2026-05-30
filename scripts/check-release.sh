#!/usr/bin/env bash
# Split-aware release smoke checks for crates.io packaging.
#
# `tagpath` depends on the lockstep `tagpath-core` version. Before
# `tagpath-core` is visible in the crates.io index, the facade dry run is
# expected to stop at dependency resolution. That is still useful CI signal:
# it proves the core package itself is publishable and the facade is waiting on
# the documented publish order, not failing for an unrelated packaging issue.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> cargo publish --dry-run --allow-dirty -p tagpath-core"
cargo publish --dry-run --allow-dirty -p tagpath-core

echo
echo "==> cargo publish --dry-run --allow-dirty -p tagpath"
facade_log="$(mktemp)"
trap 'rm -f "$facade_log"' EXIT

if cargo publish --dry-run --allow-dirty -p tagpath 2>&1 | tee "$facade_log"; then
	echo "tagpath facade publish dry-run passed"
	exit 0
else
	status=$?
fi

if grep -q "no matching package named.*tagpath-core" "$facade_log"; then
	echo
	echo "tagpath facade publish dry-run is blocked until tagpath-core is published."
	echo "Publish order: cargo publish -p tagpath-core, then rerun this script and publish -p tagpath."
	if [[ "${TAGPATH_RELEASE_CHECK_STRICT_FACADE:-0}" == "1" ]]; then
		exit "$status"
	fi
	exit 0
fi

exit "$status"

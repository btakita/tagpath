#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ITERATIONS="${TAGPATH_BENCH_ITERATIONS:-5}"
FIXTURE_FILES="${TAGPATH_BENCH_FILES:-1000}"
KEEP_FIXTURE="${TAGPATH_BENCH_KEEP:-0}"
MODE="run"
PROJECT_SESSION="0"

usage() {
	cat <<'USAGE'
Usage: scripts/benchmark-current-performance.sh [--plan] [--project-session] [--iterations N] [--files N]

Builds the current tagpath binary, creates a synthetic Rust project, and measures:
  - noop_sidecar_update
  - one_changed_file_update
  - full_reindex
  - watch_save_burst
  - mcp_indexed_project_query
  - mcp_family_by_path_read
  - project_session_mcp_indexed_project_query (--project-session)
  - project_session_mcp_family_by_path_read (--project-session)
  - project_session_save_burst_mcp_read (--project-session)

Environment:
  TAGPATH_BIN             Existing tagpath binary to use instead of target/debug/tagpath
  TAGPATH_BENCH_ROOT      Existing/new fixture root; default is mktemp
  TAGPATH_BENCH_ITERATIONS Number of timed samples; default 5
  TAGPATH_BENCH_FILES     Number of generated Rust files; default 1000
  TAGPATH_BENCH_KEEP      Set to 1 to keep the generated fixture
USAGE
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--plan)
			MODE="plan"
			shift
			;;
		--project-session)
			PROJECT_SESSION="1"
			shift
			;;
		--iterations)
			ITERATIONS="${2:?missing value for --iterations}"
			shift 2
			;;
		--files)
			FIXTURE_FILES="${2:?missing value for --files}"
			shift 2
			;;
		-h|--help)
			usage
			exit 0
			;;
		*)
			echo "error: unknown argument: $1" >&2
			usage >&2
			exit 2
			;;
	esac
done

print_plan() {
	cat <<'PLAN'
case,budget_ms,command
noop_sidecar_update,10,TAGPATH_QUIET=1 tagpath index --update "$fixture"
one_changed_file_update,125,TAGPATH_QUIET=1 tagpath index --update "$fixture" after appending one Rust function
full_reindex,450,TAGPATH_QUIET=1 tagpath index --update --force-full "$fixture"
watch_save_burst,500,tagpath watch "$fixture" --no-lint --emit-shape compact; write a five-save burst; wait for index_update
mcp_indexed_project_query,150,printf tools/call indexed_project_query | tagpath mcp
mcp_family_by_path_read,150,printf tools/call family_by_path | tagpath mcp
PLAN
	if [[ "$PROJECT_SESSION" == "1" ]]; then
		cat <<'PLAN'
project_session_mcp_indexed_project_query,400,printf tools/call indexed_project_query runtime=project_session | tagpath mcp
project_session_mcp_family_by_path_read,350,printf tools/call family_by_path runtime=project_session | tagpath mcp
project_session_save_burst_mcp_read,500,write a five-save burst; printf tools/call family_by_path runtime=project_session | tagpath mcp
PLAN
	fi
}

if [[ "$MODE" == "plan" ]]; then
	print_plan
	exit 0
fi

if ! [[ "$ITERATIONS" =~ ^[0-9]+$ ]] || [[ "$ITERATIONS" -lt 1 ]]; then
	echo "error: iterations must be a positive integer" >&2
	exit 2
fi

if ! [[ "$FIXTURE_FILES" =~ ^[0-9]+$ ]] || [[ "$FIXTURE_FILES" -lt 1 ]]; then
	echo "error: files must be a positive integer" >&2
	exit 2
fi

TAGPATH_BIN="${TAGPATH_BIN:-}"
if [[ -z "$TAGPATH_BIN" ]]; then
	if [[ "$PROJECT_SESSION" == "1" ]]; then
		( cd "$REPO_ROOT" && cargo build --quiet --bin tagpath --features project-session )
	else
		( cd "$REPO_ROOT" && cargo build --quiet --bin tagpath )
	fi
	TAGPATH_BIN="$REPO_ROOT/target/debug/tagpath"
fi

if [[ ! -x "$TAGPATH_BIN" ]]; then
	echo "error: TAGPATH_BIN is not executable: $TAGPATH_BIN" >&2
	exit 2
fi

ROOT="${TAGPATH_BENCH_ROOT:-}"
if [[ -z "$ROOT" ]]; then
	ROOT="$(mktemp -d "${TMPDIR:-/tmp}/tagpath-perf.XXXXXX")"
fi

cleanup() {
	if [[ "$KEEP_FIXTURE" != "1" ]]; then
		rm -rf "$ROOT"
	fi
}
trap cleanup EXIT

now_ms() {
	local ns
	ns="$(date +%s%N 2>/dev/null || true)"
	if [[ "$ns" =~ ^[0-9]+$ ]]; then
		echo $((ns / 1000000))
	else
		echo $(($(date +%s) * 1000))
	fi
}

make_fixture() {
	rm -rf "$ROOT"
	mkdir -p "$ROOT/src" "$ROOT/.naming/tags"
	cat > "$ROOT/.naming.toml" <<'TOML'
version = 1
name = "tagpath-performance-fixture"
convention = "snake_case"
TOML
	cat > "$ROOT/.naming/tags/user.md" <<'MD'
# user

Synthetic benchmark ontology tag.
MD
	local i padded
	for ((i = 1; i <= FIXTURE_FILES; i++)); do
		printf -v padded "%04d" "$i"
		cat > "$ROOT/src/file_${padded}.rs" <<RS
pub fn create_user_${padded}() -> usize { ${i} }
pub fn update_account_${padded}() -> usize { ${i} + 1 }
pub struct UserRecord${padded};
RS
	done
}

measure_ms() {
	local start end
	start="$(now_ms)"
	"$@"
	end="$(now_ms)"
	echo $((end - start))
}

csv_stats() {
	local case_name="$1"
	local budget_ms="$2"
	shift 2
	local values=("$@")
	local sorted count min max median status
	sorted="$(printf '%s\n' "${values[@]}" | sort -n)"
	count="${#values[@]}"
	min="$(printf '%s\n' "$sorted" | sed -n '1p')"
	max="$(printf '%s\n' "$sorted" | sed -n '$p')"
	median="$(printf '%s\n' "$sorted" | sed -n "$(((count + 1) / 2))p")"
	status="pass"
	if [[ "$median" -gt "$budget_ms" ]]; then
		status="over_budget"
	fi
	printf '%s,%s,%s,%s,%s,%s,%s\n' \
		"$case_name" "$budget_ms" "$count" "$min" "$median" "$max" "$status"
}

wait_for_pattern() {
	local file="$1"
	local pattern="$2"
	local timeout_ms="$3"
	local start
	start="$(now_ms)"
	while (( "$(now_ms)" - start < timeout_ms )); do
		if grep -q "$pattern" "$file" 2>/dev/null; then
			return 0
		fi
		sleep 0.05
	done
	return 1
}

wait_for_update_count_gt() {
	local file="$1"
	local before="$2"
	local timeout_ms="$3"
	local start count
	start="$(now_ms)"
	while (( "$(now_ms)" - start < timeout_ms )); do
		count="$(grep -c '"type":"index_update"' "$file" 2>/dev/null || true)"
		if [[ "$count" -gt "$before" ]]; then
			return 0
		fi
		sleep 0.05
	done
	return 1
}

bench_noop_sidecar_update() {
	TAGPATH_QUIET=1 "$TAGPATH_BIN" index --update "$ROOT" >/dev/null
}

bench_one_changed_file_update() {
	local sample="$1"
	printf '\npub fn changed_user_%s() -> usize { %s }\n' "$sample" "$sample" >> "$ROOT/src/file_0001.rs"
	TAGPATH_QUIET=1 "$TAGPATH_BIN" index --update "$ROOT" >/dev/null
}

bench_full_reindex() {
	TAGPATH_QUIET=1 "$TAGPATH_BIN" index --update --force-full "$ROOT" >/dev/null
}

bench_watch_save_burst() {
	local sample="$1"
	local out err pid before start end
	out="$ROOT/watch-${sample}.out"
	err="$ROOT/watch-${sample}.err"
	: > "$out"
	: > "$err"
	TAGPATH_QUIET=1 "$TAGPATH_BIN" watch "$ROOT" --no-lint --emit-shape compact >"$out" 2>"$err" &
	pid="$!"
	if ! wait_for_pattern "$out" '"type":"ready"' 10000; then
		kill "$pid" 2>/dev/null || true
		wait "$pid" 2>/dev/null || true
		echo "error: watcher did not become ready; stderr follows" >&2
		cat "$err" >&2
		return 1
	fi
	before="$(grep -c '"type":"index_update"' "$out" 2>/dev/null || true)"
	start="$(now_ms)"
	for n in 1 2 3 4 5; do
		printf '\npub fn burst_user_%s_%s() -> usize { %s }\n' "$sample" "$n" "$n" >> "$ROOT/src/file_0002.rs"
		sleep 0.02
	done
	if ! wait_for_update_count_gt "$out" "$before" 10000; then
		kill "$pid" 2>/dev/null || true
		wait "$pid" 2>/dev/null || true
		echo "error: watcher did not emit index_update after save burst; stderr follows" >&2
		cat "$err" >&2
		return 1
	fi
	end="$(now_ms)"
	kill -TERM "$pid" 2>/dev/null || true
	wait "$pid" 2>/dev/null || true
	echo $((end - start))
}

bench_mcp_indexed_project_query() {
	printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"indexed_project_query","arguments":{"path":"%s","tag":"user"}}}\n' "$ROOT" \
		| "$TAGPATH_BIN" mcp >/dev/null
}

bench_mcp_family_by_path_read() {
	printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"family_by_path","arguments":{"path":"%s","project_root":"%s"}}}\n' "$ROOT/src/file_0001.rs" "$ROOT" \
		| "$TAGPATH_BIN" mcp >/dev/null
}

bench_project_session_mcp_indexed_project_query() {
	printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"indexed_project_query","arguments":{"path":"%s","tag":"user","runtime":"project_session"}}}\n' "$ROOT" \
		| "$TAGPATH_BIN" mcp >/dev/null
}

bench_project_session_mcp_family_by_path_read() {
	printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"family_by_path","arguments":{"path":"%s","project_root":"%s","runtime":"project_session"}}}\n' "$ROOT/src/file_0001.rs" "$ROOT" \
		| "$TAGPATH_BIN" mcp >/dev/null
}

bench_project_session_save_burst_mcp_read() {
	local sample="$1"
	local start end
	start="$(now_ms)"
	for n in 1 2 3 4 5; do
		printf '\npub fn session_burst_user_%s_%s() -> usize { %s }\n' "$sample" "$n" "$n" >> "$ROOT/src/file_0003.rs"
		sleep 0.02
	done
	bench_project_session_mcp_family_by_path_read
	end="$(now_ms)"
	echo $((end - start))
}

run_samples() {
	local case_name="$1"
	local budget_ms="$2"
	local fn_name="$3"
	local values=()
	local i elapsed
	for ((i = 1; i <= ITERATIONS; i++)); do
		if [[ "$fn_name" == "bench_watch_save_burst" ]]; then
			elapsed="$("$fn_name" "$i")"
		elif [[ "$fn_name" == "bench_project_session_save_burst_mcp_read" ]]; then
			elapsed="$("$fn_name" "$i")"
		elif [[ "$fn_name" == "bench_one_changed_file_update" ]]; then
			elapsed="$(measure_ms "$fn_name" "$i")"
		else
			elapsed="$(measure_ms "$fn_name")"
		fi
		values+=("$elapsed")
	done
	csv_stats "$case_name" "$budget_ms" "${values[@]}"
}

make_fixture
"$TAGPATH_BIN" index --force "$ROOT" >/dev/null
TAGPATH_QUIET=1 "$TAGPATH_BIN" index --update "$ROOT" >/dev/null

echo "# tagpath performance benchmark"
echo "# binary=$TAGPATH_BIN"
echo "# fixture=$ROOT"
echo "# files=$FIXTURE_FILES iterations=$ITERATIONS"
echo "case,budget_ms,runs,min_ms,median_ms,max_ms,status"
run_samples "noop_sidecar_update" 10 "bench_noop_sidecar_update"
run_samples "one_changed_file_update" 125 "bench_one_changed_file_update"
run_samples "full_reindex" 450 "bench_full_reindex"
run_samples "watch_save_burst" 500 "bench_watch_save_burst"
run_samples "mcp_indexed_project_query" 150 "bench_mcp_indexed_project_query"
run_samples "mcp_family_by_path_read" 150 "bench_mcp_family_by_path_read"
if [[ "$PROJECT_SESSION" == "1" ]]; then
	run_samples "project_session_mcp_indexed_project_query" 400 "bench_project_session_mcp_indexed_project_query"
	run_samples "project_session_mcp_family_by_path_read" 350 "bench_project_session_mcp_family_by_path_read"
	run_samples "project_session_save_burst_mcp_read" 500 "bench_project_session_save_burst_mcp_read"
fi

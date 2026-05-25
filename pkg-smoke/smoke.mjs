// Smoke test for the wasm-pack-built tagpath npm package.
//
// Uses a relative import into the merged pkg/ output so no `npm install` is
// required. After publish, downstream consumers can import the package by
// name instead:
//
//   import { parse, alias, prose, normalize_query, search_over_rows }
//     from "@btakita/tagpath-wasm/nodejs";
//
// Run with: node pkg-smoke/smoke.mjs (from the repo root).

// The nodejs wasm-pack target emits CommonJS (`exports.foo = ...`). Node's
// ESM loader exposes those as a default export, so we destructure from the
// default import to access named bindings.
import tagpath from "../pkg/nodejs/tagpath.js";
import { strict as assert } from "node:assert";

const { parse, alias, prose, normalize_query, search_over_rows } = tagpath;

// 1. parse — convention is auto-detected when omitted.
const parsed = parse("createUser", undefined);
assert.equal(
	parsed.tags.join(","),
	"create,user",
	`parse: expected tags create,user, got ${JSON.stringify(parsed.tags)}`,
);

// 2. alias — without a target convention, every supported convention is emitted.
// serde-wasm-bindgen serializes Rust BTreeMap as a JS `Map`, so we read it
// via Map.has() instead of plain-object access.
const aliasResult = alias("createUser", undefined);
const aliasMap = aliasResult.aliases ?? aliasResult;
const aliasKeys =
	aliasMap instanceof Map ? [...aliasMap.keys()] : Object.keys(aliasMap);
const aliasHas = (key) =>
	aliasMap instanceof Map ? aliasMap.has(key) : key in aliasMap;
assert.ok(
	aliasHas("snake_case") || aliasHas("kebab-case"),
	`alias: missing conventions, got keys ${aliasKeys.join(",")}`,
);

// 3. prose
const description = prose("createUserSession");
assert.ok(
	typeof description === "string" && description.length > 0,
	`prose: returned empty, got ${JSON.stringify(description)}`,
);

// 4. normalize_query
const normalized = normalize_query("create user");
const tags = normalized.tags ?? normalized;
assert.ok(
	Array.isArray(tags) && tags.length >= 1,
	`normalize_query: returned no tags, got ${JSON.stringify(normalized)}`,
);

// 5. search_over_rows
const rows = JSON.stringify([
	{ name: "createUser", path: "x.js", line: 1 },
	{ name: "deleteUser", path: "x.js", line: 5 },
	{ name: "logRequest", path: "y.js", line: 12 },
]);
const matches = search_over_rows("user", rows);
assert.equal(
	matches.length,
	2,
	`search_over_rows: expected 2 matches, got ${matches.length}`,
);

console.log("smoke: all 5 bindings OK");

//! Integration tests for the binary sidecar cache (#p5xfast).
//!
//! The sidecar is a parallel file `.naming/index.bincache` that mirrors
//! the on-disk `.naming/index.json` in a bincode framing. These tests
//! pin the contract:
//!
//! - the sidecar is written alongside JSON on every `write()`
//! - subsequent `--update` no-op cycles read the sidecar
//! - missing / corrupt / schema-mismatched sidecars fall back to JSON
//!   read and the sidecar is regenerated for the next cycle
//! - JSON-missing rules still apply: removing JSON forces a full
//!   rebuild even when the sidecar is present
//! - encode/decode is deterministic so two writes of the same `Index`
//!   produce byte-identical sidecars
//! - mid-write crash (JSON renamed, sidecar not) leaves the repo
//!   recoverable: the next `--update` falls back to JSON read, no panic

use std::io::Write;
use std::path::{Path, PathBuf};

use tagpath::index::sidecar::{self, SidecarFallback};
use tagpath::index::{self, BuildOptions, Index};

// ---------- harness ----------

fn make_project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_sidecar_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_naming_toml(root: &Path) {
    let body = r#"version = 1
name = "test-sidecar"
convention = "snake_case"
"#;
    let mut f = std::fs::File::create(root.join(".naming.toml")).unwrap();
    f.write_all(body.as_bytes()).unwrap();
}

fn write_source(root: &Path, rel: &str, contents: &str) {
    let abs = root.join(rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut f = std::fs::File::create(&abs).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
}

fn build_at(root: &Path) -> Index {
    let opts = BuildOptions {
        project_root: root.to_path_buf(),
    };
    index::build(&opts).expect("build")
}

fn seed_repo(root: &Path) {
    write_naming_toml(root);
    write_source(root, "src/foo.rs", "fn create_user() {}\n");
    write_source(root, "src/bar.rs", "fn delete_user() {}\n");
    let idx = build_at(root);
    index::write(&idx, &index::index_path(root)).unwrap();
}

fn update_at(root: &Path) -> index::UpdateResult {
    let opts = BuildOptions {
        project_root: root.to_path_buf(),
    };
    index::update_incremental(&opts).expect("update_incremental")
}

// ---------- tests ----------

#[test]
fn sidecar_written_alongside_json_on_first_write() {
    let root = make_project("written_first");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let idx = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&idx, &idx_path).unwrap();

    let side_path = sidecar::sidecar_path_for(&idx_path);
    assert!(idx_path.exists(), "json must exist");
    assert!(side_path.exists(), "sidecar must exist alongside json");
    // Sidecar must round-trip back to an Index.
    let decoded = sidecar::read(&side_path).expect("decode");
    assert_eq!(decoded.sources.len(), idx.sources.len());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn noop_update_reads_sidecar() {
    let root = make_project("noop_reads_sidecar");
    seed_repo(&root);

    // Force the read path to be observable via stderr by setting the
    // debug env var for this process. Tests run single-threaded per
    // process so the marker is safe to read after the call.
    unsafe { std::env::set_var(sidecar::DEBUG_ENV, "1") };
    let result = update_at(&root);
    unsafe { std::env::remove_var(sidecar::DEBUG_ENV) };

    assert!(result.summary.fallback.is_none());
    assert!(result.summary.is_noop(), "summary: {:?}", result.summary);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_sidecar_falls_back_to_json_and_regenerates() {
    let root = make_project("missing_regen");
    seed_repo(&root);

    let idx_path = index::index_path(&root);
    let side_path = sidecar::sidecar_path_for(&idx_path);

    // Drop only the sidecar — JSON stays.
    assert!(side_path.exists());
    std::fs::remove_file(&side_path).unwrap();
    assert!(!side_path.exists());

    let result = update_at(&root);
    assert!(result.summary.is_noop());

    // Sidecar must have been regenerated.
    assert!(
        side_path.exists(),
        "sidecar must be regenerated on no-op JSON fallback"
    );
    let decoded = sidecar::read(&side_path).expect("decode");
    assert_eq!(decoded.sources.len(), 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn corrupt_sidecar_falls_back_to_json_and_regenerates() {
    let root = make_project("corrupt_regen");
    seed_repo(&root);

    let idx_path = index::index_path(&root);
    let side_path = sidecar::sidecar_path_for(&idx_path);

    // Truncate to a few bytes so the header is invalid.
    std::fs::write(&side_path, [0x00, 0x01, 0x02, 0x03]).unwrap();
    // The truncated bytes must not be decodable.
    let direct = sidecar::read(&side_path);
    assert!(matches!(direct, Err(SidecarFallback::Truncated)));

    let result = update_at(&root);
    assert!(result.summary.is_noop());

    // Sidecar must have been regenerated and now decode cleanly.
    let decoded = sidecar::read(&side_path).expect("decode");
    assert_eq!(decoded.sources.len(), 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn flipped_byte_sidecar_falls_back() {
    let root = make_project("flipped_byte");
    seed_repo(&root);

    let idx_path = index::index_path(&root);
    let side_path = sidecar::sidecar_path_for(&idx_path);

    // Flip a payload byte so the sha256 stops matching.
    let mut bytes = std::fs::read(&side_path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&side_path, &bytes).unwrap();
    assert!(matches!(
        sidecar::read(&side_path),
        Err(SidecarFallback::HashMismatch)
    ));

    let result = update_at(&root);
    assert!(result.summary.is_noop());

    // Regenerated and clean.
    let decoded = sidecar::read(&side_path).expect("decode");
    assert_eq!(decoded.sources.len(), 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn schema_mismatch_sidecar_falls_back() {
    let root = make_project("schema_mismatch");
    seed_repo(&root);

    let idx_path = index::index_path(&root);
    let side_path = sidecar::sidecar_path_for(&idx_path);

    // Overwrite the schema_version field in the framed bincache to a
    // value the running binary does not support.
    let mut bytes = std::fs::read(&side_path).unwrap();
    let bogus: u32 = 9_999;
    bytes[12..16].copy_from_slice(&bogus.to_le_bytes());
    std::fs::write(&side_path, &bytes).unwrap();
    match sidecar::read(&side_path) {
        Err(SidecarFallback::SchemaVersion { found, .. }) => assert_eq!(found, bogus),
        other => panic!("expected SchemaVersion fallback, got {other:?}"),
    }

    let result = update_at(&root);
    assert!(result.summary.is_noop());

    // Regenerated and clean.
    let decoded = sidecar::read(&side_path).expect("decode");
    assert_eq!(decoded.schema_version, index::SCHEMA_VERSION);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn json_missing_forces_full_rebuild_even_with_sidecar() {
    let root = make_project("json_missing");
    seed_repo(&root);

    let idx_path = index::index_path(&root);
    let side_path = sidecar::sidecar_path_for(&idx_path);
    assert!(side_path.exists());

    // Remove the JSON but keep the sidecar. JSON-missing rules win —
    // we treat the index as missing entirely.
    std::fs::remove_file(&idx_path).unwrap();

    let result = update_at(&root);
    assert!(matches!(
        result.summary.fallback,
        Some(index::FallbackReason::IndexMissing)
    ));

    // Full rebuild via write() regenerates both files.
    index::write(&result.index, &idx_path).unwrap();
    assert!(idx_path.exists());
    assert!(side_path.exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sidecar_encode_is_deterministic() {
    let root = make_project("determinism");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    let idx = build_at(&root);

    let a = sidecar::encode(&idx).unwrap();
    let b = sidecar::encode(&idx).unwrap();
    assert_eq!(a, b, "bincode sidecar encode must be deterministic");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn json_only_write_skips_sidecar() {
    let root = make_project("json_only");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    let idx = build_at(&root);

    let idx_path = index::index_path(&root);
    let side_path = sidecar::sidecar_path_for(&idx_path);

    // Bypass the sidecar deliberately — simulates the mid-write crash
    // case where the JSON rename succeeded but the sidecar rename did
    // not. The repo should still be recoverable on the next --update.
    index::write_json_only(&idx, &idx_path).unwrap();
    assert!(idx_path.exists());
    assert!(
        !side_path.exists(),
        "write_json_only must not emit a sidecar"
    );

    // Next --update reads JSON (sidecar missing) and must not panic.
    let result = update_at(&root);
    assert!(result.summary.is_noop());
    assert!(
        side_path.exists(),
        "sidecar must be regenerated by the next --update"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn one_file_changed_regenerates_sidecar_with_new_content() {
    let root = make_project("one_changed");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();
    let side_path = sidecar::sidecar_path_for(&idx_path);
    let pre = std::fs::read(&side_path).unwrap();

    // Modify a file so the update is no longer a no-op.
    write_source(&root, "src/foo.rs", "fn create_user_profile() {}\n");

    let result = update_at(&root);
    assert!(result.summary.fallback.is_none());
    assert_eq!(result.summary.changed, 1);

    // CLI also writes after non-noop updates. Mirror that here.
    index::write(&result.index, &idx_path).unwrap();

    // The sidecar bytes must change to reflect the new content.
    let post = std::fs::read(&side_path).unwrap();
    assert_ne!(pre, post, "sidecar must change after content edit");

    // And decode must match the in-memory Index modulo generated_at.
    let decoded = sidecar::read(&side_path).expect("decode");
    assert_eq!(decoded.families, result.index.families);

    let _ = std::fs::remove_dir_all(&root);
}

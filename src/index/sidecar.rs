//! Binary sidecar cache for `.naming/index.json` (p5xfast).
//!
//! The sidecar is a parallel file `.naming/index.bincache` that contains a
//! bincode-encoded copy of the same `Index` struct that lives in
//! `.naming/index.json`. The bincode encode/decode path is much faster
//! than serde_json for the same payload, which lets `tagpath index
//! --update` no-op cycles hit the ≥10x perf bar even on large (1000+
//! source) repos.
//!
//! ## On-disk layout
//!
//! The payload is split into two bincode sections so the no-op fast path
//! can decode only what it needs (the `head` — schema/config metadata
//! plus the `sources` array used to detect change) and skip the heavier
//! `tail` (`families` + `ontology_refs`) until something actually
//! changed.
//!
//! ```text
//!   offset  bytes  meaning
//!   ------  -----  --------------------------------------------
//!        0      8  magic = b"TPIDX01\0"
//!        8      4  wrapper_version (u32 LE)   — currently 1
//!       12      4  schema_version  (u32 LE)   — must match SCHEMA_VERSION
//!       16      4  head_len        (u32 LE)
//!       20      4  tail_len        (u32 LE)
//!       24     32  sha256(head_bytes)         — integrity check, head
//!       56     32  sha256(tail_bytes)         — integrity check, tail
//!       88   head  bincode payload of [`SidecarHead`]
//!      ...   tail  bincode payload of [`SidecarTail`]
//! ```
//!
//! ## Contract
//!
//! - **Not source of truth.** `.naming/index.json` is canonical; the
//!   sidecar is an additive build artifact. Repos may be cloned without
//!   it and the first `--update` will regenerate it from JSON.
//! - **Self-validating.** Any of {missing magic, wrapper-version skew,
//!   schema-version skew, truncated payload, sha256 mismatch} triggers
//!   the fallback to JSON read on the consumer side.
//! - **Atomic writes.** Same `.tmp` + `rename(2)` pattern as the JSON
//!   write path. The JSON file is renamed first so a mid-write crash
//!   between the two renames leaves the JSON authoritative and the
//!   sidecar absent or stale — never the reverse.
//! - **Consumer-invisible.** No CLI surface; no public types beyond the
//!   sidecar path helper. SPEC §15 lists it under the consumer contract
//!   only to say "ignore it".
//!
//! See SPEC.md §9 and §15 for the consumer-facing notes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Family, Index, OntologyRef, SCHEMA_VERSION, Source};

/// Sidecar filename, relative to the project root's `.naming/` directory.
pub const SIDECAR_RELATIVE_NAME: &str = "index.bincache";

/// 8-byte magic. The trailing digit + NUL is the wrapper format version
/// baked into the magic so a future format break is unambiguous on read.
const MAGIC: &[u8; 8] = b"TPIDX01\0";

/// Frame header size in bytes (magic + wrapper_version + schema_version
/// + head_len + tail_len + head_sha256 + tail_sha256).
const FRAME_HEADER_LEN: usize = 8 + 4 + 4 + 4 + 4 + 32 + 32;

/// Current wrapper format version. Bump if the framing changes; the magic
/// stays at `TPIDX01\0` until a true breaking change.
const WRAPPER_VERSION: u32 = 1;

/// Head payload — the cheap-to-decode prefix used by the no-op fast
/// path. Mirrors the first half of [`Index`] but with the families /
/// ontology_refs split out to keep this section small (a few hundred KB
/// on a 1000-source repo, vs 4-5 MB for the full payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarHead {
    pub generated_at: String,
    pub config_fingerprint: String,
    pub tool_version: String,
    pub sources: Vec<Source>,
}

/// Tail payload — the heavier `families` + `ontology_refs` sections,
/// only decoded when an `--update` cycle is non-noop or when callers
/// explicitly need the full `Index` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarTail {
    pub families: Vec<Family>,
    pub ontology_refs: Vec<OntologyRef>,
}

/// Env var that, when set to a truthy value, causes the sidecar read path
/// to log the chosen branch to stderr. Used by tests to assert which path
/// was taken without exposing a public marker. Truthy values: `1`, `true`,
/// `yes`, `on` (case-insensitive). Anything else (including absent) is
/// treated as off.
pub const DEBUG_ENV: &str = "TAGPATH_SIDECAR_DEBUG";

/// Compute the sidecar path that lives alongside the given JSON index path.
///
/// e.g. `.naming/index.json` → `.naming/index.bincache`.
pub fn sidecar_path_for(json_path: &Path) -> PathBuf {
    let parent = json_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(SIDECAR_RELATIVE_NAME)
}

/// Why a sidecar read fell back to JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarFallback {
    /// The sidecar file does not exist.
    Missing,
    /// I/O error opening or reading the file.
    Io(String),
    /// Header was too short to contain the framing.
    Truncated,
    /// Magic bytes did not match `TPIDX01\0`.
    BadMagic,
    /// Wrapper format version differs from the running binary.
    WrapperVersion { found: u32, expected: u32 },
    /// Schema version baked into the sidecar differs from
    /// [`SCHEMA_VERSION`].
    SchemaVersion { found: u32, expected: u32 },
    /// Header claimed a payload length larger than the on-disk file.
    PayloadTruncated { expected: usize, actual: usize },
    /// sha256 of the bincode bytes did not match the digest in the header.
    HashMismatch,
    /// bincode failed to decode the payload (corruption or a struct skew
    /// the schema version did not catch).
    Decode(String),
}

impl SidecarFallback {
    /// Short human-readable label for stderr / test assertions.
    pub fn as_str(&self) -> String {
        match self {
            SidecarFallback::Missing => "missing".to_string(),
            SidecarFallback::Io(m) => format!("io: {m}"),
            SidecarFallback::Truncated => "truncated_header".to_string(),
            SidecarFallback::BadMagic => "bad_magic".to_string(),
            SidecarFallback::WrapperVersion { found, expected } => {
                format!("wrapper_version({found}!={expected})")
            }
            SidecarFallback::SchemaVersion { found, expected } => {
                format!("schema_version({found}!={expected})")
            }
            SidecarFallback::PayloadTruncated { expected, actual } => {
                format!("payload_truncated({actual}<{expected})")
            }
            SidecarFallback::HashMismatch => "hash_mismatch".to_string(),
            SidecarFallback::Decode(m) => format!("decode: {m}"),
        }
    }
}

/// Validated section boundaries inside a sidecar byte buffer.
struct FrameLayout {
    head_range: std::ops::Range<usize>,
    tail_range: std::ops::Range<usize>,
    head_hash: [u8; 32],
    tail_hash: [u8; 32],
}

/// Walk the frame header, check magic + versions + bounds, and return
/// the head/tail byte ranges and their expected sha256s. Cheap (no
/// allocation, no hashing yet) so the fast path can skip the tail.
fn parse_frame(bytes: &[u8]) -> Result<FrameLayout, SidecarFallback> {
    if bytes.len() < FRAME_HEADER_LEN {
        return Err(SidecarFallback::Truncated);
    }
    if &bytes[0..8] != MAGIC {
        return Err(SidecarFallback::BadMagic);
    }
    let wrapper_version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if wrapper_version != WRAPPER_VERSION {
        return Err(SidecarFallback::WrapperVersion {
            found: wrapper_version,
            expected: WRAPPER_VERSION,
        });
    }
    let schema_version = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if schema_version != SCHEMA_VERSION {
        return Err(SidecarFallback::SchemaVersion {
            found: schema_version,
            expected: SCHEMA_VERSION,
        });
    }
    let head_len = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let tail_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let head_hash: [u8; 32] = bytes[24..56].try_into().unwrap();
    let tail_hash: [u8; 32] = bytes[56..88].try_into().unwrap();
    let head_start = FRAME_HEADER_LEN;
    let head_end = head_start
        .checked_add(head_len)
        .ok_or(SidecarFallback::Truncated)?;
    let tail_end = head_end
        .checked_add(tail_len)
        .ok_or(SidecarFallback::Truncated)?;
    if bytes.len() < tail_end {
        return Err(SidecarFallback::PayloadTruncated {
            expected: tail_end,
            actual: bytes.len(),
        });
    }
    Ok(FrameLayout {
        head_range: head_start..head_end,
        tail_range: head_end..tail_end,
        head_hash,
        tail_hash,
    })
}

/// Encode an `Index` into the framed bincache byte string. Pure function,
/// no I/O — exposed for the determinism tests.
pub fn encode(idx: &Index) -> Result<Vec<u8>, String> {
    let head = SidecarHead {
        generated_at: idx.generated_at.clone(),
        config_fingerprint: idx.config_fingerprint.clone(),
        tool_version: idx.tool_version.clone(),
        sources: idx.sources.clone(),
    };
    let tail = SidecarTail {
        families: idx.families.clone(),
        ontology_refs: idx.ontology_refs.clone(),
    };
    let head_bytes = bincode::serde::encode_to_vec(&head, bincode::config::standard())
        .map_err(|e| format!("bincode encode head: {e}"))?;
    let tail_bytes = bincode::serde::encode_to_vec(&tail, bincode::config::standard())
        .map_err(|e| format!("bincode encode tail: {e}"))?;
    let mut head_hasher = Sha256::new();
    head_hasher.update(&head_bytes);
    let head_hash = head_hasher.finalize();
    let mut tail_hasher = Sha256::new();
    tail_hasher.update(&tail_bytes);
    let tail_hash = tail_hasher.finalize();

    let head_len: u32 = head_bytes
        .len()
        .try_into()
        .map_err(|_| "head payload exceeds u32::MAX".to_string())?;
    let tail_len: u32 = tail_bytes
        .len()
        .try_into()
        .map_err(|_| "tail payload exceeds u32::MAX".to_string())?;

    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + head_bytes.len() + tail_bytes.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&WRAPPER_VERSION.to_le_bytes());
    out.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
    out.extend_from_slice(&head_len.to_le_bytes());
    out.extend_from_slice(&tail_len.to_le_bytes());
    out.extend_from_slice(&head_hash);
    out.extend_from_slice(&tail_hash);
    out.extend_from_slice(&head_bytes);
    out.extend_from_slice(&tail_bytes);
    Ok(out)
}

/// Decode the full framed bincache byte string into an `Index`. Verifies
/// both head and tail sha256s and reassembles the full `Index` struct.
///
/// All framing failures map to a [`SidecarFallback`] — callers should
/// silently fall back to the JSON read path on any error.
pub fn decode(bytes: &[u8]) -> Result<Index, SidecarFallback> {
    let layout = parse_frame(bytes)?;
    let head = decode_head_inner(bytes, &layout)?;
    let tail = decode_tail_inner(bytes, &layout)?;
    Ok(Index {
        schema_version: SCHEMA_VERSION,
        generated_at: head.generated_at,
        config_fingerprint: head.config_fingerprint,
        tool_version: head.tool_version,
        sources: head.sources,
        families: tail.families,
        ontology_refs: tail.ontology_refs,
    })
}

/// Decode only the cheap head section (schema/config/sources). Used by
/// the no-op fast path; the tail is left untouched and never allocated.
pub fn decode_head(bytes: &[u8]) -> Result<SidecarHead, SidecarFallback> {
    let layout = parse_frame(bytes)?;
    decode_head_inner(bytes, &layout)
}

/// Decode the tail given a previously-validated byte buffer. Verifies
/// the tail sha256 against the frame header.
pub fn decode_tail(bytes: &[u8]) -> Result<SidecarTail, SidecarFallback> {
    let layout = parse_frame(bytes)?;
    decode_tail_inner(bytes, &layout)
}

fn decode_head_inner(bytes: &[u8], layout: &FrameLayout) -> Result<SidecarHead, SidecarFallback> {
    let head_bytes = &bytes[layout.head_range.clone()];
    let mut hasher = Sha256::new();
    hasher.update(head_bytes);
    let actual = hasher.finalize();
    if actual.as_slice() != layout.head_hash.as_slice() {
        return Err(SidecarFallback::HashMismatch);
    }
    let (head, _): (SidecarHead, usize) =
        bincode::serde::decode_from_slice(head_bytes, bincode::config::standard())
            .map_err(|e| SidecarFallback::Decode(format!("head: {e}")))?;
    Ok(head)
}

fn decode_tail_inner(bytes: &[u8], layout: &FrameLayout) -> Result<SidecarTail, SidecarFallback> {
    let tail_bytes = &bytes[layout.tail_range.clone()];
    let mut hasher = Sha256::new();
    hasher.update(tail_bytes);
    let actual = hasher.finalize();
    if actual.as_slice() != layout.tail_hash.as_slice() {
        return Err(SidecarFallback::HashMismatch);
    }
    let (tail, _): (SidecarTail, usize) =
        bincode::serde::decode_from_slice(tail_bytes, bincode::config::standard())
            .map_err(|e| SidecarFallback::Decode(format!("tail: {e}")))?;
    Ok(tail)
}

/// Read the sidecar at `path` and return the full decoded `Index` (both
/// head and tail). Use [`read_head`] when the caller only needs the
/// cheap prefix and can skip family decoding.
///
/// Returns `Err(fallback_reason)` on any failure — the caller is expected
/// to silently fall back to the JSON read path.
pub fn read(path: &Path) -> Result<Index, SidecarFallback> {
    let bytes = read_bytes(path)?;
    decode(&bytes)
}

/// Read the raw sidecar bytes, performing the cheap existence check.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, SidecarFallback> {
    if !path.exists() {
        return Err(SidecarFallback::Missing);
    }
    std::fs::read(path).map_err(|e| SidecarFallback::Io(format!("{e}")))
}

/// Read the sidecar at `path` and return just the head section, leaving
/// the tail unparsed. This is the no-op fast path used by
/// [`super::update_incremental`].
pub fn read_head(path: &Path) -> Result<SidecarHead, SidecarFallback> {
    let bytes = read_bytes(path)?;
    decode_head(&bytes)
}

/// Atomically write the sidecar for `idx` to `path` using the same
/// `.tmp + rename(2)` pattern as the JSON write path.
pub fn write(idx: &Index, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create_dir_all({}): {e}", parent.display()))?;
    }
    let bytes = encode(idx)?;
    let tmp_path = tmp_path_for(path);
    if let Err(e) = std::fs::write(&tmp_path, &bytes) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("write({}): {e}", tmp_path.display()));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "rename({} -> {}): {e}",
            tmp_path.display(),
            path.display()
        ));
    }
    Ok(())
}

/// Compute the `.tmp` companion path for a sidecar target.
pub fn tmp_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

/// True when [`DEBUG_ENV`] is set to a truthy value.
pub fn debug_enabled() -> bool {
    match std::env::var(DEBUG_ENV) {
        Ok(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => false,
    }
}

/// Print a one-line debug marker to stderr if [`DEBUG_ENV`] is set.
///
/// Format: `[tagpath:sidecar] <event>` where event is `hit`,
/// `miss:<reason>`, or `write`.
pub fn debug_log(event: &str) {
    if debug_enabled() {
        eprintln!("[tagpath:sidecar] {event}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;

    fn empty_index() -> Index {
        Index {
            schema_version: SCHEMA_VERSION,
            generated_at: "2026-05-24T22:00:00Z".to_string(),
            config_fingerprint: "sha256:deadbeef".to_string(),
            tool_version: "0.13.0".to_string(),
            sources: Vec::new(),
            families: Vec::new(),
            ontology_refs: Vec::new(),
        }
    }

    #[test]
    fn round_trip_empty() {
        let idx = empty_index();
        let bytes = encode(&idx).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.schema_version, idx.schema_version);
        assert_eq!(decoded.config_fingerprint, idx.config_fingerprint);
    }

    #[test]
    fn deterministic_encode() {
        let idx = empty_index();
        let a = encode(&idx).unwrap();
        let b = encode(&idx).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn bad_magic_falls_back() {
        let mut bytes = encode(&empty_index()).unwrap();
        bytes[0] = b'X';
        assert!(matches!(decode(&bytes), Err(SidecarFallback::BadMagic)));
    }

    #[test]
    fn truncated_header() {
        let bytes = vec![0u8; 8];
        assert!(matches!(decode(&bytes), Err(SidecarFallback::Truncated)));
    }

    #[test]
    fn hash_mismatch() {
        let mut bytes = encode(&empty_index()).unwrap();
        // Flip a byte deep in the payload so the sha256 differs.
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        assert!(matches!(decode(&bytes), Err(SidecarFallback::HashMismatch)));
    }

    #[test]
    fn schema_version_mismatch() {
        let mut bytes = encode(&empty_index()).unwrap();
        // Bump the embedded schema_version to a value we don't support.
        let bogus: u32 = SCHEMA_VERSION + 99;
        bytes[12..16].copy_from_slice(&bogus.to_le_bytes());
        // The hash now doesn't match, but the schema-version guard runs
        // first by design.
        match decode(&bytes) {
            Err(SidecarFallback::SchemaVersion { found, expected }) => {
                assert_eq!(found, bogus);
                assert_eq!(expected, SCHEMA_VERSION);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}

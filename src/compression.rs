use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{family, parser};

const DEFAULT_EXAMPLE_LIMIT: usize = 3;

/// One raw symbol preview row from an upstream tool such as tsift.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawSymbolRow {
    pub identifier: String,
    pub file: PathBuf,
    pub line: usize,
    #[serde(default)]
    pub column: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// A grouped canonical family row for compact preview output.
#[derive(Debug, Clone, Serialize)]
pub struct CompressionFamilyPreview {
    pub canonical: String,
    pub tags: Vec<String>,
    pub count: usize,
    pub aliases: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<CompressionFamilyExample>,
}

/// One representative raw row inside a grouped family preview.
#[derive(Debug, Clone, Serialize)]
pub struct CompressionFamilyExample {
    pub identifier: String,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub convention: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Byte and deterministic token-estimate comparison between raw and compact previews.
#[derive(Debug, Clone, Serialize)]
pub struct CompressionMetrics {
    pub raw_utf8_bytes: usize,
    pub compact_utf8_bytes: usize,
    pub saved_utf8_bytes: isize,
    pub byte_savings_percent: f64,
    pub raw_tokens: usize,
    pub compact_tokens: usize,
    pub saved_tokens: isize,
    pub token_savings_percent: f64,
    pub token_estimate: &'static str,
}

/// Complete compression report for raw symbol rows.
#[derive(Debug, Clone, Serialize)]
pub struct CompressionReport {
    pub raw_symbol_count: usize,
    pub family_count: usize,
    pub metrics: CompressionMetrics,
    pub families: Vec<CompressionFamilyPreview>,
    pub raw_preview: String,
    pub compact_preview: String,
}

/// Build a tsift-facing compression report with the default example limit.
pub fn build_report(rows: &[RawSymbolRow]) -> CompressionReport {
    build_report_with_example_limit(rows, DEFAULT_EXAMPLE_LIMIT)
}

/// Build a tsift-facing compression report from raw symbol rows.
pub fn build_report_with_example_limit(
    rows: &[RawSymbolRow],
    example_limit: usize,
) -> CompressionReport {
    let raw_preview = render_raw_symbol_preview(rows);
    let occurrences = rows.iter().map(|row| {
        let convention = parser::detect_convention(&row.identifier);
        let parsed = parser::parse(&row.identifier, convention);
        family::FamilyOccurrence {
            identifier: row.identifier.clone(),
            file: row.file.clone(),
            line: row.line,
            column: row.column,
            convention: convention.to_string(),
            tags: parsed.tags,
            role: parsed.role,
            shape: parsed.shape,
            context: row.context.clone(),
        }
    });
    let summaries = family::summarize_occurrences(occurrences, example_limit);
    let families = summaries
        .into_iter()
        .map(|summary| {
            let aliases = family::generate_family(&summary.canonical).aliases;
            let examples = summary
                .examples
                .into_iter()
                .map(|example| CompressionFamilyExample {
                    identifier: example.identifier,
                    file: example.file,
                    line: example.line,
                    column: example.column,
                    convention: example.convention,
                    context: example.context,
                })
                .collect();
            CompressionFamilyPreview {
                canonical: summary.canonical,
                tags: summary.tags,
                count: summary.count,
                aliases,
                examples,
            }
        })
        .collect::<Vec<_>>();
    let compact_preview = render_compact_family_preview(&families);
    let metrics = compression_metrics(&raw_preview, &compact_preview);

    CompressionReport {
        raw_symbol_count: rows.len(),
        family_count: families.len(),
        metrics,
        families,
        raw_preview,
        compact_preview,
    }
}

pub fn render_raw_symbol_preview(rows: &[RawSymbolRow]) -> String {
    rows.iter()
        .map(|row| {
            let convention = parser::detect_convention(&row.identifier);
            let parsed = parser::parse(&row.identifier, convention);
            let role = parsed.role.as_deref().unwrap_or("none");
            let shape = parsed.shape.as_deref().unwrap_or("none");
            let context = row.context.as_deref().unwrap_or("none");
            format!(
                "{}:{}\t{}\t{}\tconvention:{}\ttags:[{}]\trole:{}\tshape:{}",
                row.file.display(),
                row.line,
                context,
                row.identifier,
                parsed.convention,
                parsed.tags.join(","),
                role,
                shape
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_compact_family_preview(families: &[CompressionFamilyPreview]) -> String {
    families
        .iter()
        .map(|family| {
            let aliases = family
                .aliases
                .iter()
                .map(|(convention, spelling)| format!("{convention}:{spelling}"))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}\tcount:{}\taliases:[{}]",
                family.canonical, family.count, aliases
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn estimate_tokens(output: &str) -> usize {
    output.len().div_ceil(4)
}

fn compression_metrics(raw_preview: &str, compact_preview: &str) -> CompressionMetrics {
    let raw_utf8_bytes = raw_preview.len();
    let compact_utf8_bytes = compact_preview.len();
    let raw_tokens = estimate_tokens(raw_preview);
    let compact_tokens = estimate_tokens(compact_preview);
    CompressionMetrics {
        raw_utf8_bytes,
        compact_utf8_bytes,
        saved_utf8_bytes: raw_utf8_bytes as isize - compact_utf8_bytes as isize,
        byte_savings_percent: savings_percent(raw_utf8_bytes, compact_utf8_bytes),
        raw_tokens,
        compact_tokens,
        saved_tokens: raw_tokens as isize - compact_tokens as isize,
        token_savings_percent: savings_percent(raw_tokens, compact_tokens),
        token_estimate: "ceil(utf8_bytes / 4)",
    }
}

fn savings_percent(raw: usize, compact: usize) -> f64 {
    if raw == 0 {
        0.0
    } else {
        ((raw as f64 - compact as f64) / raw as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_groups_rows_and_measures_savings() {
        let rows = vec![
            RawSymbolRow {
                identifier: "raw_symbol".to_string(),
                file: "src/search.rs".into(),
                line: 42,
                column: 0,
                context: Some("field".to_string()),
            },
            RawSymbolRow {
                identifier: "rawSymbol".to_string(),
                file: "src/search.ts".into(),
                line: 9,
                column: 0,
                context: Some("field".to_string()),
            },
            RawSymbolRow {
                identifier: "RawSymbol".to_string(),
                file: "src/search.go".into(),
                line: 12,
                column: 0,
                context: Some("field".to_string()),
            },
            RawSymbolRow {
                identifier: "raw-symbol".to_string(),
                file: "src/search.css".into(),
                line: 11,
                column: 0,
                context: Some("selector".to_string()),
            },
            RawSymbolRow {
                identifier: "RAW_SYMBOL".to_string(),
                file: "src/search.rs".into(),
                line: 44,
                column: 0,
                context: Some("const".to_string()),
            },
            RawSymbolRow {
                identifier: "Raw_Symbol".to_string(),
                file: "src/search.adb".into(),
                line: 4,
                column: 0,
                context: Some("field".to_string()),
            },
            RawSymbolRow {
                identifier: "raw_symbol_output".to_string(),
                file: "src/review.py".into(),
                line: 88,
                column: 0,
                context: Some("field".to_string()),
            },
        ];

        let report = build_report(&rows);
        assert_eq!(report.raw_symbol_count, 7);
        assert_eq!(report.family_count, 2);
        assert_eq!(report.families[0].canonical, "raw_symbol");
        assert_eq!(report.families[0].count, 6);
        assert_eq!(report.families[0].aliases["camelCase"], "rawSymbol");
        assert!(report.metrics.raw_tokens > report.metrics.compact_tokens);
        assert!(report.metrics.token_savings_percent > 0.0);
        assert_eq!(report.metrics.token_estimate, "ceil(utf8_bytes / 4)");
    }
}

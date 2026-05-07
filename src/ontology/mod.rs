use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::{config, parser};

/// Loaded tag ontology plus validation output for a project.
#[derive(Debug, Clone, Serialize)]
pub struct TagOntologyReport {
    /// Project root or explicit path that was inspected.
    pub project_path: PathBuf,
    /// Resolved .naming.toml path, when one was found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<PathBuf>,
    /// Directory containing ontology markdown files.
    pub ontology_dir: PathBuf,
    /// Stable tag records loaded from `.naming/tags/*.md`.
    pub tags: Vec<OntologyTag>,
    /// Validation errors and warnings.
    pub validation: OntologyValidation,
}

/// In-memory ontology indexed by canonical tag.
#[derive(Debug, Clone, Serialize)]
pub struct TagOntology {
    pub root: PathBuf,
    pub tags: BTreeMap<String, OntologyTag>,
}

/// One stable domain tag loaded from markdown.
#[derive(Debug, Clone, Serialize)]
pub struct OntologyTag {
    /// Canonical lowercase tag key.
    pub tag: String,
    /// Markdown file path for the tag definition.
    pub path: PathBuf,
    /// Human-facing title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Compact definition intended for agent summaries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Domain classification, mirrored from frontmatter or `.naming.toml`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Alternate spellings or terms that map to this tag.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

/// Stable reference for one tag inside a downstream summary.
#[derive(Debug, Clone, Serialize)]
pub struct OntologyTagReference {
    pub tag: String,
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OntologyValidation {
    pub valid: bool,
    pub errors: Vec<OntologyDiagnostic>,
    pub warnings: Vec<OntologyDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OntologyDiagnostic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug, Default, Deserialize)]
struct OntologyFrontmatter {
    tag: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    domain: Option<String>,
    level: Option<String>,
    shape: Option<String>,
    role: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
}

impl TagOntology {
    /// Return stable ontology references for the provided canonical tags.
    pub fn references_for_tags(&self, tags: &[String]) -> Vec<OntologyTagReference> {
        let mut seen = BTreeSet::new();
        tags.iter()
            .filter_map(|tag| {
                let ontology_tag = self.tags.get(tag)?;
                if !seen.insert(tag.clone()) {
                    return None;
                }
                Some(OntologyTagReference {
                    tag: ontology_tag.tag.clone(),
                    path: ontology_tag.path.clone(),
                    title: ontology_tag.title.clone(),
                    summary: ontology_tag.summary.clone(),
                    domain: ontology_tag.domain.clone(),
                })
            })
            .collect()
    }
}

/// Load `.naming/tags/*.md` from a project path and validate against
/// `.naming.toml` when present.
pub fn load_project(path: &Path) -> Result<TagOntologyReport, String> {
    let config_path = find_config(path);
    let (project_path, naming_config) = match &config_path {
        Some(config_path) => {
            let project_path = config_path
                .parent()
                .ok_or_else(|| "config path has no parent directory".to_string())?
                .to_path_buf();
            let naming_config = config::resolve(config_path)?;
            (project_path, Some(naming_config))
        }
        None => (path.to_path_buf(), None),
    };
    let ontology_dir = project_path.join(".naming").join("tags");
    let mut ontology = load_dir(&ontology_dir)?;
    if let Some(naming_config) = naming_config.as_ref() {
        apply_config_metadata(&mut ontology, naming_config);
    }
    let validation = validate(&ontology, naming_config.as_ref());
    Ok(TagOntologyReport {
        project_path,
        config_path,
        ontology_dir,
        tags: ontology.tags.into_values().collect(),
        validation,
    })
}

/// Load markdown tag files from a `.naming/tags` directory.
pub fn load_dir(dir: &Path) -> Result<TagOntology, String> {
    let mut tags = BTreeMap::new();
    if !dir.exists() {
        return Ok(TagOntology {
            root: dir.to_path_buf(),
            tags,
        });
    }
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let tag = load_tag_file(path)?;
        if tags.insert(tag.tag.clone(), tag).is_some() {
            return Err(format!("duplicate ontology tag '{}'", path.display()));
        }
    }
    Ok(TagOntology {
        root: dir.to_path_buf(),
        tags,
    })
}

/// Validate loaded ontology tags against `.naming.toml` declarations.
pub fn validate(
    ontology: &TagOntology,
    naming_config: Option<&config::NamingConfig>,
) -> OntologyValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for tag in ontology.tags.values() {
        if !is_single_canonical_tag(&tag.tag) {
            errors.push(OntologyDiagnostic {
                tag: Some(tag.tag.clone()),
                path: Some(tag.path.clone()),
                message: "ontology tag must normalize to one canonical tag".to_string(),
            });
        }
        if tag.summary.as_deref().is_none_or(str::is_empty) {
            warnings.push(OntologyDiagnostic {
                tag: Some(tag.tag.clone()),
                path: Some(tag.path.clone()),
                message: "ontology tag has no summary".to_string(),
            });
        }
    }
    if let Some(tags_config) = naming_config.and_then(|config| config.tags.as_ref()) {
        for declared in tags_config.declared.keys() {
            if !ontology.tags.contains_key(declared) {
                warnings.push(OntologyDiagnostic {
                    tag: Some(declared.clone()),
                    path: None,
                    message: "declared tag has no .naming/tags markdown file".to_string(),
                });
            }
        }
        if !tags_config.open {
            for tag in ontology.tags.keys() {
                if !tags_config.declared.contains_key(tag) {
                    errors.push(OntologyDiagnostic {
                        tag: Some(tag.clone()),
                        path: ontology.tags.get(tag).map(|tag| tag.path.clone()),
                        message: "ontology tag is not declared and [tags].open is false"
                            .to_string(),
                    });
                }
            }
        }
    }
    OntologyValidation {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

fn apply_config_metadata(ontology: &mut TagOntology, naming_config: &config::NamingConfig) {
    let Some(tags_config) = naming_config.tags.as_ref() else {
        return;
    };
    for (tag, declaration) in &tags_config.declared {
        let Some(ontology_tag) = ontology.tags.get_mut(tag) else {
            continue;
        };
        if ontology_tag.domain.is_none() {
            ontology_tag.domain = declaration.domain.clone();
        }
        if ontology_tag.level.is_none() {
            ontology_tag.level = declaration.level.clone();
        }
        if ontology_tag.shape.is_none() {
            ontology_tag.shape = declaration.shape.clone();
        }
        if ontology_tag.role.is_none() {
            ontology_tag.role = declaration.role.clone();
        }
    }
}

fn load_tag_file(path: &Path) -> Result<OntologyTag, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let (frontmatter, body) = parse_frontmatter(&content, path)?;
    let file_tag = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("ontology file has no valid stem: {}", path.display()))?
        .to_string();
    let tag = frontmatter.tag.unwrap_or_else(|| file_tag.clone());
    let summary = frontmatter.summary.or_else(|| first_body_paragraph(&body));
    Ok(OntologyTag {
        tag: canonical_tag(&tag),
        path: path.to_path_buf(),
        title: frontmatter.title,
        summary,
        domain: frontmatter.domain,
        level: frontmatter.level,
        shape: frontmatter.shape,
        role: frontmatter.role,
        aliases: frontmatter.aliases,
    })
}

fn parse_frontmatter(content: &str, path: &Path) -> Result<(OntologyFrontmatter, String), String> {
    let Some(rest) = content.strip_prefix("+++\n") else {
        return Ok((OntologyFrontmatter::default(), content.to_string()));
    };
    let Some((frontmatter, body)) = rest.split_once("\n+++\n") else {
        return Err(format!(
            "unterminated TOML frontmatter in {}",
            path.display()
        ));
    };
    let frontmatter = toml::from_str(frontmatter)
        .map_err(|error| format!("failed to parse {} frontmatter: {error}", path.display()))?;
    Ok((frontmatter, body.to_string()))
}

fn first_body_paragraph(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToString::to_string)
}

fn canonical_tag(tag: &str) -> String {
    let convention = parser::detect_convention(tag);
    parser::parse(tag, convention).tags.join("_")
}

fn is_single_canonical_tag(tag: &str) -> bool {
    let convention = parser::detect_convention(tag);
    let parsed = parser::parse(tag, convention);
    parsed.tags.len() == 1 && parsed.tags[0] == tag
}

fn find_config(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let candidate = current.join(".naming.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_project_reads_markdown_tags_and_validates_declarations() {
        let dir = std::env::temp_dir().join("tagpath_test_ontology_load");
        let _ = std::fs::remove_dir_all(&dir);
        let tags_dir = dir.join(".naming").join("tags");
        std::fs::create_dir_all(&tags_dir).unwrap();
        std::fs::write(
            dir.join(".naming.toml"),
            r#"version = 1
name = "ontology-test"

[tags]
open = false

[tags.declared.session]
domain = "agent-doc"

[tags.declared.review]
domain = "agent-doc"
"#,
        )
        .unwrap();
        std::fs::write(
            tags_dir.join("session.md"),
            r#"+++
title = "Session"
summary = "Interactive document session."
domain = "agent-doc"
aliases = ["conversation"]
+++

# Session
"#,
        )
        .unwrap();
        std::fs::write(tags_dir.join("review.md"), "Review task.\n").unwrap();

        let report = load_project(&dir).unwrap();
        assert!(report.validation.valid, "{:?}", report.validation.errors);
        assert_eq!(report.tags.len(), 2);
        let session = report.tags.iter().find(|tag| tag.tag == "session").unwrap();
        assert_eq!(session.title.as_deref(), Some("Session"));
        assert_eq!(
            session.summary.as_deref(),
            Some("Interactive document session.")
        );
        let review = report.tags.iter().find(|tag| tag.tag == "review").unwrap();
        assert_eq!(review.domain.as_deref(), Some("agent-doc"));
        assert_eq!(review.summary.as_deref(), Some("Review task."));
        let ontology = load_dir(&tags_dir).unwrap();
        let refs = ontology.references_for_tags(&["session".to_string(), "missing".to_string()]);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].tag, "session");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_flags_closed_config_unknown_tags() {
        let dir = std::env::temp_dir().join("tagpath_test_ontology_closed");
        let _ = std::fs::remove_dir_all(&dir);
        let tags_dir = dir.join(".naming").join("tags");
        std::fs::create_dir_all(&tags_dir).unwrap();
        std::fs::write(tags_dir.join("extra.md"), "Extra tag.\n").unwrap();
        let ontology = load_dir(&tags_dir).unwrap();
        let config: crate::config::NamingConfig = toml::from_str(
            r#"version = 1
name = "closed"

[tags]
open = false
"#,
        )
        .unwrap();
        let validation = validate(&ontology, Some(&config));
        assert!(!validation.valid);
        assert_eq!(validation.errors.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_frontmatter_rejects_unclosed_block() {
        let path = Path::new("bad.md");
        let error = parse_frontmatter("+++\ntitle = \"Bad\"\n", path).unwrap_err();
        assert!(error.contains("unterminated"));
    }
}

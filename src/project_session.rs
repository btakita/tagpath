//! Feature-gated lazily-rs prototype for project-scoped derived state.
//!
//! `ProjectSession` deliberately lives in the root `tagpath` facade crate,
//! not in `tagpath-core`: it models filesystem, index, ontology, lint, and
//! MCP/watch-facing state that the pure core crate should not own.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::UNIX_EPOCH;

use lazily::{CellHandle, Context, SlotHandle};
use serde::Serialize;

use crate::{config, extract, index, lint, ontology, parser};

#[derive(Debug, Clone)]
pub struct ConfigState {
    pub config_path: Option<PathBuf>,
    pub config: Option<Rc<config::NamingConfig>>,
    pub error: Option<String>,
    pub fingerprint: Option<String>,
}

impl PartialEq for ConfigState {
    fn eq(&self, other: &Self) -> bool {
        self.config_path == other.config_path
            && self.error == other.error
            && self.fingerprint == other.fingerprint
    }
}

impl Eq for ConfigState {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceListState {
    pub files: Rc<Vec<PathBuf>>,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct OntologyState {
    pub ontology_dir: PathBuf,
    pub report: Option<Rc<ontology::TagOntologyReport>>,
    pub error: Option<String>,
    pub fingerprint: Option<String>,
}

impl PartialEq for OntologyState {
    fn eq(&self, other: &Self) -> bool {
        self.ontology_dir == other.ontology_dir
            && self.error == other.error
            && self.fingerprint == other.fingerprint
    }
}

impl Eq for OntologyState {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarState {
    pub path: PathBuf,
    pub exists: bool,
    pub len: u64,
    pub modified_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFamilyMember {
    pub name: String,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub convention: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFamily {
    pub tags: Vec<String>,
    pub ontology_tags: Vec<String>,
    pub members: Vec<ProjectFamilyMember>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectFamilyMap {
    pub families: BTreeMap<String, ProjectFamily>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSearchHit {
    pub family_id: String,
    pub tags: Vec<String>,
    pub member: ProjectFamilyMember,
}

pub struct ProjectSession {
    ctx: Context,
    project_root: CellHandle<PathBuf>,
    config: CellHandle<ConfigState>,
    sources: CellHandle<SourceListState>,
    ontology: CellHandle<OntologyState>,
    sidecar: CellHandle<SidecarState>,
    query: CellHandle<String>,
    extraction: SlotHandle<Vec<extract::ExtractedIdentifier>>,
    family_map: SlotHandle<ProjectFamilyMap>,
    search: SlotHandle<Vec<ProjectSearchHit>>,
    lint_findings: SlotHandle<Result<Vec<lint::LintViolation>, String>>,
}

impl ProjectSession {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let ctx = Context::new();
        let root = project_root.into();
        let config = ctx.cell(ConfigState::load(&root));
        let sources = ctx.cell(SourceListState::load(&root));
        let ontology = ctx.cell(OntologyState::load(&root));
        let sidecar = ctx.cell(SidecarState::load(&root));
        let query = ctx.cell(String::new());
        let project_root = ctx.cell(root);

        let extraction = {
            let project_root_cell = project_root;
            let sources_cell = sources;
            ctx.slot(move |ctx| {
                let root = ctx.get_cell(&project_root_cell);
                let _source_dependency = ctx.get_cell_rc(&sources_cell);
                extract::extract_from_path(&root)
            })
        };

        let family_map = {
            let extraction_slot = extraction;
            let ontology_cell = ontology;
            ctx.slot(move |ctx| {
                let extracted = ctx.get_rc(&extraction_slot);
                let ontology = ctx.get_cell_rc(&ontology_cell);
                ProjectFamilyMap::from_extracted(&extracted, ontology.report.as_deref())
            })
        };

        let search = {
            let query_cell = query;
            let family_map_slot = family_map;
            ctx.slot(move |ctx| {
                let query = ctx.get_cell(&query_cell);
                let families = ctx.get_rc(&family_map_slot);
                search_family_map(&query, &families)
            })
        };

        let lint_findings = {
            let project_root_cell = project_root;
            let config_cell = config;
            let sources_cell = sources;
            ctx.slot(move |ctx| {
                let root = ctx.get_cell(&project_root_cell);
                let config = ctx.get_cell_rc(&config_cell);
                let _source_dependency = ctx.get_cell_rc(&sources_cell);
                match (config.config.as_deref(), config.error.as_ref()) {
                    (Some(config), _) => Ok(lint::lint(&root, config)),
                    (None, Some(error)) => Err(error.clone()),
                    (None, None) => Err("no .naming.toml found".to_string()),
                }
            })
        };

        Self {
            ctx,
            project_root,
            config,
            sources,
            ontology,
            sidecar,
            query,
            extraction,
            family_map,
            search,
            lint_findings,
        }
    }

    pub fn project_root(&self) -> PathBuf {
        self.ctx.get_cell(&self.project_root)
    }

    pub fn set_project_root(&self, project_root: impl Into<PathBuf>) {
        self.ctx.set_cell(&self.project_root, project_root.into());
        self.refresh();
    }

    pub fn refresh(&self) {
        let root = self.project_root();
        self.ctx.batch(|ctx| {
            ctx.set_cell(&self.config, ConfigState::load(&root));
            ctx.set_cell(&self.sources, SourceListState::load(&root));
            ctx.set_cell(&self.ontology, OntologyState::load(&root));
            ctx.set_cell(&self.sidecar, SidecarState::load(&root));
        });
    }

    pub fn set_query(&self, query: impl Into<String>) {
        self.ctx.set_cell(&self.query, query.into());
    }

    pub fn config_state(&self) -> Rc<ConfigState> {
        self.ctx.get_cell_rc(&self.config)
    }

    pub fn source_files(&self) -> Rc<Vec<PathBuf>> {
        self.ctx.get_cell_rc(&self.sources).files.clone()
    }

    pub fn ontology_state(&self) -> Rc<OntologyState> {
        self.ctx.get_cell_rc(&self.ontology)
    }

    pub fn sidecar_state(&self) -> SidecarState {
        self.ctx.get_cell(&self.sidecar)
    }

    pub fn extracted_identifiers(&self) -> Rc<Vec<extract::ExtractedIdentifier>> {
        self.ctx.get_rc(&self.extraction)
    }

    pub fn family_map(&self) -> Rc<ProjectFamilyMap> {
        self.ctx.get_rc(&self.family_map)
    }

    pub fn search_hits(&self) -> Rc<Vec<ProjectSearchHit>> {
        self.ctx.get_rc(&self.search)
    }

    pub fn lint_findings(&self) -> Rc<Result<Vec<lint::LintViolation>, String>> {
        self.ctx.get_rc(&self.lint_findings)
    }
}

impl ConfigState {
    fn load(project_root: &Path) -> Self {
        let config_path = lint::find_config(project_root);
        match config_path.as_ref() {
            Some(path) => match config::resolve(path) {
                Ok(config) => {
                    let fingerprint = Some(fingerprint_serializable(&config));
                    Self {
                        config_path,
                        config: Some(Rc::new(config)),
                        error: None,
                        fingerprint,
                    }
                }
                Err(error) => Self {
                    config_path,
                    config: None,
                    error: Some(error),
                    fingerprint: None,
                },
            },
            None => Self {
                config_path: None,
                config: None,
                error: Some("no .naming.toml found".to_string()),
                fingerprint: None,
            },
        }
    }
}

impl SourceListState {
    fn load(project_root: &Path) -> Self {
        let mut files = extract::list_source_files(project_root);
        files.sort();
        let fingerprint = fingerprint_sources(&files);
        Self {
            files: Rc::new(files),
            fingerprint,
        }
    }
}

impl OntologyState {
    fn load(project_root: &Path) -> Self {
        let ontology_dir = lint::find_config(project_root)
            .and_then(|path| {
                path.parent()
                    .map(|parent| parent.join(".naming").join("tags"))
            })
            .unwrap_or_else(|| project_root.join(".naming").join("tags"));
        match ontology::load_project(project_root) {
            Ok(report) => {
                let fingerprint = Some(fingerprint_serializable(&report));
                Self {
                    ontology_dir,
                    report: Some(Rc::new(report)),
                    error: None,
                    fingerprint,
                }
            }
            Err(error) => Self {
                ontology_dir,
                report: None,
                error: Some(error),
                fingerprint: None,
            },
        }
    }
}

impl SidecarState {
    fn load(project_root: &Path) -> Self {
        let index_path = index::index_path(project_root);
        let path = index::sidecar::sidecar_path_for(&index_path);
        let metadata = std::fs::metadata(&path).ok();
        let modified_secs = metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        Self {
            path,
            exists: metadata.is_some(),
            len: metadata.as_ref().map_or(0, |metadata| metadata.len()),
            modified_secs,
        }
    }
}

impl ProjectFamilyMap {
    fn from_extracted(
        extracted: &[extract::ExtractedIdentifier],
        ontology: Option<&ontology::TagOntologyReport>,
    ) -> Self {
        let ontology_tags: BTreeSet<&str> = ontology
            .map(|report| report.tags.iter().map(|tag| tag.tag.as_str()).collect())
            .unwrap_or_default();
        let mut families: BTreeMap<String, ProjectFamily> = BTreeMap::new();
        for ident in extracted {
            if ident.parsed.tags.is_empty() {
                continue;
            }
            let family_id = ident.parsed.tags.join("_");
            let family = families.entry(family_id).or_insert_with(|| ProjectFamily {
                tags: ident.parsed.tags.clone(),
                ontology_tags: ident
                    .parsed
                    .tags
                    .iter()
                    .filter(|tag| ontology_tags.contains(tag.as_str()))
                    .cloned()
                    .collect(),
                members: Vec::new(),
            });
            family.members.push(ProjectFamilyMember {
                name: ident.identifier.clone(),
                path: ident.file.clone(),
                line: ident.line,
                column: ident.column,
                convention: ident.parsed.convention.to_string(),
            });
        }
        for family in families.values_mut() {
            family.members.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then(a.line.cmp(&b.line))
                    .then(a.column.cmp(&b.column))
                    .then(a.name.cmp(&b.name))
            });
            family.members.dedup();
        }
        Self { families }
    }
}

fn search_family_map(query: &str, family_map: &ProjectFamilyMap) -> Vec<ProjectSearchHit> {
    let query_convention = parser::detect_convention(query);
    let query_tags = parser::parse(query, query_convention).tags;
    if query_tags.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (family_id, family) in &family_map.families {
        if !query_tags
            .iter()
            .all(|query_tag| family.tags.iter().any(|tag| tag == query_tag))
        {
            continue;
        }
        for member in &family.members {
            hits.push(ProjectSearchHit {
                family_id: family_id.clone(),
                tags: family.tags.clone(),
                member: member.clone(),
            });
        }
    }
    hits
}

fn fingerprint_serializable<T: Serialize>(value: &T) -> String {
    let encoded = serde_json::to_string(value).unwrap_or_else(|error| error.to_string());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    encoded.hash(&mut hasher);
    format!("hash:{:016x}", hasher.finish())
}

fn fingerprint_sources(files: &[PathBuf]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in files {
        file.hash(&mut hasher);
        if let Ok(metadata) = std::fs::metadata(file) {
            metadata.len().hash(&mut hasher);
            if let Some(duration) = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            {
                duration.as_nanos().hash(&mut hasher);
            }
        }
    }
    format!("hash:{:016x}", hasher.finish())
}

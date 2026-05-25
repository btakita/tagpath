use crate::index::{self, BuildOptions, Family, Index, IndexShape};
use crate::parser::{self, Convention};
use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct RenameOptions {
    pub path: PathBuf,
    pub old: String,
    pub new: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenameReport {
    pub project_root: String,
    pub old: String,
    pub new: String,
    pub old_family_id: String,
    pub new_family_id: String,
    pub dry_run: bool,
    pub files_changed: usize,
    pub replacements: usize,
    pub edits: Vec<RenameEdit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenameEdit {
    pub path: String,
    pub old: String,
    pub new: String,
    pub replacements: usize,
}

pub fn rename_family(opts: &RenameOptions) -> Result<RenameReport, String> {
    let project_root = index::find_project_root(&opts.path).ok_or_else(|| {
        format!(
            "no .naming.toml found (searched from {} upward); run `tagpath init`",
            opts.path.display()
        )
    })?;
    let idx = load_index(&project_root, !opts.dry_run)?;
    let (old_family_id, _) = family_id_for(&opts.old)?;
    let (new_family_id, new_tags) = family_id_for(&opts.new)?;
    let family = find_family(&idx, &old_family_id, &opts.old).ok_or_else(|| {
        format!(
            "no indexed family matches `{}`; run `tagpath index` and try again",
            opts.old
        )
    })?;
    let replacements_by_path = replacement_plan(family, &new_tags)?;
    let mut edits = Vec::new();

    for (rel_path, replacements) in replacements_by_path {
        let abs_path = project_root.join(&rel_path);
        let original = std::fs::read_to_string(&abs_path)
            .map_err(|e| format!("read({}): {e}", abs_path.display()))?;
        let mut content = original.clone();
        let mut pairs: Vec<(String, String)> = replacements.into_iter().collect();
        pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

        for (old_name, new_name) in pairs {
            let (next, count) = replace_identifier(&content, &old_name, &new_name)?;
            if count > 0 {
                content = next;
                edits.push(RenameEdit {
                    path: rel_path.clone(),
                    old: old_name,
                    new: new_name,
                    replacements: count,
                });
            }
        }

        if content != original && !opts.dry_run {
            std::fs::write(&abs_path, content)
                .map_err(|e| format!("write({}): {e}", abs_path.display()))?;
        }
    }

    if !opts.dry_run && !edits.is_empty() {
        let refreshed = index::build(&BuildOptions {
            project_root: project_root.clone(),
        })?;
        index::write(&refreshed, &index::index_path(&project_root))?;
    }

    let replacements = edits.iter().map(|edit| edit.replacements).sum();
    let files_changed = edits
        .iter()
        .map(|edit| edit.path.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    Ok(RenameReport {
        project_root: project_root.display().to_string(),
        old: opts.old.clone(),
        new: opts.new.clone(),
        old_family_id,
        new_family_id,
        dry_run: opts.dry_run,
        files_changed,
        replacements,
        edits,
    })
}

fn load_index(project_root: &Path, write_refresh: bool) -> Result<Index, String> {
    let idx_path = index::index_path(project_root);
    let report = index::check(project_root)?;
    if report.fresh {
        return index::read(&idx_path);
    }

    let opts = BuildOptions {
        project_root: project_root.to_path_buf(),
    };
    if write_refresh {
        let result = index::update_incremental_with(&opts, IndexShape::Full)?;
        index::write(&result.index, &idx_path)?;
        Ok(result.index)
    } else {
        index::build(&opts)
    }
}

fn family_id_for(name: &str) -> Result<(String, Vec<String>), String> {
    let convention = parser::detect_convention(name);
    let parsed = parser::parse(name, convention);
    if parsed.tags.is_empty() {
        return Err(format!("`{name}` does not contain any tags"));
    }
    Ok((parsed.tags.join("_"), parsed.tags))
}

fn find_family<'a>(idx: &'a Index, family_id: &str, old: &str) -> Option<&'a Family> {
    idx.families.iter().find(|family| {
        family.family_id == family_id
            || family.handle == old
            || family.members.iter().any(|member| member.name == old)
    })
}

fn replacement_plan(
    family: &Family,
    new_tags: &[String],
) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let mut by_path: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for member in &family.members {
        let convention = Convention::from_str(&member.convention)
            .map_err(|_| format!("unknown convention `{}` in index", member.convention))?;
        let new_name = parser::join_tags(new_tags, convention);
        if member.name != new_name {
            by_path
                .entry(member.path.clone())
                .or_default()
                .insert(member.name.clone(), new_name);
        }
    }
    Ok(by_path)
}

fn replace_identifier(content: &str, old: &str, new: &str) -> Result<(String, usize), String> {
    let re = Regex::new(&format!(r"(^|[^A-Za-z0-9_$-]){}", regex::escape(old)))
        .map_err(|e| format!("compile rename regex for `{old}`: {e}"))?;
    let mut count = 0usize;
    let replaced = re.replace_all(content, |caps: &regex::Captures<'_>| {
        let matched = caps.get(0).expect("whole match");
        if content[matched.end()..]
            .chars()
            .next()
            .is_some_and(is_identifier_char)
        {
            return matched.as_str().to_string();
        }
        count += 1;
        let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        format!("{prefix}{new}")
    });
    Ok((replaced.into_owned(), count))
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '-')
}

#[cfg(test)]
mod tests {
    use super::replace_identifier;

    #[test]
    fn replace_identifier_respects_token_boundaries() {
        let (renamed, count) = replace_identifier(
            "create_user create_user_profile xcreate_user",
            "create_user",
            "make_user",
        )
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(renamed, "make_user create_user_profile xcreate_user");
    }
}

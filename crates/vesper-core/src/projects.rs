//! Bounded, deterministic project routing. Observed document paths are transient.
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMode {
    #[default]
    Manual,
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRule {
    pub id: String,
    pub application_id: String,
    pub application_name: String,
    pub project: String,
    /// User-selected root, never an observed document path.
    pub folder: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSource {
    Manual,
    Application,
    Folder,
    #[default]
    Shared,
    ContextUnavailable,
    Ambiguous,
    /// An automatically detected, visible workspace (folder or web host).
    Workspace,
    /// No workspace: the app category is known, vocabulary scope stays shared.
    Category,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSelection {
    pub project: String,
    pub source: ProjectSource,
    pub rule_id: Option<String>,
}

fn plain(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= limit
        && !value.chars().any(char::is_control)
        && !value.contains("<|")
}

fn absolute_path(value: &str) -> bool {
    plain(value, 4096)
        && Path::new(value).is_absolute()
        && !Path::new(value)
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
}

pub fn validate_project_rules(rules: &[ProjectRule]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    if rules.len() > 32 {
        return Err("Use at most 32 project rules.".into());
    }
    for rule in rules {
        if !plain(&rule.id, 64)
            || !ids.insert(&rule.id)
            || !plain(&rule.application_id, 256)
            || !plain(&rule.application_name, 256)
            || !plain(&rule.project, 64)
            || rule
                .folder
                .as_ref()
                .is_some_and(|folder| !absolute_path(folder))
        {
            return Err("Project rules need unique IDs, an application, a project name and an optional absolute folder.".into());
        }
    }
    Ok(())
}

/// Manual scope wins. Otherwise prefer a unique most-specific folder, then a
/// unique application rule. Conflicts and unavailable required context abstain.
/// Call once before showing a capture UI; never resolve again during feedback.
pub fn resolve_project(
    mode: ProjectMode,
    manual: &str,
    rules: &[ProjectRule],
    application_id: Option<&str>,
    document_path: Option<&str>,
) -> ProjectSelection {
    let fallback = |source| ProjectSelection {
        source,
        ..Default::default()
    };
    if mode == ProjectMode::Manual || !manual.trim().is_empty() {
        return ProjectSelection {
            project: manual.trim().into(),
            source: if manual.trim().is_empty() {
                ProjectSource::Shared
            } else {
                ProjectSource::Manual
            },
            rule_id: None,
        };
    }
    if validate_project_rules(rules).is_err() {
        return fallback(ProjectSource::Ambiguous);
    }
    let Some(application) = application_id.filter(|id| plain(id, 256)) else {
        return fallback(ProjectSource::ContextUnavailable);
    };
    let matching: Vec<_> = rules
        .iter()
        .filter(|r| r.enabled && r.application_id == application)
        .collect();
    let document = document_path
        .filter(|path| absolute_path(path))
        .map(Path::new);
    if document.is_none() && matching.iter().any(|r| r.folder.is_some()) {
        return fallback(ProjectSource::ContextUnavailable);
    }
    let mut candidates: Vec<_> = matching
        .iter()
        .copied()
        .filter(|r| {
            r.folder
                .as_deref()
                .is_some_and(|folder| document.is_some_and(|path| path.starts_with(folder)))
        })
        .collect();
    let source = if candidates.is_empty() {
        candidates = matching
            .into_iter()
            .filter(|r| r.folder.is_none())
            .collect();
        ProjectSource::Application
    } else {
        let depth = candidates
            .iter()
            .filter_map(|r| r.folder.as_ref())
            .map(|p| Path::new(p).components().count())
            .max()
            .unwrap_or(0);
        candidates.retain(|r| {
            r.folder
                .as_ref()
                .is_some_and(|p| Path::new(p).components().count() == depth)
        });
        ProjectSource::Folder
    };
    match candidates.as_slice() {
        [] => fallback(ProjectSource::Shared),
        [rule] => ProjectSelection {
            project: rule.project.trim().into(),
            source,
            rule_id: Some(rule.id.clone()),
        },
        _ => fallback(ProjectSource::Ambiguous),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rule(id: &str, folder: Option<&str>) -> ProjectRule {
        ProjectRule {
            id: id.into(),
            application_id: "test.editor".into(),
            application_name: "Editor".into(),
            project: id.into(),
            folder: folder.map(String::from),
            enabled: true,
        }
    }
    fn resolve(rules: &[ProjectRule], path: Option<&str>) -> ProjectSelection {
        resolve_project(ProjectMode::Automatic, "", rules, Some("test.editor"), path)
    }
    #[test]
    fn manual_and_legacy_profiles_keep_explicit_scope() {
        let profile: crate::PersonalizationProfile =
            serde_json::from_str(r#"{"schemaVersion":3,"activeProject":"Old"}"#).unwrap();
        assert_eq!(profile.project_mode, ProjectMode::Manual);
        assert_eq!(
            resolve_project(
                ProjectMode::Automatic,
                " override ",
                &[rule("auto", None)],
                Some("test.editor"),
                None
            )
            .project,
            "override"
        );
    }
    #[test]
    fn folders_use_components_and_most_specific_root() {
        let rules = [
            rule("app", None),
            rule("parent", Some("/work/project")),
            rule("child", Some("/work/project/nested")),
        ];
        assert_eq!(
            resolve(&rules, Some("/work/project/file.rs")).project,
            "parent"
        );
        assert_eq!(
            resolve(&rules, Some("/work/project/nested/file.rs")).project,
            "child"
        );
        assert_eq!(
            resolve(&rules, Some("/work/project-other/file.rs")).project,
            "app"
        );
        assert_eq!(
            resolve(&rules, Some("/work/project/../other/file.rs")).source,
            ProjectSource::ContextUnavailable
        );
    }
    #[test]
    fn ambiguous_or_unavailable_context_never_guesses() {
        assert_eq!(
            resolve(&[rule("a", None), rule("b", None)], None).source,
            ProjectSource::Ambiguous
        );
        assert_eq!(
            resolve(&[rule("a", None), rule("b", Some("/work"))], None).source,
            ProjectSource::ContextUnavailable
        );
        assert_eq!(
            resolve(
                &[rule("a", Some("/work")), rule("b", Some("/work"))],
                Some("/work/file")
            )
            .source,
            ProjectSource::Ambiguous
        );
        assert_eq!(
            resolve_project(
                ProjectMode::Automatic,
                "",
                &[rule("a", None)],
                Some("different.app"),
                None
            )
            .source,
            ProjectSource::Shared
        );
    }
    #[test]
    fn bounds_and_disabled_rules_are_enforced() {
        let mut disabled = rule("a", None);
        disabled.enabled = false;
        assert_eq!(resolve(&[disabled], None).source, ProjectSource::Shared);
        assert!(validate_project_rules(&[rule("a", Some("relative"))]).is_err());
        assert!(validate_project_rules(&[rule("a", None), rule("a", None)]).is_err());
    }
}

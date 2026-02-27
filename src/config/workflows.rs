//! Workflow text override configuration.
//!
//! Allows users to override workflow step instruction text via config.yaml,
//! mirroring how `prompts:` works for LLM prompts.
//!
//! ## Config format
//!
//! ```yaml
//! workflows:
//!   improve.cleanup: |
//!     Custom cleanup instructions...
//!     {doc_hint}
//!     {ctx}
//! ```
//!
//! Flat key format: `workflow_name.step_name` (e.g. `improve.cleanup`, `update.scan`).

use std::collections::{HashMap, HashSet};
use tracing::warn;

use super::prompts::extract_placeholders;

use serde::{Deserialize, Serialize};

/// Workflow text overrides. Keys are `workflow.step` (e.g. `improve.cleanup`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowsConfig {
    #[serde(flatten)]
    pub templates: HashMap<String, String>,
}

/// Valid workflow keys and their allowed placeholder variables.
///
/// ## Workflow keys
///
/// | Key | Placeholders |
/// |-----|-------------|
/// | `setup.init` | `path` |
/// | `setup.perspective` | `path` |
/// | `setup.validate_ok` | `detail` |
/// | `setup.validate_error` | `detail` |
/// | `setup.create` | `format_rules` |
/// | `setup.scan` | _(none)_ |
/// | `setup.complete` | _(none)_ |
/// | `update.scan` | `ctx` |
/// | `update.check` | _(none)_ |
/// | `update.organize` | _(none)_ |
/// | `update.summary` | _(none)_ |
/// | `resolve.queue` | `ctx`, `deferred_note` |
/// | `resolve.answer` | `stale`, `ctx` |
/// | `resolve.answer_intro` | `ctx` |
/// | `resolve.apply` | _(none)_ |
/// | `resolve.verify` | _(none)_ |
/// | `ingest.search` | `topic`, `ctx` |
/// | `ingest.research` | `topic`, `ctx` |
/// | `ingest.create` | `fields`, `format_rules` |
/// | `ingest.verify` | _(none)_ |
/// | `enrich.review` | `ctx` |
/// | `enrich.gaps` | `fields` |
/// | `enrich.research` | `ctx`, `format_rules` |
/// | `enrich.verify` | _(none)_ |
/// | `improve.cleanup` | `doc_hint`, `ctx` |
/// | `improve.resolve` | `doc_hint`, `stale`, `ctx` |
/// | `improve.enrich` | `doc_hint`, `fields`, `ctx` |
/// | `improve.check` | `doc_hint`, `compare_note` |
pub fn known_workflows() -> HashMap<&'static str, &'static [&'static str]> {
    HashMap::from([
        ("setup.init", &["path"] as &[&str]),
        ("setup.perspective", &["path"]),
        ("setup.validate_ok", &["detail"]),
        ("setup.validate_error", &["detail"]),
        ("setup.create", &["format_rules"]),
        ("setup.scan", &[] as &[&str]),
        ("setup.complete", &[] as &[&str]),
        ("update.scan", &["ctx"]),
        ("update.check", &[] as &[&str]),
        ("update.organize", &[] as &[&str]),
        ("update.summary", &[] as &[&str]),
        ("resolve.queue", &["ctx", "deferred_note"]),
        ("resolve.answer", &["stale", "ctx"]),
        ("resolve.answer_intro", &["ctx"]),
        ("resolve.apply", &[] as &[&str]),
        ("resolve.verify", &[] as &[&str]),
        ("ingest.search", &["topic", "ctx"]),
        ("ingest.research", &["topic", "ctx"]),
        ("ingest.create", &["fields", "format_rules"]),
        ("ingest.verify", &[] as &[&str]),
        ("enrich.review", &["ctx"]),
        ("enrich.gaps", &["fields"]),
        ("enrich.research", &["ctx", "format_rules"]),
        ("enrich.verify", &[] as &[&str]),
        ("improve.cleanup", &["doc_hint", "ctx"]),
        ("improve.resolve", &["doc_hint", "stale", "ctx"]),
        ("improve.enrich", &["doc_hint", "fields", "ctx"]),
        ("improve.check", &["doc_hint", "compare_note"]),
    ])
}

/// Validate workflow overrides: warn on unknown keys or placeholders.
pub fn validate_workflows(config: &WorkflowsConfig) {
    let known = known_workflows();
    for (key, template) in &config.templates {
        if let Some(allowed_vars) = known.get(key.as_str()) {
            let allowed: HashSet<&str> = allowed_vars.iter().copied().collect();
            for var in extract_placeholders(template) {
                if !allowed.contains(var.as_str()) {
                    warn!(
                        "Workflow '{}' references unknown placeholder '{{{}}}'. Known: {:?}",
                        key, var, allowed_vars
                    );
                }
            }
        } else {
            warn!(
                "Unknown workflow key '{}' in config. Known keys: {:?}",
                key,
                known.keys().collect::<Vec<_>>()
            );
        }
    }
}

/// Resolve a workflow instruction: use config override if present, else default.
/// Substitutes `{placeholder}` variables in the template.
pub fn resolve_workflow_text(
    config: &WorkflowsConfig,
    key: &str,
    default: &str,
    vars: &[(&str, &str)],
) -> String {
    let template = config
        .templates
        .get(key)
        .map(|s| s.as_str())
        .unwrap_or(default);
    let mut result = template.to_string();
    for (name, value) in vars {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_workflow_text_uses_default() {
        let config = WorkflowsConfig::default();
        let result = resolve_workflow_text(&config, "improve.cleanup", "Default {ctx}", &[("ctx", "here")]);
        assert_eq!(result, "Default here");
    }

    #[test]
    fn test_resolve_workflow_text_uses_override() {
        let mut config = WorkflowsConfig::default();
        config.templates.insert("improve.cleanup".into(), "Custom: {ctx}".into());
        let result = resolve_workflow_text(&config, "improve.cleanup", "Default {ctx}", &[("ctx", "here")]);
        assert_eq!(result, "Custom: here");
    }

    #[test]
    fn test_resolve_workflow_text_fallback_no_vars() {
        let config = WorkflowsConfig::default();
        let result = resolve_workflow_text(&config, "setup.scan", "Static text", &[]);
        assert_eq!(result, "Static text");
    }

    #[test]
    fn test_validate_workflows_warns_unknown_key() {
        let mut config = WorkflowsConfig::default();
        config.templates.insert("nonexistent.step".into(), "template".into());
        validate_workflows(&config); // should not panic, just warn
    }

    #[test]
    fn test_validate_workflows_warns_unknown_placeholder() {
        let mut config = WorkflowsConfig::default();
        config.templates.insert("improve.cleanup".into(), "{doc_hint} {unknown_var}".into());
        validate_workflows(&config); // should not panic, just warn
    }

    #[test]
    fn test_validate_workflows_ok_for_valid() {
        let mut config = WorkflowsConfig::default();
        config.templates.insert("improve.cleanup".into(), "Fix {doc_hint} with {ctx}".into());
        validate_workflows(&config); // no warnings
    }

    #[test]
    fn test_known_workflows_count() {
        let known = known_workflows();
        assert_eq!(known.len(), 28);
    }

    #[test]
    fn test_workflows_config_deserialize_from_yaml() {
        let yaml = r#"
improve.cleanup: "Custom cleanup for {doc_hint} with {ctx}"
update.scan: "Custom scan with {ctx}"
"#;
        let config: WorkflowsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates.len(), 2);
        assert!(config.templates.contains_key("improve.cleanup"));
        assert!(config.templates.contains_key("update.scan"));
    }

    #[test]
    fn test_workflows_config_in_full_config() {
        let yaml = r#"
database:
  pool_size: 4
workflows:
  improve.enrich: "Custom enrich {doc_hint}"
  update.scan: "Custom scan {ctx}"
"#;
        let config: crate::Config = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.workflows.templates.len(), 2);
        let result = resolve_workflow_text(
            &config.workflows,
            "improve.enrich",
            "default",
            &[("doc_hint", " for doc1")],
        );
        assert_eq!(result, "Custom enrich  for doc1");
    }
}

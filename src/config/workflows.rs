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
//! Flat key format: `workflow_name.step_name` (e.g. `improve.cleanup`, `maintain.scan`).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::warn;

use super::prompts::extract_placeholders;

use serde::{Deserialize, Serialize};

/// Workflow text overrides. Keys are `workflow.step` (e.g. `improve.cleanup`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowsConfig {
    /// Default resolve variant: "default", "type_evidence", or "research_batch".
    #[serde(default)]
    pub resolve_variant: Option<String>,

    /// Batch size for resolve step 2 (default 50, clamped to 10..=100).
    #[serde(default)]
    pub resolve_batch_size: Option<usize>,

    #[serde(flatten)]
    pub templates: HashMap<String, String>,
}

impl WorkflowsConfig {
    /// Effective resolve batch size: configured value clamped to 10..=100, or 50.
    pub fn resolve_batch_size(&self) -> usize {
        self.resolve_batch_size.unwrap_or(30).clamp(10, 100)
    }

    /// Merge another config on top of this one (other wins on conflicts).
    pub fn merge(&mut self, other: &WorkflowsConfig) {
        if other.resolve_variant.is_some() {
            self.resolve_variant = other.resolve_variant.clone();
        }
        if other.resolve_batch_size.is_some() {
            self.resolve_batch_size = other.resolve_batch_size;
        }
        for (k, v) in &other.templates {
            self.templates.insert(k.clone(), v.clone());
        }
    }

    /// Load per-repo workflow prompts from `.factbase/prompts.yaml`.
    /// Returns `None` if the file doesn't exist or can't be parsed.
    pub fn load_repo_prompts(repo_path: &Path) -> Option<WorkflowsConfig> {
        let path = repo_path.join(".factbase").join("prompts.yaml");
        let content = std::fs::read_to_string(&path).ok()?;
        match serde_yaml_ng::from_str::<WorkflowsConfig>(&content) {
            Ok(config) => Some(config),
            Err(e) => {
                warn!("Failed to parse {}: {}", path.display(), e);
                None
            }
        }
    }

    /// Load workflow instruction overrides from `.factbase/instructions/*.toml`.
    ///
    /// Each TOML file is named after a workflow (e.g., `maintain.toml`) and contains
    /// step names as keys with instruction text as values:
    ///
    /// ```toml
    /// scan = """Custom scan instruction..."""
    /// check = """Custom check instruction..."""
    /// ```
    ///
    /// The filename becomes the workflow prefix: `maintain.toml` with key `scan`
    /// produces template key `maintain.scan`.
    pub fn load_instruction_files(repo_path: &Path) -> Option<WorkflowsConfig> {
        let dir = repo_path.join(".factbase").join("instructions");
        let entries = std::fs::read_dir(&dir).ok()?;
        let mut config = WorkflowsConfig::default();
        let mut found_any = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let workflow_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if workflow_name.is_empty() {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read {}: {}", path.display(), e);
                    continue;
                }
            };
            let table: toml::Table = match content.parse() {
                Ok(t) => t,
                Err(e) => {
                    warn!("Failed to parse TOML {}: {}", path.display(), e);
                    continue;
                }
            };
            for (step_name, value) in &table {
                if let Some(text) = value.as_str() {
                    config
                        .templates
                        .insert(format!("{workflow_name}.{step_name}"), text.to_string());
                    found_any = true;
                }
            }
        }
        if found_any {
            Some(config)
        } else {
            None
        }
    }
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
/// | `setup.scan` | `path` |
/// | `setup.complete` | _(none)_ |
/// | `resolve.queue` | `ctx`, `deferred_note` |
/// | `resolve.answer` | `stale`, `ctx` |
/// | `resolve.answer_intro` | `stale`, `ctx` |
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
        ("setup.scan", &["path"] as &[&str]),
        ("setup.complete", &[] as &[&str]),
        ("resolve.queue", &["ctx", "deferred_note"]),
        ("resolve.answer", &["stale", "ctx"]),
        ("resolve.answer_intro", &["stale", "ctx"]),
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
        ("correct.parse", &["correction", "source_note"]),
        ("correct.search", &["correction"]),
        (
            "correct.fix",
            &["correction", "source_note", "source_footnote", "today"],
        ),
        ("correct.cleanup", &["correction", "source"]),
        ("transition.parse", &["change", "source_note", "today"]),
        ("transition.nomenclature_question", &["change"]),
        ("transition.search", &["nomenclature", "change"]),
        (
            "transition.apply",
            &[
                "change",
                "nomenclature",
                "effective_date",
                "source_note",
                "source_footnote",
            ],
        ),
        ("transition.organize", &[] as &[&str]),
        ("transition.maintain", &[] as &[&str]),
        (
            "transition.report",
            &["change", "effective_date", "source", "nomenclature"],
        ),
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
        let result = resolve_workflow_text(
            &config,
            "improve.cleanup",
            "Default {ctx}",
            &[("ctx", "here")],
        );
        assert_eq!(result, "Default here");
    }

    #[test]
    fn test_resolve_workflow_text_uses_override() {
        let mut config = WorkflowsConfig::default();
        config
            .templates
            .insert("improve.cleanup".into(), "Custom: {ctx}".into());
        let result = resolve_workflow_text(
            &config,
            "improve.cleanup",
            "Default {ctx}",
            &[("ctx", "here")],
        );
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
        config
            .templates
            .insert("nonexistent.step".into(), "template".into());
        validate_workflows(&config); // should not panic, just warn
    }

    #[test]
    fn test_validate_workflows_warns_unknown_placeholder() {
        let mut config = WorkflowsConfig::default();
        config
            .templates
            .insert("improve.cleanup".into(), "{doc_hint} {unknown_var}".into());
        validate_workflows(&config); // should not panic, just warn
    }

    #[test]
    fn test_validate_workflows_ok_for_valid() {
        let mut config = WorkflowsConfig::default();
        config
            .templates
            .insert("improve.cleanup".into(), "Fix {doc_hint} with {ctx}".into());
        validate_workflows(&config); // no warnings
    }

    #[test]
    fn test_known_workflows_count() {
        let known = known_workflows();
        assert_eq!(known.len(), 35);
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

    #[test]
    fn test_merge_templates_override() {
        let mut base = WorkflowsConfig::default();
        base.templates
            .insert("resolve.answer".into(), "base answer".into());
        base.templates
            .insert("resolve.queue".into(), "base queue".into());

        let mut overlay = WorkflowsConfig::default();
        overlay
            .templates
            .insert("resolve.answer".into(), "custom answer".into());

        base.merge(&overlay);
        assert_eq!(base.templates["resolve.answer"], "custom answer");
        assert_eq!(base.templates["resolve.queue"], "base queue");
    }

    #[test]
    fn test_merge_resolve_variant() {
        let mut base = WorkflowsConfig::default();
        assert!(base.resolve_variant.is_none());

        let mut overlay = WorkflowsConfig::default();
        overlay.resolve_variant = Some("type_evidence".into());

        base.merge(&overlay);
        assert_eq!(base.resolve_variant.as_deref(), Some("type_evidence"));
    }

    #[test]
    fn test_merge_resolve_variant_not_overwritten_by_none() {
        let mut base = WorkflowsConfig::default();
        base.resolve_variant = Some("research_batch".into());

        let overlay = WorkflowsConfig::default(); // resolve_variant is None

        base.merge(&overlay);
        assert_eq!(base.resolve_variant.as_deref(), Some("research_batch"));
    }

    #[test]
    fn test_resolve_variant_deserialize() {
        let yaml = r#"
resolve_variant: type_evidence
resolve.answer: "Custom answer {ctx}"
"#;
        let config: WorkflowsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.resolve_variant.as_deref(), Some("type_evidence"));
        assert_eq!(config.templates.len(), 1);
        assert!(config.templates.contains_key("resolve.answer"));
    }

    #[test]
    fn test_resolve_variant_in_full_config() {
        let yaml = r#"
workflows:
  resolve_variant: research_batch
  resolve.answer: "Custom {ctx}"
"#;
        let config: crate::Config = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(
            config.workflows.resolve_variant.as_deref(),
            Some("research_batch")
        );
        assert_eq!(config.workflows.templates.len(), 1);
    }

    #[test]
    fn test_load_repo_prompts_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(WorkflowsConfig::load_repo_prompts(dir.path()).is_none());
    }

    #[test]
    fn test_load_repo_prompts_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let factbase_dir = dir.path().join(".factbase");
        std::fs::create_dir_all(&factbase_dir).unwrap();
        std::fs::write(
            factbase_dir.join("prompts.yaml"),
            "resolve_variant: type_evidence\nresolve.answer: \"Custom answer\"\n",
        )
        .unwrap();
        let config = WorkflowsConfig::load_repo_prompts(dir.path()).unwrap();
        assert_eq!(config.resolve_variant.as_deref(), Some("type_evidence"));
        assert_eq!(config.templates["resolve.answer"], "Custom answer");
    }

    #[test]
    fn test_load_repo_prompts_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let factbase_dir = dir.path().join(".factbase");
        std::fs::create_dir_all(&factbase_dir).unwrap();
        std::fs::write(factbase_dir.join("prompts.yaml"), "{{invalid yaml").unwrap();
        assert!(WorkflowsConfig::load_repo_prompts(dir.path()).is_none());
    }

    #[test]
    fn test_resolve_batch_size_default() {
        let config = WorkflowsConfig::default();
        assert_eq!(config.resolve_batch_size(), 30);
    }

    #[test]
    fn test_resolve_batch_size_configured() {
        let mut config = WorkflowsConfig::default();
        config.resolve_batch_size = Some(25);
        assert_eq!(config.resolve_batch_size(), 25);
    }

    #[test]
    fn test_resolve_batch_size_clamped_low() {
        let mut config = WorkflowsConfig::default();
        config.resolve_batch_size = Some(3);
        assert_eq!(config.resolve_batch_size(), 10);
    }

    #[test]
    fn test_resolve_batch_size_clamped_high() {
        let mut config = WorkflowsConfig::default();
        config.resolve_batch_size = Some(200);
        assert_eq!(config.resolve_batch_size(), 100);
    }

    #[test]
    fn test_resolve_batch_size_deserialize() {
        let yaml = "resolve_batch_size: 30\n";
        let config: WorkflowsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.resolve_batch_size(), 30);
    }

    #[test]
    fn test_resolve_batch_size_in_full_config() {
        let yaml = "workflows:\n  resolve_batch_size: 40\n";
        let config: crate::Config = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.workflows.resolve_batch_size(), 40);
    }

    #[test]
    fn test_merge_resolve_batch_size() {
        let mut base = WorkflowsConfig::default();
        base.resolve_batch_size = Some(50);
        let mut overlay = WorkflowsConfig::default();
        overlay.resolve_batch_size = Some(25);
        base.merge(&overlay);
        assert_eq!(base.resolve_batch_size(), 25);
    }

    #[test]
    fn test_merge_resolve_batch_size_not_overwritten_by_none() {
        let mut base = WorkflowsConfig::default();
        base.resolve_batch_size = Some(30);
        let overlay = WorkflowsConfig::default();
        base.merge(&overlay);
        assert_eq!(base.resolve_batch_size(), 30);
    }

    // --- load_instruction_files ---

    #[test]
    fn test_load_instruction_files_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(WorkflowsConfig::load_instruction_files(dir.path()).is_none());
    }

    #[test]
    fn test_load_instruction_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".factbase/instructions")).unwrap();
        assert!(WorkflowsConfig::load_instruction_files(dir.path()).is_none());
    }

    #[test]
    fn test_load_instruction_files_valid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(
            instr_dir.join("maintain.toml"),
            r#"scan = "Custom scan instruction"
check = "Custom check {ctx}"
"#,
        )
        .unwrap();
        let config = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        assert_eq!(config.templates["maintain.scan"], "Custom scan instruction");
        assert_eq!(config.templates["maintain.check"], "Custom check {ctx}");
    }

    #[test]
    fn test_load_instruction_files_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(instr_dir.join("maintain.toml"), "scan = \"A\"").unwrap();
        std::fs::write(instr_dir.join("resolve.toml"), "queue = \"B\"").unwrap();
        let config = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        assert_eq!(config.templates["maintain.scan"], "A");
        assert_eq!(config.templates["resolve.queue"], "B");
    }

    #[test]
    fn test_load_instruction_files_skips_non_toml() {
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(instr_dir.join("maintain.toml"), "scan = \"A\"").unwrap();
        std::fs::write(instr_dir.join("notes.txt"), "not a toml file").unwrap();
        let config = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        assert_eq!(config.templates.len(), 1);
    }

    #[test]
    fn test_load_instruction_files_invalid_toml_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(instr_dir.join("bad.toml"), "{{invalid toml").unwrap();
        std::fs::write(instr_dir.join("good.toml"), "scan = \"OK\"").unwrap();
        let config = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        assert_eq!(config.templates.len(), 1);
        assert_eq!(config.templates["good.scan"], "OK");
    }

    #[test]
    fn test_load_instruction_files_multiline_toml() {
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(
            instr_dir.join("improve.toml"),
            "cleanup = \"\"\"\nLine 1\nLine 2\nLine 3\n\"\"\"",
        )
        .unwrap();
        let config = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        assert!(config.templates["improve.cleanup"].contains("Line 1"));
        assert!(config.templates["improve.cleanup"].contains("Line 3"));
    }

    #[test]
    fn test_instruction_files_override_priority() {
        // TOML files should be overridden by config.yaml templates
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(instr_dir.join("maintain.toml"), "scan = \"from toml\"").unwrap();

        let mut wf_config = WorkflowsConfig::default();
        // Merge TOML (lower priority)
        let toml_overrides = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        wf_config.merge(&toml_overrides);
        // Merge config.yaml (higher priority)
        let mut yaml_config = WorkflowsConfig::default();
        yaml_config
            .templates
            .insert("maintain.scan".into(), "from yaml".into());
        wf_config.merge(&yaml_config);

        assert_eq!(wf_config.templates["maintain.scan"], "from yaml");
    }

    #[test]
    fn test_instruction_files_fallback_to_default() {
        // When no TOML or config override exists, resolve_workflow_text uses the default
        let config = WorkflowsConfig::default();
        let result = resolve_workflow_text(&config, "maintain.scan", "compiled default", &[]);
        assert_eq!(result, "compiled default");
    }

    #[test]
    fn test_instruction_files_toml_overrides_default() {
        // TOML file should override compiled default
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(
            instr_dir.join("maintain.toml"),
            "scan = \"from toml {ctx}\"",
        )
        .unwrap();

        let mut wf_config = WorkflowsConfig::default();
        let toml_overrides = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        wf_config.merge(&toml_overrides);

        let result = resolve_workflow_text(
            &wf_config,
            "maintain.scan",
            "compiled default",
            &[("ctx", "here")],
        );
        assert_eq!(result, "from toml here");
    }

    #[test]
    fn test_instruction_files_missing_step_falls_back() {
        // If TOML only overrides some steps, others fall back to default
        let dir = tempfile::tempdir().unwrap();
        let instr_dir = dir.path().join(".factbase/instructions");
        std::fs::create_dir_all(&instr_dir).unwrap();
        std::fs::write(instr_dir.join("maintain.toml"), "scan = \"custom scan\"").unwrap();

        let mut wf_config = WorkflowsConfig::default();
        let toml_overrides = WorkflowsConfig::load_instruction_files(dir.path()).unwrap();
        wf_config.merge(&toml_overrides);

        // scan is overridden
        let result = resolve_workflow_text(&wf_config, "maintain.scan", "default scan", &[]);
        assert_eq!(result, "custom scan");
        // check falls back to default
        let result = resolve_workflow_text(&wf_config, "maintain.check", "default check", &[]);
        assert_eq!(result, "default check");
    }
}

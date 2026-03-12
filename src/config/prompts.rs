use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::warn;

/// Configurable LLM prompt templates.
///
/// Each key maps to a named prompt. If absent, the hardcoded default is used.
/// Templates use `{placeholder}` syntax for variable substitution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptsConfig {
    #[serde(flatten)]
    pub templates: HashMap<String, String>,
}

/// Known prompt keys and their allowed variables.
pub fn known_prompts() -> HashMap<&'static str, &'static [&'static str]> {
    HashMap::from([
        ("bootstrap", &["domain", "entity_types"] as &[&str]),
        ("rewrite_section", &["section", "changes"]),
        ("inbox_merge", &["document_content", "inbox_content"]),
        (
            "organize_merge",
            &["doc_title", "keep_facts", "merge_facts"],
        ),
        ("organize_split", &["doc_title", "facts", "sections"]),
        ("cross_validate", &["doc_title", "fact_batch"]),
        ("link_detect", &["entities_list", "content"]),
        ("link_detect_batch", &["entities_list", "docs_section"]),
        ("entity_discover", &["content"]),
        ("entity_classify", &["types_list", "candidates"]),
    ])
}

/// Validate prompt templates: warn on unknown keys or unknown placeholders.
pub fn validate_prompts(config: &PromptsConfig) {
    let known = known_prompts();
    for (key, template) in &config.templates {
        if let Some(allowed_vars) = known.get(key.as_str()) {
            // Check for unknown placeholders
            let allowed: HashSet<&str> = allowed_vars.iter().copied().collect();
            for var in extract_placeholders(template) {
                if !allowed.contains(var.as_str()) {
                    warn!(
                        "Prompt '{}' references unknown placeholder '{{{}}}'. Known: {:?}",
                        key, var, allowed_vars
                    );
                }
            }
        } else {
            warn!(
                "Unknown prompt key '{}' in config. Known keys: {:?}",
                key,
                known.keys().collect::<Vec<_>>()
            );
        }
    }
}

/// Extract `{placeholder}` names from a template string.
pub(crate) fn extract_placeholders(template: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Skip escaped {{
            if chars.peek() == Some(&'{') {
                chars.next();
                continue;
            }
            let mut name = String::new();
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
                name.push(inner);
            }
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                result.push(name);
            }
        }
    }
    result
}

/// Load a text file override from `.factbase/<relative_path>` under `repo_path`.
/// Returns `None` if the file doesn't exist; logs a warning and returns `None` on read error.
pub fn load_file_override(repo_path: &Path, relative_path: &str) -> Option<String> {
    let path = repo_path.join(".factbase").join(relative_path);
    if !path.exists() {
        return None;
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => Some(content),
        Err(e) => {
            warn!("Failed to read override file {}: {}", path.display(), e);
            None
        }
    }
}

/// Resolve a prompt: use config override if present, otherwise use default.
/// Substitutes `{var}` placeholders with provided values.
pub fn resolve_prompt(
    config: &PromptsConfig,
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
    fn test_resolve_prompt_uses_default() {
        let config = PromptsConfig::default();
        let result = resolve_prompt(&config, "test", "Hello {name}!", &[("name", "World")]);
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_resolve_prompt_uses_override() {
        let mut config = PromptsConfig::default();
        config
            .templates
            .insert("test".into(), "Custom: {name}".into());
        let result = resolve_prompt(&config, "test", "Hello {name}!", &[("name", "World")]);
        assert_eq!(result, "Custom: World");
    }

    #[test]
    fn test_resolve_prompt_multiple_vars() {
        let config = PromptsConfig::default();
        let result = resolve_prompt(&config, "k", "{a} and {b}", &[("a", "X"), ("b", "Y")]);
        assert_eq!(result, "X and Y");
    }

    #[test]
    fn test_extract_placeholders() {
        let vars = extract_placeholders("Hello {name}, your {item} is ready");
        assert_eq!(vars, vec!["name", "item"]);
    }

    #[test]
    fn test_extract_placeholders_skips_escaped() {
        let vars = extract_placeholders("JSON: {{\"key\": \"{value}\"}}");
        assert_eq!(vars, vec!["value"]);
    }

    #[test]
    fn test_extract_placeholders_empty() {
        let vars = extract_placeholders("No placeholders here");
        assert!(vars.is_empty());
    }

    #[test]
    fn test_known_prompts_has_all_keys() {
        let known = known_prompts();
        assert_eq!(known.len(), 10);
        assert!(known.contains_key("bootstrap"));
        assert!(known.contains_key("rewrite_section"));
        assert!(known.contains_key("inbox_merge"));
        assert!(known.contains_key("organize_merge"));
        assert!(known.contains_key("organize_split"));
        assert!(known.contains_key("cross_validate"));
        assert!(known.contains_key("link_detect"));
        assert!(known.contains_key("link_detect_batch"));
        assert!(known.contains_key("entity_discover"));
        assert!(known.contains_key("entity_classify"));
    }

    #[test]
    fn test_validate_prompts_warns_unknown_key() {
        // Just ensure it doesn't panic — warning is logged
        let mut config = PromptsConfig::default();
        config
            .templates
            .insert("nonexistent".into(), "template".into());
        validate_prompts(&config);
    }

    #[test]
    fn test_validate_prompts_warns_unknown_placeholder() {
        let mut config = PromptsConfig::default();
        config
            .templates
            .insert("bootstrap".into(), "{domain} {unknown_var}".into());
        validate_prompts(&config);
    }

    #[test]
    fn test_validate_prompts_ok_for_valid() {
        let mut config = PromptsConfig::default();
        config.templates.insert(
            "bootstrap".into(),
            "Domain: {domain}, Types: {entity_types}".into(),
        );
        validate_prompts(&config); // no warnings
    }

    #[test]
    fn test_resolve_prompt_no_vars() {
        let config = PromptsConfig::default();
        let result = resolve_prompt(&config, "k", "static prompt", &[]);
        assert_eq!(result, "static prompt");
    }

    #[test]
    fn test_prompts_config_deserialize_from_yaml() {
        let yaml = r#"
bootstrap: "Custom bootstrap for {domain} with {entity_types}"
link_detect: "Find entities: {entities_list} in {content}"
"#;
        let config: PromptsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates.len(), 2);
        assert!(config.templates.contains_key("bootstrap"));
        assert!(config.templates.contains_key("link_detect"));
    }

    #[test]
    fn test_prompts_config_in_full_config() {
        let yaml = r#"
database:
  pool_size: 4
prompts:
  bootstrap: "Custom: {domain}"
  rewrite_section: "Rewrite: {section} with {changes}"
"#;
        let config: crate::Config = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.prompts.templates.len(), 2);
        let result = resolve_prompt(
            &config.prompts,
            "bootstrap",
            "default",
            &[("domain", "test")],
        );
        assert_eq!(result, "Custom: test");
    }

    // ==================== load_file_override tests ====================

    #[test]
    fn test_load_file_override_returns_none_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = load_file_override(tmp.path(), "prompts/bootstrap.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_load_file_override_returns_content_when_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let prompts_dir = tmp.path().join(".factbase").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(prompts_dir.join("bootstrap.txt"), "Custom bootstrap prompt").unwrap();
        let result = load_file_override(tmp.path(), "prompts/bootstrap.txt");
        assert_eq!(result.as_deref(), Some("Custom bootstrap prompt"));
    }

    #[test]
    fn test_load_file_override_top_level_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let factbase_dir = tmp.path().join(".factbase");
        std::fs::create_dir_all(&factbase_dir).unwrap();
        std::fs::write(factbase_dir.join("obsidian.css"), "/* custom css */").unwrap();
        let result = load_file_override(tmp.path(), "obsidian.css");
        assert_eq!(result.as_deref(), Some("/* custom css */"));
    }
}

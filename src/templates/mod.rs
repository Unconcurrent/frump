use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use crate::domain::*;

/// A task template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub name: String,
    pub task_type: String,
    pub subject_template: String,
    pub body_template: String,
    /// Keeps the order the template author gave.
    pub properties: IndexMap<String, String>,
}

impl TaskTemplate {
    /// Refuse a template whose property keys or values a task could not hold.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("A template needs a name");
        }
        for (key, value) in &self.properties {
            PropertyKey::new(key)
                .with_context(|| format!("Template '{}' has an invalid property key", self.name))?;
            validate_property_value(value).with_context(|| {
                format!("Template '{}' has an invalid value for {}", self.name, key)
            })?;
        }
        Ok(())
    }

    /// Create a task from this template.
    ///
    /// A placeholder is `{name}`, where the name is letters, digits, `_` or `-`; other brace text
    /// is literal. Each placeholder is replaced once, from the original text, and a fill value is
    /// never expanded again. A placeholder without a fill, and a fill that no used text needs,
    /// are errors. When `body` is given it replaces the template body and is not filled.
    pub fn instantiate(
        &self,
        id: TaskId,
        fills: &IndexMap<String, String>,
        body: Option<&str>,
    ) -> Result<Task> {
        let mut used = BTreeSet::new();
        let subject = fill(&self.subject_template, fills, &mut used)?;
        let body = match body {
            Some(body) => body.to_string(),
            None => fill(&self.body_template, fills, &mut used)?,
        };
        let unused: Vec<&str> = fills
            .keys()
            .filter(|key| !used.contains(key.as_str()))
            .map(String::as_str)
            .collect();
        if !unused.is_empty() {
            bail!(
                "Template '{}' has no placeholder for: {}",
                self.name,
                unused.join(", ")
            );
        }
        let mut task = Task::new(id, TaskType::parse(&self.task_type), subject);
        task.set_body(body);
        for (key, value) in &self.properties {
            task.add_property(PropertyKey::new(key)?, value.clone());
        }
        Ok(task)
    }
}

fn is_placeholder_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn fill(
    text: &str,
    fills: &IndexMap<String, String>,
    used: &mut BTreeSet<String>,
) -> Result<String> {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    let mut missing = Vec::new();
    while let Some(open) = rest.find('{') {
        output.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if is_placeholder_name(&after[..close]) => {
                let name = &after[..close];
                match fills.get(name) {
                    Some(value) => {
                        output.push_str(value);
                        used.insert(name.to_string());
                    }
                    None => missing.push(name.to_string()),
                }
                rest = &after[close + 1..];
            }
            _ => {
                output.push('{');
                rest = after;
            }
        }
    }
    output.push_str(rest);
    if !missing.is_empty() {
        missing.dedup();
        bail!("Missing --fill for: {}", missing.join(", "));
    }
    Ok(output)
}

/// Parse `KEY=VALUE` arguments in order; a repeated key is an error.
pub fn parse_pairs(pairs: &[String], flag: &str) -> Result<IndexMap<String, String>> {
    let mut map = IndexMap::new();
    for pair in pairs {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| anyhow!("{flag} must use KEY=VALUE, got '{pair}'"))?;
        if map.insert(key.to_string(), value.to_string()).is_some() {
            bail!("{flag} names {key} more than once");
        }
    }
    Ok(map)
}

/// Template manager for one template file.
pub struct TemplateManager {
    templates_file: PathBuf,
}

impl TemplateManager {
    /// Templates stored in `templates_file` (see `board::templates_file`).
    pub fn new(templates_file: PathBuf) -> Self {
        TemplateManager { templates_file }
    }

    /// Load templates from file
    pub fn load(&self) -> Result<Vec<TaskTemplate>> {
        if !self.templates_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.templates_file)
            .with_context(|| format!("Failed to read {}", self.templates_file.display()))?;

        let templates: Vec<TaskTemplate> = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", self.templates_file.display()))?;
        for template in &templates {
            template
                .validate()
                .with_context(|| format!("In {}", self.templates_file.display()))?;
        }

        Ok(templates)
    }

    /// Save templates to file
    pub fn save(&self, templates: &[TaskTemplate]) -> Result<()> {
        let content =
            serde_json::to_string_pretty(templates).context("Failed to serialize templates")?;

        fs::write(&self.templates_file, content)
            .with_context(|| format!("Failed to write {}", self.templates_file.display()))?;

        Ok(())
    }

    /// Add a new template
    pub fn add(&self, template: TaskTemplate) -> Result<()> {
        template.validate()?;
        let mut templates = self.load()?;

        // Check if template with same name exists
        if templates.iter().any(|t| t.name == template.name) {
            return Err(anyhow::anyhow!(
                "Template '{}' already exists",
                template.name
            ));
        }

        templates.push(template);
        self.save(&templates)?;

        Ok(())
    }

    /// Get a template by name
    pub fn get(&self, name: &str) -> Result<TaskTemplate> {
        let templates = self.load()?;

        templates
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| anyhow::anyhow!("Template '{}' not found", name))
    }

    /// List all templates
    pub fn list(&self) -> Result<Vec<TaskTemplate>> {
        self.load()
    }

    /// Remove a template
    pub fn remove(&self, name: &str) -> Result<()> {
        let mut templates = self.load()?;

        let original_len = templates.len();
        templates.retain(|t| t.name != name);

        if templates.len() == original_len {
            return Err(anyhow::anyhow!("Template '{}' not found", name));
        }

        self.save(&templates)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(subject: &str, body: &str) -> TaskTemplate {
        TaskTemplate {
            name: "bug".to_string(),
            task_type: "Bug".to_string(),
            subject_template: subject.to_string(),
            body_template: body.to_string(),
            properties: IndexMap::from([
                ("Priority".to_string(), "2".to_string()),
                ("Assigned To".to_string(), "hipp".to_string()),
            ]),
        }
    }

    fn fills(pairs: &[&str]) -> IndexMap<String, String> {
        parse_pairs(
            &pairs.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
            "--fill",
        )
        .unwrap()
    }

    #[test]
    fn instantiate_fills_subject_and_body_once_and_keeps_property_order() {
        let task = template(
            "Fix {component} issue",
            "Issue in {component}: {description} {not a name}",
        )
        .instantiate(
            TaskId::new(7).unwrap(),
            &fills(&["component=parser", "description=uses {component} literally"]),
            None,
        )
        .unwrap();
        assert_eq!(task.subject, "Fix parser issue");
        assert_eq!(
            task.body,
            "Issue in parser: uses {component} literally {not a name}"
        );
        assert_eq!(task.task_type, TaskType::parse("Bug"));
        let keys: Vec<&str> = task.properties.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["Priority", "Assigned To"]);
    }

    #[test]
    fn instantiate_refuses_missing_and_unused_fills() {
        let bug = template("Fix {component} issue", "{description}");
        let missing = bug
            .instantiate(TaskId::new(1).unwrap(), &fills(&["component=parser"]), None)
            .unwrap_err();
        assert!(missing.to_string().contains("description"), "{missing}");
        let unused = bug
            .instantiate(
                TaskId::new(1).unwrap(),
                &fills(&["component=parser", "description=x", "extra=y"]),
                None,
            )
            .unwrap_err();
        assert!(unused.to_string().contains("extra"), "{unused}");
    }

    #[test]
    fn an_overridden_body_is_not_filled() {
        let task = template("Fix {component} issue", "{description}")
            .instantiate(
                TaskId::new(1).unwrap(),
                &fills(&["component=parser"]),
                Some("Own text with {braces}"),
            )
            .unwrap();
        assert_eq!(task.body, "Own text with {braces}");
    }

    #[test]
    fn parse_pairs_refuses_duplicates_and_missing_equals() {
        let args = |pairs: &[&str]| pairs.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        assert!(parse_pairs(&args(&["a=1", "a=2"]), "--fill").is_err());
        assert!(parse_pairs(&args(&["a"]), "--fill").is_err());
        assert_eq!(
            parse_pairs(&args(&["a=b=c"]), "--fill").unwrap()["a"],
            "b=c"
        );
    }

    #[test]
    fn a_template_file_with_an_invalid_property_key_is_an_error() {
        let root = std::env::temp_dir().join(format!(
            "frump-template-invalid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let file = root.join(".frump_templates.json");
        fs::write(
            &file,
            r#"[{"name":"bug","task_type":"Bug","subject_template":"S","body_template":"","properties":{"lower case":"x"}}]"#,
        )
        .unwrap();
        let error = TemplateManager::new(file).list().unwrap_err();
        assert!(
            format!("{error:#}").contains("invalid property key"),
            "{error:#}"
        );
        let _ = fs::remove_dir_all(root);
    }
}

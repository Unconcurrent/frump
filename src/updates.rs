use anyhow::{bail, Result};
use chrono::{SecondsFormat, Utc};

use crate::{PropertyKey, Task};

pub const LAST_UPDATED_PROPERTY: &str = "Last Updated";

/// Return a sortable, unambiguous timestamp shared by the CLI and web server.
pub fn now_utc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Record a successful task mutation without allowing a duplicate property.
pub fn mark_updated(task: &mut Task, timestamp: &str) {
    task.set_property(
        PropertyKey::new(LAST_UPDATED_PROPERTY).expect("Last Updated is a valid property key"),
        timestamp.to_string(),
    );
}

/// Append an auditable Markdown update and record the same instant as task metadata.
pub fn append_update(task: &mut Task, text: &str, author: Option<&str>) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Appended body text cannot be empty.");
    }

    let timestamp = now_utc();
    let attribution = author
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| format!(" - {name}"))
        .unwrap_or_default();
    // A plain line, as a person would type it. A Markdown heading would be parsed as a task
    // boundary by Frump's line-oriented format.
    let entry = format!("Update {timestamp}{attribution}\n\n{text}");
    task.body = if task.body.trim().is_empty() {
        entry
    } else {
        format!("{}\n\n{entry}", task.body.trim_end())
    };
    mark_updated(task, &timestamp);
    Ok(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TaskId, TaskType};

    #[test]
    fn append_update_adds_timestamp_author_and_metadata() {
        let mut task = Task::new(TaskId::new(1).unwrap(), TaskType::Task, "Example".into());
        append_update(&mut task, "Checked the result.", Some("data")).unwrap();

        assert!(task.body.starts_with("Update "));
        assert!(task.body.contains(" - data\n\nChecked the result."));
        assert_eq!(
            task.get_property(&PropertyKey::new(LAST_UPDATED_PROPERTY).unwrap()),
            Some(&task.body[7..27])
        );
    }
}

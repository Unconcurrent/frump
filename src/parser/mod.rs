//! Markdown parser for Frump documents.
//!
//! This module provides functions to parse and serialize Frump documents in Markdown format.
//!
//! # Format
//!
//! A Frump document consists of:
//! 1. A header section (markdown text)
//! 2. A Team section listing team members
//! 3. A Tasks section with task definitions
//!
//! # Example Markdown Format
//!
//! ```text
//! # My Project
//!
//! ## Team
//!
//! * John Doe <john@example.com> - Developer
//!
//! ## Tasks
//!
//! ### Task 1 - Build authentication
//!
//! Implement login and registration.
//! Status: working
//! ```
//!
//! # Usage
//!
//! ```rust
//! use frump::parser;
//!
//! let content = "# Project\n\n## Team\n\n## Tasks\n";
//! let doc = parser::parse(content).unwrap();
//! let serialized = parser::serialize(&doc);
//! assert!(serialized.contains("# Project"));
//! ```

use anyhow::{anyhow, Result};

use crate::domain::*;

/// Parse a Frump document from Markdown format.
///
/// # Arguments
///
/// * `content` - The Markdown content to parse
///
/// # Returns
///
/// A `FrumpDoc` containing the parsed header, team, and tasks, or an error if parsing fails.
///
/// # Errors
///
/// Returns an error if:
/// - Task ID is invalid (zero or negative)
/// - Property key doesn't meet validation rules
/// - Team member email is invalid
/// - Task format is malformed
///
/// # Example
///
/// ```rust
/// use frump::parser;
///
/// let content = "# My Project\n\n## Team\n\n## Tasks\n";
/// let doc = parser::parse(content).unwrap();
/// assert_eq!(doc.tasks.len(), 0);  // No tasks in this example
/// assert_eq!(doc.team.len(), 0);    // No team members in this example
/// ```
pub fn parse(content: &str) -> Result<FrumpDoc> {
    let mut header = String::new();
    let mut team_members = Vec::new();
    let mut next = Vec::new();

    let lines: Vec<&str> = content.lines().collect();
    let section = |name: &str| {
        lines
            .iter()
            .position(|line| line.trim().eq_ignore_ascii_case(name))
    };
    let team_start = section("## Team");
    let next_start = section("## Next");
    let tasks_start = section("## Tasks");
    let first_section = [team_start, next_start, tasks_start]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(lines.len());
    for line in &lines[..first_section] {
        header.push_str(line);
        header.push('\n');
    }

    // Parse Team section if present
    if let Some(start) = team_start {
        let mut i = start + 1;
        while i < lines.len() {
            let line = lines[i].trim();
            if line.starts_with("## ") {
                break;
            }
            if line.starts_with("* ") || line.starts_with("- ") {
                if let Some(member) = parse_team_member(line)? {
                    team_members.push(member);
                }
            }
            i += 1;
        }
    }

    if let Some(start) = next_start {
        let mut i = start + 1;
        while i < lines.len() && !lines[i].trim().starts_with("## ") {
            let value = lines[i]
                .trim()
                .trim_start_matches("- ")
                .trim_start_matches("* ")
                .trim();
            if !value.is_empty() {
                next.push(TaskId::new(
                    value
                        .parse()
                        .map_err(|_| anyhow!("Invalid Next task ID: {value}"))?,
                )?);
            }
            i += 1;
        }
    }

    let tasks = tasks_in(&lines, tasks_start)?;

    let team = Team::new(team_members);
    let task_collection = TaskCollection::new(tasks);

    let mut doc = FrumpDoc::new(header, team, task_collection);
    doc.next = next;
    Ok(doc)
}

/// The tasks of a board's `## Tasks` section, read exactly as [`parse`] reads them, without
/// reading the header, Team or Next sections. Git history uses it: a committed typo in the Team
/// section must not make the task numbers of that commit unreadable.
pub fn parse_tasks(content: &str) -> Result<Vec<Task>> {
    let lines: Vec<&str> = content.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("## Tasks"));
    tasks_in(&lines, start)
}

/// Tasks from the `## Tasks` line at `start` to the end of the file, one per `### ` heading.
fn tasks_in(lines: &[&str], start: Option<usize>) -> Result<Vec<Task>> {
    let mut tasks = Vec::new();
    let Some(start) = start else {
        return Ok(tasks);
    };
    let mut i = start + 1;

    while i < lines.len() {
        let line = lines[i].trim();

        // Start of a new task
        if line.starts_with("### ") {
            let task_start = i;
            i += 1;

            // Find the end of this task (next ### or end of file)
            while i < lines.len() && !lines[i].trim().starts_with("### ") {
                i += 1;
            }

            if let Some(task) = parse_task(&lines[task_start..i])? {
                tasks.push(task);
            }
        } else {
            i += 1;
        }
    }
    Ok(tasks)
}

/// Serialize a Frump document to Markdown format.
///
/// Converts a `FrumpDoc` back to the standard Markdown representation.
/// This function is the inverse of `parse()` and should produce valid
/// Markdown that can be parsed back.
///
/// # Arguments
///
/// * `doc` - The Frump document to serialize
///
/// # Returns
///
/// A `String` containing the Markdown representation of the document.
///
/// # Example
///
/// ```rust
/// use frump::{parser, Task, TaskId, TaskType, TaskCollection, Team, FrumpDoc};
///
/// let tasks = vec![
///     Task::new(TaskId::new(1).unwrap(), TaskType::Task, "Test task".to_string())
/// ];
/// let doc = FrumpDoc::new(
///     "# Test\n".to_string(),
///     Team::new(vec![]),
///     TaskCollection::new(tasks)
/// );
///
/// let markdown = parser::serialize(&doc);
/// assert!(markdown.contains("### Task 1 - Test task"));
/// ```
pub fn serialize(doc: &FrumpDoc) -> String {
    let mut output = String::new();

    // Write header
    output.push_str(&doc.header);

    // Write Team section if there are team members
    if !doc.team.is_empty() {
        output.push_str("## Team\n\n");
        for member in doc.team.members() {
            output.push_str(&format!("* {} <{}>", member.name, member.email));
            if let Some(role) = &member.role {
                output.push_str(&format!(" - {}", role));
            }
            output.push('\n');
        }
        output.push('\n');
    }

    if !doc.next.is_empty() {
        output.push_str("## Next\n\n");
        for id in &doc.next {
            output.push_str(&format!("- {}\n", id));
        }
        output.push('\n');
    }

    // Write Tasks section
    output.push_str("## Tasks\n\n");
    for task in doc.tasks.tasks() {
        output.push_str(&format!(
            "### {} {} - {}\n",
            task.task_type, task.id, task.subject
        ));

        if !task.body.is_empty() {
            output.push('\n');
            output.push_str(task.body.trim());
            output.push('\n');
        }

        if !task.properties.is_empty() {
            if !task.body.is_empty() {
                output.push('\n');
            }
            for prop in &task.properties {
                output.push_str(&format!("{}: {}\n", prop.key, prop.value));
            }
        }

        output.push('\n');
    }

    output
}

fn parse_team_member(line: &str) -> Result<Option<TeamMember>> {
    // Format: * Name <email> - Role
    // or: * Name <email>
    let line = line.trim_start_matches("* ").trim_start_matches("- ");

    if let Some(email_start) = line.find('<') {
        if let Some(email_end) = line.find('>') {
            let name = line[..email_start].trim().to_string();
            let email_str = &line[email_start + 1..email_end];
            let email = Email::new(email_str)?;

            let role = if email_end + 1 < line.len() {
                line[email_end + 1..]
                    .trim()
                    .strip_prefix('-')
                    .map(|role| role.trim().to_string())
            } else {
                None
            };

            let mut member = TeamMember::new(name, email);
            if let Some(r) = role {
                member.set_role(r);
            }

            return Ok(Some(member));
        }
    }

    Ok(None)
}

fn parse_task(lines: &[&str]) -> Result<Option<Task>> {
    if lines.is_empty() {
        return Ok(None);
    }

    // Parse the heading: ### Task 3 - Write docs
    let heading = lines[0].trim_start_matches("### ").trim();

    // Split into task_type, id, and subject
    let parts: Vec<&str> = heading.splitn(3, ' ').collect();

    if parts.len() < 2 {
        return Err(anyhow!("Invalid task heading format: {}", heading));
    }

    let task_type = TaskType::parse(parts[0]);
    let id = TaskId::new(
        parts[1]
            .parse()
            .map_err(|_| anyhow!("Invalid task ID: {}", parts[1]))?,
    )?;

    let subject = if parts.len() > 2 {
        parts[2].trim_start_matches("- ").trim().to_string()
    } else {
        String::new()
    };

    let mut task = Task::new(id, task_type, subject);

    // Properties form the final contiguous metadata block. Parsing a property-looking
    // line in the middle of prose as metadata would silently truncate the body.
    let mut property_end = lines.len();
    while property_end > 1 && lines[property_end - 1].trim().is_empty() {
        property_end -= 1;
    }
    let mut property_start = property_end;
    while property_start > 1 && try_parse_property(lines[property_start - 1].trim()).is_some() {
        property_start -= 1;
    }

    let mut body_lines = Vec::new();
    for line in &lines[1..property_start] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !body_lines.is_empty() {
                body_lines.push("");
            }
            continue;
        }
        body_lines.push(trimmed);
    }

    for line in &lines[property_start..property_end] {
        if let Some((key, value)) = try_parse_property(line.trim()) {
            task.add_property(key, value);
        }
    }

    let body = body_lines.join("\n").trim().to_string();
    task.set_body(body);

    Ok(Some(task))
}

fn try_parse_property(line: &str) -> Option<(PropertyKey, String)> {
    // A property is a name/value pair separated by ':'
    // The name must be capitalized and can consist of maximum 3 words
    if let Some(colon_pos) = line.find(':') {
        let key_str = line[..colon_pos].trim();
        let value = line[colon_pos + 1..].trim();

        // Try to parse as a property key
        if let Some(key) = PropertyKey::try_parse(key_str) {
            return Some((key, value.to_string()));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_task() {
        let content = r#"# Test

## Tasks

### Task 1 - test task
"#;
        let doc = parse(content).unwrap();
        assert_eq!(doc.tasks.len(), 1);
        let task = doc.tasks.tasks().first().unwrap();
        assert_eq!(task.id.value(), 1);
        assert_eq!(task.subject, "test task");
    }

    #[test]
    fn next_section_round_trips_before_tasks() {
        let content = "# Test\n\n## Next\n\n- 2\n- 1\n\n## Tasks\n\n### Task 1 - First\n\nStatus: todo\n\n### Task 2 - Second\n\nStatus: todo\n";
        let document = parse(content).unwrap();
        assert_eq!(
            document.next,
            vec![TaskId::new(2).unwrap(), TaskId::new(1).unwrap()]
        );
        let reparsed = parse(&serialize(&document)).unwrap();
        assert_eq!(reparsed.next, document.next);
    }

    #[test]
    fn test_parse_task_with_body() {
        let content = r#"# Test

## Tasks

### Task 1 - test task

This is the body
"#;
        let doc = parse(content).unwrap();
        let task = doc.tasks.tasks().first().unwrap();
        assert_eq!(task.body, "This is the body");
    }

    #[test]
    fn test_parse_task_with_properties() {
        let content = r#"# Test

## Tasks

### Task 1 - test task

Some body text
Status: working
Assigned To: John
"#;
        let doc = parse(content).unwrap();
        let task = doc.tasks.tasks().first().unwrap();
        assert_eq!(task.status(), Some("working"));
        assert_eq!(task.assignee(), Some("John"));
    }

    #[test]
    fn report_labels_in_body_do_not_become_properties() {
        let content = r#"# Test

## Tasks

### Investigation 114 - Preserve report body

INVESTIGATION REPORT: evidence follows.

CONTRACT 1: do not lose this paragraph.

Status: investigations
Assigned To: Ada
"#;
        let doc = parse(content).unwrap();
        let task = doc.tasks.tasks().first().unwrap();
        assert!(task
            .body
            .contains("INVESTIGATION REPORT: evidence follows."));
        assert!(task
            .body
            .contains("CONTRACT 1: do not lose this paragraph."));
        assert_eq!(task.status(), Some("investigations"));
        assert_eq!(task.assignee(), Some("Ada"));
    }

    #[test]
    fn test_parse_team() {
        let content = r#"# Test

## Team

* John Doe <john@example.com> - Developer
* Jane Smith <jane@example.com>

## Tasks
"#;
        let doc = parse(content).unwrap();
        assert_eq!(doc.team.len(), 2);
        let first = doc.team.members().first().unwrap();
        assert_eq!(first.name, "John Doe");
        assert_eq!(first.role, Some("Developer".to_string()));
    }

    #[test]
    fn test_round_trip() {
        let content = r#"# Test

## Team

* John Doe <john@example.com> - Developer

## Tasks

### Task 1 - test task

Body text
Status: working

"#;
        let doc = parse(content).unwrap();
        let serialized = serialize(&doc);
        let doc2 = parse(&serialized).unwrap();

        assert_eq!(doc.tasks.len(), doc2.tasks.len());
        assert_eq!(doc.team.len(), doc2.team.len());
    }

    #[test]
    fn test_parse_multiline_body() {
        let content = r#"# Test

## Tasks

### Task 1 - test task

Line 1
Line 2
Line 3
Status: working
"#;
        let doc = parse(content).unwrap();
        let task = doc.tasks.tasks().first().unwrap();
        assert!(task.body.contains("Line 1"));
        assert!(task.body.contains("Line 2"));
        assert!(task.body.contains("Line 3"));
    }

    #[test]
    fn test_parse_empty_body() {
        let content = r#"# Test

## Tasks

### Task 1 - test task

Status: working
"#;
        let doc = parse(content).unwrap();
        let task = doc.tasks.tasks().first().unwrap();
        assert!(task.body.is_empty() || task.body.trim().is_empty());
    }

    #[test]
    fn test_parse_multiple_types() {
        let content = r#"# Test

## Tasks

### Task 1 - task one

### Bug 2 - bug two

### Feature 3 - feature three
"#;
        let doc = parse(content).unwrap();
        assert_eq!(doc.tasks.len(), 3);
        assert_eq!(doc.tasks.tasks()[0].task_type, TaskType::Task);
        assert_eq!(doc.tasks.tasks()[1].task_type, TaskType::Bug);
        assert_eq!(doc.tasks.tasks()[2].task_type, TaskType::Feature);
    }

    #[test]
    fn test_parse_no_tasks() {
        let content = r#"# Test

## Team

* John Doe <john@example.com>

## Tasks
"#;
        let doc = parse(content).unwrap();
        assert_eq!(doc.tasks.len(), 0);
        assert_eq!(doc.team.len(), 1);
    }

    #[test]
    fn test_parse_no_team() {
        let content = r#"# Test

## Team

## Tasks

### Task 1 - test task
"#;
        let doc = parse(content).unwrap();
        assert_eq!(doc.tasks.len(), 1);
        assert_eq!(doc.team.len(), 0);
    }

    #[test]
    fn test_parse_special_characters_in_subject() {
        let content = r#"# Test

## Tasks

### Task 1 - Test with "quotes" and 'apostrophes' & symbols!
"#;
        let doc = parse(content).unwrap();
        let task = doc.tasks.tasks().first().unwrap();
        assert_eq!(
            task.subject,
            "Test with \"quotes\" and 'apostrophes' & symbols!"
        );
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Helper to generate valid task IDs
    fn task_id_strategy() -> impl Strategy<Value = u32> {
        1u32..=1000u32
    }

    proptest! {
        #[test]
        fn test_property_round_trip(
            id in task_id_strategy(),
            subject in "[a-zA-Z0-9 ]{1,100}",
            body in "[a-zA-Z0-9 \n.]{0,500}",
        ) {
            let content = format!(
                "# Test\n\n## Tasks\n\n### Task {} - {}\n\n{}\n",
                id, subject, body
            );

            if let Ok(doc) = parse(&content) {
                let serialized = serialize(&doc);
                if let Ok(doc2) = parse(&serialized) {
                    prop_assert_eq!(doc.tasks.len(), doc2.tasks.len());
                    if !doc.tasks.is_empty() {
                        prop_assert_eq!(
                            doc.tasks.tasks()[0].id.value(),
                            doc2.tasks.tasks()[0].id.value()
                        );
                        prop_assert_eq!(
                            &doc.tasks.tasks()[0].subject,
                            &doc2.tasks.tasks()[0].subject
                        );
                    }
                }
            }
        }

        #[test]
        fn test_property_multiple_tasks_round_trip(
            ids in prop::collection::vec(task_id_strategy(), 1..5),
        ) {
            let mut content = String::from("# Test\n\n## Tasks\n\n");

            for (idx, id) in ids.iter().enumerate() {
                content.push_str(&format!("### Task {} - Subject {}\n\n", id, idx));
            }

            if let Ok(doc) = parse(&content) {
                let serialized = serialize(&doc);
                if let Ok(doc2) = parse(&serialized) {
                    prop_assert_eq!(doc.tasks.len(), doc2.tasks.len());
                }
            }
        }

        #[test]
        fn test_property_team_round_trip(
            name in "[A-Z][a-z]{2,15} [A-Z][a-z]{2,15}",
            domain in "[a-z]{3,10}",
        ) {
            let email = format!("{}@{}.com", name.to_lowercase().replace(" ", "."), domain);
            let content = format!(
                "# Test\n\n## Team\n\n* {} <{}>\n\n## Tasks\n",
                name, email
            );

            if let Ok(doc) = parse(&content) {
                let serialized = serialize(&doc);
                if let Ok(doc2) = parse(&serialized) {
                    prop_assert_eq!(doc.team.len(), doc2.team.len());
                    if !doc.team.is_empty() {
                        prop_assert_eq!(
                            &doc.team.members()[0].name,
                            &doc2.team.members()[0].name
                        );
                    }
                }
            }
        }
    }
}

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::domain::*;

/// Serializable task representation for export
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportTask {
    pub id: u32,
    pub task_type: String,
    pub subject: String,
    pub body: String,
    /// Keeps the order of the properties in the task file.
    pub properties: IndexMap<String, String>,
}

/// Serializable team member for export
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportTeamMember {
    pub name: String,
    pub email: String,
    pub role: Option<String>,
}

/// Complete document export format
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportDoc {
    pub header: String,
    pub team: Vec<ExportTeamMember>,
    #[serde(default)]
    pub next: Vec<u32>,
    pub tasks: Vec<ExportTask>,
}

impl From<&FrumpDoc> for ExportDoc {
    fn from(doc: &FrumpDoc) -> Self {
        let team = doc
            .team
            .members()
            .iter()
            .map(|m| ExportTeamMember {
                name: m.name.clone(),
                email: m.email.as_str().to_string(),
                role: m.role.clone(),
            })
            .collect();

        let tasks = doc
            .tasks
            .tasks()
            .iter()
            .map(|t| ExportTask {
                id: t.id.value(),
                task_type: t.task_type.as_str().to_string(),
                subject: t.subject.clone(),
                body: t.body.clone(),
                properties: t
                    .properties
                    .iter()
                    .map(|p| (p.key.as_str().to_string(), p.value.clone()))
                    .collect(),
            })
            .collect();

        ExportDoc {
            header: doc.header.clone(),
            team,
            next: doc.next.iter().map(|id| id.value()).collect(),
            tasks,
        }
    }
}

impl ExportDoc {
    /// Convert to FrumpDoc
    pub fn to_frump_doc(&self) -> Result<FrumpDoc> {
        let team_members: Result<Vec<TeamMember>> = self
            .team
            .iter()
            .map(|m| {
                let email = Email::new(&m.email)?;
                let mut member = TeamMember::new(m.name.clone(), email);
                if let Some(role) = &m.role {
                    member.set_role(role.clone());
                }
                Ok(member)
            })
            .collect();

        let tasks: Result<Vec<Task>> = self
            .tasks
            .iter()
            .map(|t| {
                let id = TaskId::new(t.id)?;
                let task_type = TaskType::parse(&t.task_type);
                let mut task = Task::new(id, task_type, t.subject.clone());
                task.set_body(t.body.clone());

                for (key, value) in &t.properties {
                    let prop_key = PropertyKey::new(key).with_context(|| {
                        format!("Task {} has an invalid property key '{}'", t.id, key)
                    })?;
                    task.add_property(prop_key, value.clone());
                }

                Ok(task)
            })
            .collect();

        let mut doc = FrumpDoc::new(
            self.header.clone(),
            Team::new(team_members?),
            TaskCollection::new(tasks?),
        );
        doc.next = self
            .next
            .iter()
            .map(|id| TaskId::new(*id))
            .collect::<Result<_>>()?;
        doc.validate_next()?;
        Ok(doc)
    }
}

/// Export to JSON
pub fn export_json(doc: &FrumpDoc) -> Result<String> {
    let export = ExportDoc::from(doc);
    serde_json::to_string_pretty(&export).context("Failed to serialize to JSON")
}

/// Import from JSON
pub fn import_json(json: &str) -> Result<FrumpDoc> {
    let export: ExportDoc = serde_json::from_str(json).context("Failed to parse JSON")?;
    export.to_frump_doc()
}

/// CSV record for task export
#[derive(Debug, Serialize)]
pub struct CsvTask {
    pub id: u32,
    pub task_type: String,
    pub subject: String,
    pub body: String,
    pub status: String,
    pub assignee: String,
    pub tags: String,
}

/// Export to CSV
pub fn export_csv(doc: &FrumpDoc) -> Result<String> {
    let mut wtr = csv::Writer::from_writer(vec![]);

    for task in doc.tasks.tasks() {
        let csv_task = CsvTask {
            id: task.id.value(),
            task_type: task.task_type.as_str().to_string(),
            subject: task.subject.clone(),
            body: task.body.clone(),
            status: task.status().unwrap_or("").to_string(),
            assignee: task.assignee().unwrap_or("").to_string(),
            tags: task
                .get_property(&PropertyKey::tags())
                .unwrap_or("")
                .to_string(),
        };
        wtr.serialize(csv_task)?;
    }

    let data = wtr.into_inner()?;
    String::from_utf8(data).context("Failed to convert CSV to string")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_round_trip() {
        let id = TaskId::new(1).unwrap();
        let task = Task::new(id, TaskType::Task, "test".to_string());
        let tasks = TaskCollection::new(vec![task]);
        let team = Team::empty();
        let doc = FrumpDoc::new("# Test\n".to_string(), team, tasks);

        let json = export_json(&doc).unwrap();
        let imported = import_json(&json).unwrap();

        assert_eq!(imported.tasks.len(), 1);
        assert_eq!(imported.tasks.tasks()[0].subject, "test");
    }

    #[test]
    fn json_keeps_the_property_order_of_the_task_file() {
        let mut task = Task::new(TaskId::new(1).unwrap(), TaskType::Task, "test".to_string());
        let keys = [
            "Status",
            "Priority",
            "Depends On",
            "Assigned To",
            "Tags",
            "Last Updated",
        ];
        for (index, key) in keys.iter().enumerate() {
            task.add_property(PropertyKey::new(key).unwrap(), index.to_string());
        }
        let doc = FrumpDoc::new(
            "# Test\n".to_string(),
            Team::empty(),
            TaskCollection::new(vec![task]),
        );

        let json = export_json(&doc).unwrap();
        let positions: Vec<usize> = keys
            .iter()
            .map(|key| json.find(&format!("\"{key}\"")).unwrap())
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{json}");

        let imported = import_json(&json).unwrap();
        let imported_keys: Vec<&str> = imported.tasks.tasks()[0]
            .properties
            .iter()
            .map(|p| p.key.as_str())
            .collect();
        assert_eq!(imported_keys, keys);
    }

    #[test]
    fn test_csv_export() {
        let id = TaskId::new(1).unwrap();
        let mut task = Task::new(id, TaskType::Task, "test".to_string());
        task.set_status("working".to_string());
        let tasks = TaskCollection::new(vec![task]);
        let team = Team::empty();
        let doc = FrumpDoc::new("# Test\n".to_string(), team, tasks);

        let csv = export_csv(&doc).unwrap();
        assert!(csv.contains("test"));
        assert!(csv.contains("working"));
    }
}

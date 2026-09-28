//! Backward-compatible storage for a single board file and an already-sharded board.

use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{parser, FrumpDoc, Task, TaskCollection};

pub fn is_sharded(path: &Path) -> bool {
    let root = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    root.join("general.md").is_file() && root.join("tasks").is_dir()
}

pub fn sharded_root(path: &Path) -> Option<PathBuf> {
    if path.is_dir() && is_sharded(path) {
        Some(path.to_path_buf())
    } else if path.file_name().is_some_and(|name| name == "general.md")
        && is_sharded(path.parent()?)
    {
        path.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

pub fn read(path: &Path) -> Result<FrumpDoc> {
    let Some(root) = sharded_root(path) else {
        let document = parser::parse(
            &fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?,
        )?;
        document.validate_next()?;
        return Ok(document);
    };
    let general = parser::parse(
        &fs::read_to_string(root.join("general.md")).context("Failed to read general.md")?,
    )?;
    let mut tasks = Vec::new();
    for entry in fs::read_dir(root.join("tasks")).context("Failed to read tasks directory")? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|ext| ext == "md") {
            let fragment = fs::read_to_string(entry.path())?;
            let parsed = parser::parse(&format!("## Tasks\n\n{fragment}"))?;
            if parsed.tasks.len() != 1 {
                bail!("{} must contain exactly one task", entry.path().display());
            }
            tasks.extend(parsed.tasks.tasks().iter().cloned());
        }
    }
    tasks.sort_by_key(|task| task.id);
    let mut document = FrumpDoc::new(general.header, general.team, TaskCollection::new(tasks));
    document.next = general.next;
    document.validate_next()?;
    Ok(document)
}

pub fn write(path: &Path, doc: &FrumpDoc) -> Result<()> {
    let Some(root) = sharded_root(path) else {
        return write_if_changed(path, &parser::serialize(doc));
    };
    let mut general = FrumpDoc::new(
        doc.header.clone(),
        doc.team.clone(),
        TaskCollection::empty(),
    );
    general.next = doc.next.clone();
    let tasks_dir = root.join("tasks");
    let expected: BTreeSet<_> = doc
        .tasks
        .tasks()
        .iter()
        .map(|task| task.id.value())
        .collect();
    // One file per task number: duplicate numbers would overwrite each other.
    if expected.len() != doc.tasks.len() {
        bail!(
            "The board has duplicate task numbers, and a board directory holds one file per \
             number. Run `frump renumber-duplicates` first."
        );
    }
    write_if_changed(&root.join("general.md"), &serialize_general(&general))?;
    for task in doc.tasks.tasks() {
        let path = tasks_dir.join(format!("{}.md", task.id.value()));
        if !task_file_matches(&path, task) {
            write_if_changed(&path, &serialize_task(task))?;
        }
    }
    // Every task now has its `<id>.md` file, so any other `.md` file holds a removed task or
    // a task stored under another name (`foo.md` with task 9, now in `9.md`). Only task files
    // are removed: the read path loads `.md` files, and anything else in `tasks/` (a backup
    // such as `123.bak`) is not a task.
    for entry in fs::read_dir(&tasks_dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let canonical = path
            .file_stem()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
            .is_some_and(|id| expected.contains(&id) && path.ends_with(format!("{id}.md")));
        if !canonical {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn task_file_matches(path: &Path, task: &Task) -> bool {
    let Ok(fragment) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(parsed) = parser::parse(&format!("## Tasks\n\n{fragment}")) else {
        return false;
    };
    parsed.tasks.len() == 1 && serialize_task(&parsed.tasks.tasks()[0]) == serialize_task(task)
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() != Some(content) {
        fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

fn serialize_general(doc: &FrumpDoc) -> String {
    let mut content = doc.header.clone();
    if !doc.team.is_empty() {
        content.push_str("## Team\n\n");
        for member in doc.team.members() {
            content.push_str(&format!("* {} <{}>", member.name, member.email));
            if let Some(role) = &member.role {
                content.push_str(&format!(" - {role}"));
            }
            content.push('\n');
        }
    }
    if !doc.next.is_empty() {
        content.push_str("## Next\n\n");
        for id in &doc.next {
            content.push_str(&format!("- {id}\n"));
        }
        content.push('\n');
    }
    content
}

/// Replace one board file with a sharded board. The source stays recoverable until the
/// fully written and re-read destination has been published.
pub fn migrate_single_file(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() || is_sharded(source) {
        bail!("{} is already a sharded board", source.display());
    }
    if !source.is_file() {
        bail!("{} is not a board file", source.display());
    }
    if destination.exists() {
        bail!(
            "Refusing to migrate: destination {} already exists",
            destination.display()
        );
    }
    let doc = read(source)?;
    doc.validate_next()?;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".frump-migrate-{}-{nonce}", std::process::id()));
    fs::create_dir(&temporary)
        .with_context(|| format!("Failed to create {}", temporary.display()))?;
    let result = (|| {
        fs::create_dir(temporary.join("tasks"))?;
        fs::write(temporary.join("general.md"), "")?;
        write(&temporary, &doc)?;
        let migrated = read(&temporary)?;
        migrated.validate_next()?;
        if parser::serialize(&migrated) != parser::serialize(&doc) {
            bail!("Migrated board does not match the source board");
        }
        let backup = parent.join(format!(
            ".frump-migrate-source-{}-{nonce}",
            std::process::id()
        ));
        fs::rename(source, &backup)
            .with_context(|| format!("Failed to prepare {} for migration", source.display()))?;
        if let Err(error) = fs::rename(&temporary, destination) {
            let _ = fs::rename(&backup, source);
            return Err(error).with_context(|| {
                format!(
                    "Failed to publish migrated board at {}",
                    destination.display()
                )
            });
        }
        fs::remove_file(&backup).with_context(|| {
            format!(
                "Migrated board is ready, but failed to remove {}",
                backup.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

/// The canonical text of one task, as a directory board stores it in `tasks/<id>.md`.
pub fn serialize_task(task: &Task) -> String {
    let doc = FrumpDoc::new(
        String::new(),
        crate::Team::empty(),
        TaskCollection::new(vec![task.clone()]),
    );
    parser::serialize(&doc)
        .split_once("## Tasks\n\n")
        .map(|(_, tasks)| tasks)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TaskId, TaskType, Team};

    #[test]
    fn sharded_board_round_trips_tasks_as_individual_files() {
        let root = std::env::temp_dir().join(format!("frump-storage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tasks")).unwrap();
        fs::write(root.join("general.md"), "# Check\n\n## Team\n").unwrap();
        let mut task = Task::new(TaskId::new(1).unwrap(), TaskType::Task, "One".into());
        task.set_body("Evidence".into());
        let doc = FrumpDoc::new(
            "# Check\n\n".into(),
            Team::empty(),
            TaskCollection::new(vec![task]),
        );
        write(&root, &doc).unwrap();
        assert!(root.join("tasks/1.md").is_file());
        let loaded = read(&root).unwrap();
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks.tasks()[0].body, "Evidence");
        let _ = fs::remove_dir_all(root);
    }
}

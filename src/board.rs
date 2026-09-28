//! Rules shared by the CLI and the web board: board identity, input protection, task numbers,
//! prerequisite states and closing a task.

use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::git::BoardHistory;
use crate::{storage, FrumpDoc, PropertyKey, Task, TaskId};

/// The one path of a board: absolute and canonical. A directory board is its directory, whether
/// it was named by the directory or by its `general.md`. A board that does not exist yet keeps
/// its file name under the canonical path of its existing parent.
pub fn resolve_board(path: &Path) -> Result<PathBuf> {
    let resolved = resolve_path(path)?;
    if resolved
        .file_name()
        .is_some_and(|name| name == "general.md")
    {
        if let Some(root) = storage::sharded_root(&resolved) {
            return Ok(root);
        }
    }
    Ok(resolved)
}

/// The template file of a board: `.frump_templates.json` in the directory that contains it.
pub fn templates_file(board: &Path) -> PathBuf {
    board
        .parent()
        .unwrap_or(board)
        .join(".frump_templates.json")
}

/// Refuse `path` when it is the board, a file inside a directory board, or another name for
/// any board file (a symlink, or a hard link on Unix). `role` names the argument in the error.
pub fn ensure_outside_board(board: &Path, path: &Path, role: &str) -> Result<()> {
    let resolved = resolve_path(path)?;
    if resolved == board
        || (board.is_dir() && resolved.starts_with(board))
        || board_files(board)?
            .iter()
            .any(|file| same_file(file, &resolved))
    {
        bail!(
            "Refusing {role} {}: it is the task board {} or part of it.",
            path.display(),
            board.display()
        );
    }
    Ok(())
}

/// Every file of a board: the file itself, or every file under a board directory.
fn board_files(board: &Path) -> Result<Vec<PathBuf>> {
    if !board.is_dir() {
        return Ok(vec![board.to_path_buf()]);
    }
    let mut files = Vec::new();
    let mut directories = vec![board.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory)
            .with_context(|| format!("Failed to read {}", directory.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn resolve_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path
            .canonicalize()
            .with_context(|| format!("Failed to resolve {}", path.display()));
    }
    let name = path
        .file_name()
        .ok_or_else(|| anyhow!("{} has no file name", path.display()))?;
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => std::env::current_dir().context("Failed to determine the current directory")?,
    };
    Ok(parent
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", parent.display()))?
        .join(name))
}

#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.is_file() && a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_file(_: &Path, _: &Path) -> bool {
    false
}

/// `count` new task numbers above every number on the board and in its history.
pub fn allocate_task_ids(
    doc: &FrumpDoc,
    history: &BoardHistory,
    count: usize,
) -> Result<Vec<TaskId>> {
    let highest = doc.tasks.max_id().max(history.max_id());
    let mut next = match highest {
        Some(id) => id.next()?,
        None => TaskId::new(1)?,
    };
    let mut ids = Vec::with_capacity(count);
    for position in 0..count {
        ids.push(next);
        if position + 1 < count {
            next = next.next()?;
        }
    }
    Ok(ids)
}

/// One new task number above every number on the board and in its history.
pub fn next_task_id(doc: &FrumpDoc, history: &BoardHistory) -> Result<TaskId> {
    Ok(allocate_task_ids(doc, history, 1)?[0])
}

/// The state of one `Depends On` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prerequisite {
    /// On the board; satisfied when its Status is done.
    Active { id: TaskId, done: bool },
    /// Removed from the board and present in history; satisfied.
    Closed(TaskId),
    /// Never on the board or in history; not satisfied.
    Unknown(TaskId),
    /// Not a task number; not satisfied.
    Malformed(String),
}

impl Prerequisite {
    pub fn satisfied(&self) -> bool {
        matches!(
            self,
            Prerequisite::Active { done: true, .. } | Prerequisite::Closed(_)
        )
    }
}

fn depends_on_key() -> PropertyKey {
    PropertyKey::new("Depends On").expect("constant is valid")
}

/// The task numbers a task's `Depends On` names; entries that are not numbers are left out.
pub fn dependency_ids(task: &Task) -> Vec<TaskId> {
    prerequisites_raw(task)
        .into_iter()
        .filter_map(|raw| parse_task_id(&raw))
        .collect()
}

fn prerequisites_raw(task: &Task) -> Vec<String> {
    task.get_property(&depends_on_key())
        .map(|value| value.split(',').map(|raw| raw.trim().to_string()).collect())
        .unwrap_or_default()
}

fn parse_task_id(raw: &str) -> Option<TaskId> {
    raw.parse::<u32>().ok().and_then(|id| TaskId::new(id).ok())
}

/// Every `Depends On` entry of `task`, with its state.
pub fn prerequisites(task: &Task, doc: &FrumpDoc, history: &BoardHistory) -> Vec<Prerequisite> {
    prerequisites_raw(task)
        .into_iter()
        .map(|raw| match parse_task_id(&raw) {
            None => Prerequisite::Malformed(raw),
            Some(id) => match doc.tasks.find_by_id(id) {
                Some(found) => Prerequisite::Active {
                    id,
                    done: found.status() == Some("done"),
                },
                None if history.contains(id) => Prerequisite::Closed(id),
                None => Prerequisite::Unknown(id),
            },
        })
        .collect()
}

/// Whether every prerequisite of `task` is satisfied.
pub fn is_ready(task: &Task, doc: &FrumpDoc, history: &BoardHistory) -> bool {
    prerequisites(task, doc, history)
        .iter()
        .all(Prerequisite::satisfied)
}

/// Dependency errors of the whole board: malformed and unknown entries, self references and
/// cycles between tasks on the board.
pub fn validate_dependencies(doc: &FrumpDoc, history: &BoardHistory) -> Vec<String> {
    let mut errors = Vec::new();
    for task in doc.tasks.tasks() {
        for prerequisite in prerequisites(task, doc, history) {
            match prerequisite {
                Prerequisite::Malformed(raw) => errors.push(format!(
                    "Task {} has invalid Depends On value '{}'",
                    task.id, raw
                )),
                Prerequisite::Unknown(id) => errors.push(format!(
                    "Task {} references unknown dependency {}",
                    task.id, id
                )),
                Prerequisite::Active { id, .. } if id == task.id => {
                    errors.push(format!("Task {} depends on itself", task.id))
                }
                _ => {}
            }
        }
    }
    fn visit(
        id: TaskId,
        doc: &FrumpDoc,
        visiting: &mut std::collections::HashSet<TaskId>,
        visited: &mut std::collections::HashSet<TaskId>,
    ) -> bool {
        if visited.contains(&id) {
            return false;
        }
        if !visiting.insert(id) {
            return true;
        }
        let cyclic = doc
            .tasks
            .find_by_id(id)
            .map(|task| {
                dependency_ids(task)
                    .into_iter()
                    .filter(|dependency| doc.tasks.find_by_id(*dependency).is_some())
                    .any(|dependency| visit(dependency, doc, visiting, visited))
            })
            .unwrap_or(false);
        visiting.remove(&id);
        visited.insert(id);
        cyclic
    }
    let mut visiting = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    for task in doc.tasks.tasks() {
        if visit(task.id, doc, &mut visiting, &mut visited) {
            errors.push("Dependency cycle detected".to_string());
            break;
        }
    }
    errors
}

/// What closing one task means for its recovery, prepared before the board changes.
#[derive(Debug, Clone)]
pub struct CloseResult {
    pub task: Task,
    /// Set when history does not hold the task exactly as it is now.
    pub warning: Option<String>,
}

/// Check that every task in `ids` may close, and prepare each close result from the original
/// task. Nothing changes; the first refusal stops the whole set.
///
/// A task may close when its Status is done and every prerequisite is satisfied. Git state never
/// refuses a close: when HEAD lacks the task or has other text, the result carries a warning
/// with the complete task text. When no commit has the task at all, the warning also says its
/// number can be given out again and names the tasks that will depend on an unknown number.
pub fn prepare_close(
    doc: &FrumpDoc,
    history: &BoardHistory,
    ids: &[TaskId],
) -> Result<Vec<CloseResult>> {
    // A number held by two tasks names neither of them: refuse before anything changes.
    for &id in ids {
        let holders = doc
            .tasks
            .tasks()
            .iter()
            .filter(|task| task.id == id)
            .count();
        if holders > 1 {
            bail!(
                "Refusing to close task {id}: {holders} tasks have this number. Run \
                 `frump renumber-duplicates` first; nothing was closed."
            );
        }
    }
    let mut results = Vec::with_capacity(ids.len());
    for &id in ids {
        let task = doc
            .tasks
            .find_by_id(id)
            .ok_or_else(|| anyhow!("Task {} not found.", id))?;
        if task.status() != Some("done") {
            bail!("Refusing to close task {}: Status must be done.", id);
        }
        if let Some(open) = prerequisites(task, doc, history)
            .into_iter()
            .find(|prerequisite| !prerequisite.satisfied())
        {
            bail!(
                "Refusing to close task {}: {}.",
                id,
                match open {
                    Prerequisite::Active { id, .. } => format!("dependency {id} is not done"),
                    Prerequisite::Unknown(id) => format!("dependency {id} is unknown"),
                    Prerequisite::Malformed(raw) =>
                        format!("Depends On entry '{raw}' is not a task number"),
                    Prerequisite::Closed(_) => unreachable!("a closed prerequisite is satisfied"),
                }
            );
        }
        let text = storage::serialize_task(task);
        let committed = history
            .head_task(id)
            .is_some_and(|head| storage::serialize_task(head) == text);
        let warning = (!committed).then(|| {
            let mut warning = if !history.in_git() {
                format!(
                    "Task {id} is closed, but the board is not in Git, so this text is now gone."
                )
            } else if history.head_task(id).is_none() {
                format!("Task {id} is closed, but HEAD does not have this task.")
            } else {
                format!("Task {id} is closed, but HEAD has another version of it.")
            };
            if !history.contains(id) {
                warning.push_str(&format!(
                    " No commit has task {id}, so its number can be given out again."
                ));
                let dependents: Vec<String> = doc
                    .tasks
                    .tasks()
                    .iter()
                    .filter(|other| !ids.contains(&other.id) && dependency_ids(other).contains(&id))
                    .map(|other| other.id.to_string())
                    .collect();
                if !dependents.is_empty() {
                    warning.push_str(&format!(
                        " These tasks now depend on an unknown task: {}.",
                        dependents.join(", ")
                    ));
                }
            }
            warning.push_str(" To restore the task, paste this text back:\n\n");
            warning.push_str(text.trim_end());
            warning
        });
        results.push(CloseResult {
            task: task.clone(),
            warning,
        });
    }
    Ok(results)
}

/// Remove prepared tasks from the board and from the Next list.
pub fn apply_close(doc: &mut FrumpDoc, results: &[CloseResult]) {
    for result in results {
        doc.tasks.remove(result.task.id);
        doc.remove_from_next(result.task.id);
    }
}

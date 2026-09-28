use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use git2::{ErrorCode, ObjectType, Oid, Repository, Sort, Tree};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::domain::{Task, TaskId};
use crate::{parser, storage};

/// The task history of one board: every snapshot of that board in the ancestry of HEAD.
///
/// The repository is the one that contains the board, whatever the current directory is. A
/// single-file board is read at its own path. A directory board is read from `<dir>/tasks/*.md`;
/// in commits where `<dir>/general.md` does not exist yet, the sibling `<dir>.md` is read
/// instead, because `migrate` and `commit` treat it as the same board.
pub struct BoardHistory {
    in_git: bool,
    commits: Vec<CommitSnapshot>,
}

/// A historical snapshot of a task from git history
#[derive(Debug, Clone)]
pub struct TaskHistory {
    pub task_id: TaskId,
    pub commits: Vec<TaskCommit>,
}

/// A commit that affected a task
#[derive(Debug, Clone)]
pub struct TaskCommit {
    pub hash: String,
    pub author: String,
    pub date: DateTime<Utc>,
    pub message: String,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

type Snapshot = BTreeMap<TaskId, Task>;

struct CommitSnapshot {
    oid: Oid,
    parents: Vec<Oid>,
    author: String,
    date: DateTime<Utc>,
    message: String,
    tasks: Arc<Snapshot>,
}

enum Layout {
    File(PathBuf),
    Directory {
        dir: PathBuf,
        legacy: Option<PathBuf>,
    },
}

impl BoardHistory {
    /// Read the history of the board at `board`, a single file or a board directory.
    ///
    /// History holds task content only: task files, or the Tasks section of a single file. The
    /// header, Team and Next sections are not read, so a committed typo there cannot make the
    /// task numbers of that commit unreadable. A board outside git, or in a repository without
    /// commits, has an empty history. A failed object read, a board path of the wrong kind, or
    /// a task that does not parse is an error: an unreadable history must never pass for an
    /// empty one.
    pub fn load(board: &Path) -> Result<Self> {
        let board = crate::board::resolve_board(board)?;
        let board = board.as_path();
        let empty = |in_git| BoardHistory {
            in_git,
            commits: Vec::new(),
        };
        let start = if board.is_dir() {
            board
        } else {
            board.parent().unwrap_or(board)
        };
        let repo = match Repository::discover(start) {
            Ok(repo) => repo,
            Err(error) if error.code() == ErrorCode::NotFound => return Ok(empty(false)),
            Err(error) => return Err(error).context("Failed to open the board's Git repository"),
        };
        let Some(workdir) = repo.workdir() else {
            return Ok(empty(false));
        };
        let workdir = workdir
            .canonicalize()
            .with_context(|| format!("Failed to resolve {}", workdir.display()))?;
        let layout = match storage::sharded_root(board) {
            Some(root) => {
                let Ok(dir) = root.strip_prefix(&workdir) else {
                    return Ok(empty(false));
                };
                // A board directory at the repository root has no sibling inside the repository.
                let legacy = dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| dir.with_file_name(format!("{name}.md")));
                Layout::Directory {
                    dir: dir.to_path_buf(),
                    legacy,
                }
            }
            None => match board.strip_prefix(&workdir) {
                Ok(path) => Layout::File(path.to_path_buf()),
                Err(_) => return Ok(empty(false)),
            },
        };
        match repo.head() {
            Ok(_) => {}
            Err(error) if error.code() == ErrorCode::UnbornBranch => return Ok(empty(true)),
            Err(error) => return Err(error).context("Failed to read HEAD"),
        }

        let mut reader = SnapshotReader {
            repo: &repo,
            layout,
            snapshots: HashMap::new(),
            task_files: HashMap::new(),
        };
        let mut walk = repo.revwalk()?;
        walk.push_head()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        let mut commits = Vec::new();
        for oid in walk {
            let commit = repo.find_commit(oid?)?;
            let tasks = reader
                .snapshot(&commit.tree()?)
                .with_context(|| format!("Failed to read the board in commit {}", commit.id()))?;
            let author = commit.author().name().unwrap_or("Unknown").to_string();
            let date = DateTime::from_timestamp(commit.time().seconds(), 0)
                .ok_or_else(|| anyhow!("Invalid timestamp in commit {}", commit.id()))?;
            commits.push(CommitSnapshot {
                oid: commit.id(),
                parents: commit.parent_ids().collect(),
                author,
                date,
                message: commit.message().unwrap_or("").trim().to_string(),
                tasks,
            });
        }
        Ok(BoardHistory {
            in_git: true,
            commits,
        })
    }

    /// Whether the board is inside a Git work tree.
    pub fn in_git(&self) -> bool {
        self.in_git
    }

    /// The highest task number any commit of the board has used.
    pub fn max_id(&self) -> Option<TaskId> {
        self.commits
            .iter()
            .filter_map(|commit| commit.tasks.keys().next_back().copied())
            .max()
    }

    /// Whether any commit of the board has task `id`.
    pub fn contains(&self, id: TaskId) -> bool {
        self.commits
            .iter()
            .any(|commit| commit.tasks.contains_key(&id))
    }

    /// Task `id` as HEAD has it.
    pub fn head_task(&self, id: TaskId) -> Option<&Task> {
        self.commits.first()?.tasks.get(&id)
    }

    /// The last state of task `id`: its state in the first commit that has it, walking from HEAD
    /// through its ancestry in topological order (newest commit time first among branches).
    pub fn last_state(&self, id: TaskId) -> Option<&Task> {
        self.commits.iter().find_map(|commit| commit.tasks.get(&id))
    }

    /// Tasks that history has and the current board does not, by number, in their last state.
    pub fn removed_tasks(&self, current: &crate::FrumpDoc) -> Vec<&Task> {
        let mut ids: Vec<TaskId> = self
            .commits
            .iter()
            .flat_map(|commit| commit.tasks.keys().copied())
            .filter(|id| current.tasks.find_by_id(*id).is_none())
            .collect();
        ids.sort();
        ids.dedup();
        ids.into_iter()
            .filter_map(|id| self.last_state(id))
            .collect()
    }

    /// The commits that changed task `id`, oldest first.
    ///
    /// A commit changed the task when the task's text there differs from its text in every
    /// parent; a root commit compares with an empty board. The kind compares with the first
    /// parent. An unrelated commit, or one that changes another task, is not an event.
    pub fn task_history(&self, id: TaskId) -> TaskHistory {
        let index: HashMap<Oid, usize> = self
            .commits
            .iter()
            .enumerate()
            .map(|(position, commit)| (commit.oid, position))
            .collect();
        let text = |position: usize| {
            self.commits[position]
                .tasks
                .get(&id)
                .map(storage::serialize_task)
        };
        let mut commits = Vec::new();
        for position in (0..self.commits.len()).rev() {
            let commit = &self.commits[position];
            let current = text(position);
            let parents: Vec<Option<String>> = commit
                .parents
                .iter()
                .map(|parent| index.get(parent).and_then(|&position| text(position)))
                .collect();
            let first_parent = parents.first().cloned().flatten();
            let changed = if parents.is_empty() {
                current.is_some()
            } else {
                parents.iter().all(|parent| *parent != current)
            };
            if !changed {
                continue;
            }
            let change_type = match (&first_parent, &current) {
                (None, Some(_)) => ChangeType::Created,
                (Some(_), None) => ChangeType::Deleted,
                _ => ChangeType::Modified,
            };
            commits.push(TaskCommit {
                hash: commit.oid.to_string(),
                author: commit.author.clone(),
                date: commit.date,
                message: commit.message.clone(),
                change_type,
            });
        }
        TaskHistory {
            task_id: id,
            commits,
        }
    }
}

struct SnapshotReader<'repo> {
    repo: &'repo Repository,
    layout: Layout,
    /// Parsed snapshots by the object id of a board file or a tasks directory.
    snapshots: HashMap<Oid, Arc<Snapshot>>,
    /// Parsed task files by blob id, so each distinct task file is parsed once.
    task_files: HashMap<Oid, Task>,
}

impl SnapshotReader<'_> {
    fn snapshot(&mut self, tree: &Tree) -> Result<Arc<Snapshot>> {
        match &self.layout {
            Layout::File(path) => {
                let path = path.clone();
                self.board_file(tree, &path)
            }
            Layout::Directory { dir, legacy } => {
                let (dir, legacy) = (dir.clone(), legacy.clone());
                // general.md marks the directory layout; before it exists, the board was the
                // sibling single file (if any).
                match entry(tree, &dir.join("general.md"))? {
                    None => {
                        return match legacy {
                            Some(legacy) => self.board_file(tree, &legacy),
                            None => Ok(Arc::default()),
                        }
                    }
                    Some(marker) if marker.kind() != Some(ObjectType::Blob) => {
                        bail!("{} is not a file", dir.join("general.md").display())
                    }
                    Some(_) => {}
                }
                let Some(tasks) = entry(tree, &dir.join("tasks"))? else {
                    return Ok(Arc::default());
                };
                if tasks.kind() != Some(ObjectType::Tree) {
                    bail!("{} is not a directory", dir.join("tasks").display());
                }
                if let Some(snapshot) = self.snapshots.get(&tasks.id()) {
                    return Ok(snapshot.clone());
                }
                let tasks_tree = self.repo.find_tree(tasks.id())?;
                let mut snapshot = Snapshot::new();
                for file in tasks_tree.iter() {
                    let Some(name) = file.name() else { continue };
                    if !name.ends_with(".md") {
                        continue;
                    }
                    if file.kind() != Some(ObjectType::Blob) {
                        bail!("Task path tasks/{name} is not a file");
                    }
                    let task = match self.task_files.get(&file.id()) {
                        Some(task) => task.clone(),
                        None => {
                            let fragment = self.blob_text(file.id(), name)?;
                            let mut parsed =
                                parser::parse_tasks(&format!("## Tasks\n\n{fragment}"))
                                    .with_context(|| format!("Failed to parse task file {name}"))?;
                            if parsed.len() != 1 {
                                bail!("Task file {name} must contain exactly one task");
                            }
                            let task = parsed.remove(0);
                            self.task_files.insert(file.id(), task.clone());
                            task
                        }
                    };
                    snapshot.insert(task.id, task);
                }
                let snapshot = Arc::new(snapshot);
                self.snapshots.insert(tasks.id(), snapshot.clone());
                Ok(snapshot)
            }
        }
    }

    fn board_file(&mut self, tree: &Tree, path: &Path) -> Result<Arc<Snapshot>> {
        let Some(file) = entry(tree, path)? else {
            return Ok(Arc::default());
        };
        if file.kind() != Some(ObjectType::Blob) {
            bail!("{} is not a file", path.display());
        }
        if let Some(snapshot) = self.snapshots.get(&file.id()) {
            return Ok(snapshot.clone());
        }
        let content = self.blob_text(file.id(), &path.display().to_string())?;
        let tasks = parser::parse_tasks(&content)
            .with_context(|| format!("Failed to parse the tasks of {}", path.display()))?;
        let mut snapshot = Snapshot::new();
        for task in &tasks {
            snapshot.entry(task.id).or_insert_with(|| task.clone());
        }
        let snapshot = Arc::new(snapshot);
        self.snapshots.insert(file.id(), snapshot.clone());
        Ok(snapshot)
    }

    fn blob_text(&self, oid: Oid, name: &str) -> Result<String> {
        let blob = self.repo.find_blob(oid)?;
        String::from_utf8(blob.content().to_vec())
            .with_context(|| format!("{name} is not valid UTF-8"))
    }
}

/// The tree entry at `path`, or `None` when the path does not exist in this commit.
fn entry(tree: &Tree, path: &Path) -> Result<Option<git2::TreeEntry<'static>>> {
    match tree.get_path(path) {
        Ok(entry) => Ok(Some(entry)),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

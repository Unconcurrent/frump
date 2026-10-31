use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use git2::{ErrorCode, ObjectType, Oid, Repository, Sort, Tree};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use crate::domain::{Task, TaskId};
use crate::{parser, storage};

/// The task history of one board: latest task states and compact changes through HEAD ancestry.
///
/// The repository is the one that contains the board, whatever the current directory is. A
/// single-file board is read at its own path. A directory board is read from `<dir>/tasks/*.md`;
/// in commits where `<dir>/general.md` does not exist yet, the sibling `<dir>.md` is read
/// instead, because `migrate` and `commit` treat it as the same board.
pub struct BoardHistory {
    in_git: bool,
    head: Arc<Snapshot>,
    latest: Snapshot,
    commits: Vec<CommitChanges>,
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

type Snapshot = BTreeMap<TaskId, Arc<Task>>;

struct CommitChanges {
    oid: Oid,
    author: String,
    date: DateTime<Utc>,
    message: String,
    changes: BTreeMap<TaskId, ChangeType>,
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
    /// commits, has an empty history. Malformed historical headings are tolerated while valid
    /// task IDs are retained. Object-read errors and paths of the wrong kind remain errors.
    pub fn load(board: &Path) -> Result<Self> {
        let board = crate::board::resolve_board(board)?;
        let board = board.as_path();
        let empty = |in_git| BoardHistory {
            in_git,
            head: Arc::default(),
            latest: Snapshot::new(),
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
            snapshot: None,
            task_files: HashMap::new(),
        };
        let mut walk = repo.revwalk()?;
        walk.push_head()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        let mut commits = Vec::new();
        let mut head = None;
        let mut latest = Snapshot::new();
        for oid in walk {
            let commit = repo.find_commit(oid?)?;
            let tasks = reader
                .snapshot(&commit.tree()?)
                .with_context(|| format!("Failed to read the board in commit {}", commit.id()))?;
            head.get_or_insert_with(|| tasks.clone());
            // Topological/time order also defines recovery of closed tasks. Keep only the
            // first (latest) body for each identity, never every body in every revision.
            for (&id, task) in tasks.iter() {
                latest.entry(id).or_insert_with(|| task.clone());
            }
            let mut parents = Vec::with_capacity(commit.parent_count());
            for parent in commit.parents() {
                parents.push(reader.snapshot(&parent.tree()?).with_context(|| {
                    format!("Failed to read the board in commit {}", parent.id())
                })?);
            }
            let date = DateTime::from_timestamp(commit.time().seconds(), 0)
                .ok_or_else(|| anyhow!("Invalid timestamp in commit {}", commit.id()))?;
            let changes = snapshot_changes(&tasks, &parents);
            if !changes.is_empty() {
                commits.push(CommitChanges {
                    oid: commit.id(),
                    author: commit.author().name().unwrap_or("Unknown").to_string(),
                    date,
                    message: commit.message().unwrap_or("").trim().to_string(),
                    changes,
                });
            }
        }
        Ok(BoardHistory {
            in_git: true,
            head: head.unwrap_or_default(),
            latest,
            commits,
        })
    }

    /// Whether the board is inside a Git work tree.
    pub fn in_git(&self) -> bool {
        self.in_git
    }

    /// The highest task number any commit of the board has used.
    pub fn max_id(&self) -> Option<TaskId> {
        self.latest.keys().next_back().copied()
    }

    /// Whether any commit of the board has task `id`.
    pub fn contains(&self, id: TaskId) -> bool {
        self.latest.contains_key(&id)
    }

    /// Task `id` as HEAD has it.
    pub fn head_task(&self, id: TaskId) -> Option<&Task> {
        self.head.get(&id).map(Arc::as_ref)
    }

    /// The last state in topological/time order, including closed tasks.
    pub fn last_state(&self, id: TaskId) -> Option<&Task> {
        self.latest.get(&id).map(Arc::as_ref)
    }

    /// Tasks absent from the current board, in their last historical state.
    pub fn removed_tasks(&self, current: &crate::FrumpDoc) -> Vec<&Task> {
        self.latest
            .iter()
            .filter(|(id, _)| current.tasks.find_by_id(**id).is_none())
            .map(|(_, task)| task.as_ref())
            .collect()
    }

    /// The commits that changed task `id`, oldest first. A merge is an event only when its
    /// task differs from every parent; removal and identical recreation remain distinct.
    pub fn task_history(&self, id: TaskId) -> TaskHistory {
        let commits = self
            .commits
            .iter()
            .rev()
            .filter_map(|commit| {
                commit.changes.get(&id).map(|change_type| TaskCommit {
                    hash: commit.oid.to_string(),
                    author: commit.author.clone(),
                    date: commit.date,
                    message: commit.message.clone(),
                    change_type: change_type.clone(),
                })
            })
            .collect();
        TaskHistory {
            task_id: id,
            commits,
        }
    }
}

fn snapshot_changes(
    tasks: &Arc<Snapshot>,
    parents: &[Arc<Snapshot>],
) -> BTreeMap<TaskId, ChangeType> {
    if parents.iter().any(|parent| Arc::ptr_eq(tasks, parent)) {
        return BTreeMap::new();
    }
    let mut ids: BTreeSet<_> = tasks.keys().copied().collect();
    for parent in parents {
        ids.extend(parent.keys().copied());
    }
    let mut changes = BTreeMap::new();
    for id in ids {
        let current = tasks.get(&id);
        if parents
            .iter()
            .any(|parent| match (current, parent.get(&id)) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    Arc::ptr_eq(a, b)
                        || (a.id == b.id
                            && a.task_type == b.task_type
                            && a.subject == b.subject
                            && a.body == b.body
                            && a.properties == b.properties)
                        || storage::serialize_task(a) == storage::serialize_task(b)
                }
                _ => false,
            })
        {
            continue;
        }
        let previous = parents.first().and_then(|parent| parent.get(&id));
        let kind = match (previous, current) {
            (None, Some(_)) => ChangeType::Created,
            (Some(_), None) => ChangeType::Deleted,
            _ => ChangeType::Modified,
        };
        changes.insert(id, kind);
    }
    changes
}

struct SnapshotReader<'repo> {
    repo: &'repo Repository,
    layout: Layout,
    /// The immediately preceding source (normally the next commit in a linear walk).
    /// Holding all source snapshots multiplies the board body by its history length.
    snapshot: Option<(Oid, Arc<Snapshot>)>,
    /// Share live task bodies across adjacent sharded snapshots without retaining old bodies.
    task_files: HashMap<Oid, Vec<Weak<Task>>>,
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
                if let Some((id, snapshot)) = &self.snapshot {
                    if *id == tasks.id() {
                        return Ok(snapshot.clone());
                    }
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
                    let cached: Option<Vec<_>> = self
                        .task_files
                        .get(&file.id())
                        .and_then(|tasks| tasks.iter().map(Weak::upgrade).collect());
                    let parsed = match cached {
                        Some(tasks) => tasks,
                        None => {
                            let fragment = self.blob_text(file.id(), name)?;
                            let parsed: Vec<_> =
                                parser::parse_tasks(&format!("## Tasks\n\n{fragment}"))
                                    .with_context(|| format!("Failed to parse task file {name}"))?
                                    .into_iter()
                                    .map(Arc::new)
                                    .collect();
                            self.task_files
                                .insert(file.id(), parsed.iter().map(Arc::downgrade).collect());
                            parsed
                        }
                    };
                    for task in parsed {
                        snapshot.entry(task.id).or_insert(task);
                    }
                }
                let snapshot = Arc::new(snapshot);
                self.snapshot = Some((tasks.id(), snapshot.clone()));
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
        if let Some((id, snapshot)) = &self.snapshot {
            if *id == file.id() {
                return Ok(snapshot.clone());
            }
        }
        let content = self.blob_text(file.id(), &path.display().to_string())?;
        let tasks = parser::parse_tasks(&content)
            .with_context(|| format!("Failed to parse the tasks of {}", path.display()))?;
        let mut snapshot = Snapshot::new();
        for task in tasks {
            snapshot.entry(task.id).or_insert_with(|| Arc::new(task));
        }
        let snapshot = Arc::new(snapshot);
        self.snapshot = Some((file.id(), snapshot.clone()));
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

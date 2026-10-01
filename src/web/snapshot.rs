//! Web read model: bounded card previews, lazily loaded bodies and one retained change set.
//! File metadata invalidates the cache; no content fingerprint or second persistent authority.
use super::*;
use axum::body::Body;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, PartialEq, Eq)]
struct FileStamp {
    path: PathBuf,
    modified: SystemTime,
    len: u64,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

fn file_stamps(file: &FsPath) -> Result<Vec<FileStamp>> {
    let paths = if let Some(root) = storage::sharded_root(file) {
        let mut paths = vec![root.join("general.md")];
        for entry in fs::read_dir(root.join("tasks"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "md") {
                paths.push(path);
            }
        }
        paths.sort();
        paths
    } else {
        vec![file.to_path_buf()]
    };
    paths
        .into_iter()
        .map(|path| {
            let metadata = fs::metadata(&path)?;
            #[cfg(unix)]
            let identity = {
                use std::os::unix::fs::MetadataExt;
                (
                    metadata.dev(),
                    metadata.ino(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                )
            };
            Ok(FileStamp {
                path,
                modified: metadata.modified()?,
                len: metadata.len(),
                #[cfg(unix)]
                identity,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
struct TaskSummary {
    id: u32,
    task_type: String,
    subject: String,
    excerpt: String,
    properties: Vec<PropertyDto>,
    revision: u64,
}

#[derive(Serialize)]
struct BoardUpdate<'a> {
    revision: u64,
    reset: bool,
    header: &'a str,
    team: &'a [TeamMemberDto],
    next: &'a [u32],
    tasks: Vec<&'a TaskSummary>,
    removed: &'a [u32],
    order: Option<&'a [u32]>,
}

struct CachedTask {
    dto: TaskDto,
    summary: TaskSummary,
    // Fold once when the task changes, rather than allocating its whole body for each search.
    search: String,
}

#[derive(Default)]
pub(super) struct BoardCache {
    stamps: Vec<FileStamp>,
    revision: u64,
    previous_revision: u64,
    header: String,
    team: Vec<TeamMemberDto>,
    next: Vec<u32>,
    tasks: BTreeMap<u32, CachedTask>,
    changed: Vec<u32>,
    removed: Vec<u32>,
    order: Vec<u32>,
    order_changed: bool,
    duplicate_document: Option<Vec<TaskDto>>,
}

impl BoardCache {
    pub fn refresh(&mut self, file: &FsPath) -> Result<()> {
        let lock_path = storage::sharded_root(file)
            .map(|root| root.join("general.md"))
            .unwrap_or_else(|| file.to_path_buf());
        let lock = fs::File::open(lock_path)?;
        FileExt::lock_shared(&lock)?;
        let mut stamps = file_stamps(file)?;
        if self.revision != 0 && self.stamps == stamps {
            return Ok(());
        }
        // Do not publish a snapshot read across a writer's changes. A busy writer gets a
        // retriable response; the last good snapshot remains intact, never cached as current.
        let mut parsed = None;
        for _ in 0..3 {
            let doc = read_parsed_document(file)?;
            let after = file_stamps(file)?;
            if stamps == after {
                parsed = Some(doc);
                break;
            }
            stamps = after;
        }
        let doc =
            parsed.ok_or_else(|| anyhow::anyhow!("The board is changing. Please try again."))?;
        let document = document_to_dto(&doc);
        let next: Vec<_> = doc.next.iter().map(|id| id.value()).collect();
        let order: Vec<_> = document.tasks.iter().map(|task| task.id).collect();
        let changed: Vec<_> = document
            .tasks
            .iter()
            .filter(|task| {
                self.tasks
                    .get(&task.id)
                    .is_none_or(|cached| cached.dto != **task)
            })
            .map(|task| task.id)
            .collect();
        let incoming: BTreeSet<_> = order.iter().copied().collect();
        let duplicate_document = (incoming.len() != order.len()).then(|| document.tasks.clone());
        let removed: Vec<_> = self
            .tasks
            .keys()
            .filter(|id| !incoming.contains(id))
            .copied()
            .collect();
        if self.revision != 0
            && changed.is_empty()
            && removed.is_empty()
            && self.header == document.header
            && self.team == document.team
            && self.next == next
            && self.order == order
        {
            self.stamps = stamps;
            return Ok(());
        }
        self.previous_revision = self.revision;
        self.revision = if self.revision == 0 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_micros()
                .try_into()?
        } else {
            self.revision
                .checked_add(1)
                .context("Board revision exhausted")?
        };
        for task in document.tasks {
            if self
                .tasks
                .get(&task.id)
                .is_some_and(|cached| cached.dto == task)
            {
                continue;
            }
            let summary = TaskSummary {
                id: task.id,
                task_type: task.task_type.clone(),
                subject: task.subject.clone(),
                excerpt: task.body.chars().take(240).collect(),
                properties: task.properties.clone(),
                revision: self.revision,
            };
            let search = format!(
                "#{}\n{}\n{}\n{}\n{}",
                task.id,
                task.task_type,
                task.subject,
                task.body,
                task.properties
                    .iter()
                    .map(|p| format!("{}: {}", p.key, p.value))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
            .to_lowercase();
            self.tasks.insert(
                task.id,
                CachedTask {
                    dto: task,
                    summary,
                    search,
                },
            );
        }
        for id in &removed {
            self.tasks.remove(id);
        }
        self.header = document.header;
        self.team = document.team;
        self.next = next;
        self.order_changed = self.order != order;
        self.duplicate_document = duplicate_document;
        self.order = order;
        self.changed = changed;
        self.removed = removed;
        self.stamps = stamps;
        Ok(())
    }

    pub fn board_response(&self, since: Option<u64>) -> Result<Response> {
        if self.duplicate_document.is_some() {
            anyhow::bail!("The board has duplicate task numbers. Run frump renumber-duplicates before editing it in the browser.");
        }
        if since == Some(self.revision) {
            return response(StatusCode::NOT_MODIFIED, self.revision, Body::empty());
        }
        let reset = since != Some(self.previous_revision) || self.previous_revision == 0;
        let ids = if reset { &self.order } else { &self.changed };
        let update = BoardUpdate {
            revision: self.revision,
            reset,
            header: &self.header,
            team: &self.team,
            next: &self.next,
            tasks: ids
                .iter()
                .filter_map(|id| self.tasks.get(id).map(|task| &task.summary))
                .collect(),
            removed: if reset { &[] } else { &self.removed },
            order: (reset || self.order_changed).then_some(self.order.as_slice()),
        };
        response(
            StatusCode::OK,
            self.revision,
            Body::from(serde_json::to_vec(&update)?),
        )
    }

    pub fn document_response(&self, headers: &HeaderMap) -> Result<Response> {
        if unchanged(headers, self.revision) {
            return response(StatusCode::NOT_MODIFIED, self.revision, Body::empty());
        }
        let document = DocumentDto {
            header: self.header.clone(),
            team: self.team.clone(),
            tasks: self.duplicate_document.clone().unwrap_or_else(|| {
                self.order
                    .iter()
                    .filter_map(|id| self.tasks.get(id).map(|task| task.dto.clone()))
                    .collect()
            }),
        };
        response(
            StatusCode::OK,
            self.revision,
            Body::from(serde_json::to_vec(&document)?),
        )
    }

    pub fn task_response(&self, id: u32, headers: &HeaderMap) -> Result<Response> {
        if self.duplicate_document.is_some() {
            anyhow::bail!("The board has duplicate task numbers. Run frump renumber-duplicates before editing it in the browser.");
        }
        let Some(task) = self.tasks.get(&id) else {
            return Ok((StatusCode::NOT_FOUND, "Task not found").into_response());
        };
        if unchanged(headers, task.summary.revision) {
            return response(
                StatusCode::NOT_MODIFIED,
                task.summary.revision,
                Body::empty(),
            );
        }
        response(
            StatusCode::OK,
            task.summary.revision,
            Body::from(serde_json::to_vec(&task.dto)?),
        )
    }

    pub fn search(&self, query: &str) -> Vec<u32> {
        let folded = query.to_lowercase();
        let terms: Vec<_> = folded.split_whitespace().collect();
        self.order
            .iter()
            .filter(|id| {
                self.tasks
                    .get(id)
                    .is_some_and(|task| terms.iter().all(|term| task.search.contains(term)))
            })
            .copied()
            .collect()
    }
}

fn unchanged(headers: &HeaderMap, revision: u64) -> bool {
    let expected = format!("\"{revision}\"");
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|tag| {
                tag.trim() == expected
                    || tag.trim() == "*"
                    || tag.trim().strip_prefix("W/") == Some(expected.as_str())
            })
        })
}

fn response(status: StatusCode, revision: u64, body: Body) -> Result<Response> {
    Ok(Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::ETAG, format!("\"{revision}\""))
        .body(body)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct BoardFixture(PathBuf);
    impl BoardFixture {
        fn new() -> Self {
            static NUMBER: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "frump-web-cache-{}-{}",
                std::process::id(),
                NUMBER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn file(&self) -> PathBuf {
            self.0.join("board.md")
        }
        fn write(&self, body: &str) {
            fs::write(self.file(), format!("# Cache checks\n\n## Tasks\n\n### Task 1 - First\n\n{body}\n\nStatus: open\n\n### Task 2 - Second\n\nOther body.\n\nStatus: done\n")).unwrap();
        }
    }
    impl Drop for BoardFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    async fn json(response: Response) -> serde_json::Value {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn unchanged_large_documents_and_tasks_return_empty_conditional_responses() {
        let fixture = BoardFixture::new();
        fixture.write(&"Evidence. ".repeat(200_000));
        let mut cache = BoardCache::default();
        cache.refresh(&fixture.file()).unwrap();
        let revision = cache.revision;
        let summary = json(cache.board_response(None).unwrap()).await;
        assert_eq!(
            summary["tasks"][0]["excerpt"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            240
        );
        assert!(summary["tasks"][0].get("body").is_none());
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            format!("\"{revision}\"").parse().unwrap(),
        );
        cache.refresh(&fixture.file()).unwrap();
        assert_eq!(cache.revision, revision);
        for response in [
            cache.board_response(Some(revision)).unwrap(),
            cache.document_response(&headers).unwrap(),
            cache.task_response(1, &headers).unwrap(),
        ] {
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
            assert!(to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty());
        }
    }

    #[tokio::test]
    async fn same_length_body_edits_emit_only_the_changed_task_and_search_the_complete_body() {
        let fixture = BoardFixture::new();
        fixture.write("Earlier evidence");
        let mut cache = BoardCache::default();
        cache.refresh(&fixture.file()).unwrap();
        let revision = cache.revision;
        let second_revision = cache.tasks[&2].summary.revision;
        fixture.write("Updated evidence");
        cache.refresh(&fixture.file()).unwrap();
        let delta = json(cache.board_response(Some(revision)).unwrap()).await;
        assert_eq!(delta["reset"], false);
        assert_eq!(delta["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(delta["tasks"][0]["id"], 1);
        assert!(delta["order"].is_null());
        assert_eq!(cache.tasks[&2].summary.revision, second_revision);
        assert_eq!(cache.search("updated evidence"), [1]);
        assert!(cache.search("earlier").is_empty());
    }

    #[tokio::test]
    async fn missed_revisions_reset_and_removed_tasks_disappear_from_search() {
        let fixture = BoardFixture::new();
        fixture.write("First evidence");
        let mut cache = BoardCache::default();
        cache.refresh(&fixture.file()).unwrap();
        let first = cache.revision;
        fixture.write("Second evidence");
        cache.refresh(&fixture.file()).unwrap();
        let second = cache.revision;
        fs::write(
            fixture.file(),
            "# Cache checks\n\n## Tasks\n\n### Task 2 - Second\n\nOther body.\n\nStatus: done\n",
        )
        .unwrap();
        cache.refresh(&fixture.file()).unwrap();
        let delta = json(cache.board_response(Some(second)).unwrap()).await;
        assert_eq!(delta["removed"], serde_json::json!([1]));
        assert_eq!(delta["order"], serde_json::json!([2]));
        let reset = json(cache.board_response(Some(first)).unwrap()).await;
        assert_eq!(reset["reset"], true);
        assert_eq!(reset["tasks"].as_array().unwrap().len(), 1);
        assert!(cache.search("evidence").is_empty());
    }

    #[tokio::test]
    async fn sharded_task_changes_additions_and_removals_invalidate_the_snapshot() {
        let fixture = BoardFixture::new();
        let root = fixture.0.join("board");
        fs::create_dir_all(root.join("tasks")).unwrap();
        fs::write(root.join("general.md"), "# Directory board\n").unwrap();
        fs::write(
            root.join("tasks/1.md"),
            "### Task 1 - First\n\nBefore\n\nStatus: open\n",
        )
        .unwrap();
        let mut cache = BoardCache::default();
        cache.refresh(&root).unwrap();
        let revision = cache.revision;
        fs::write(
            root.join("tasks/1.md"),
            "### Task 1 - First\n\nAfter!\n\nStatus: open\n",
        )
        .unwrap();
        fs::write(
            root.join("tasks/2.md"),
            "### Task 2 - New\n\nAdded\n\nStatus: open\n",
        )
        .unwrap();
        cache.refresh(&root).unwrap();
        let added = json(cache.board_response(Some(revision)).unwrap()).await;
        assert_eq!(added["tasks"].as_array().unwrap().len(), 2);
        let revision = cache.revision;
        fs::remove_file(root.join("tasks/1.md")).unwrap();
        cache.refresh(&root).unwrap();
        let removed = json(cache.board_response(Some(revision)).unwrap()).await;
        assert_eq!(removed["removed"], serde_json::json!([1]));
        assert!(cache.search("after").is_empty());
    }

    #[test]
    fn invalid_edits_are_not_cached_and_recovery_reads_the_correct_version() {
        let fixture = BoardFixture::new();
        fixture.write("Valid evidence");
        let mut cache = BoardCache::default();
        cache.refresh(&fixture.file()).unwrap();
        let revision = cache.revision;
        fs::remove_file(fixture.file()).unwrap();
        assert!(cache.refresh(&fixture.file()).is_err());
        assert_eq!(cache.revision, revision);
        fixture.write("Recovered evidence");
        cache.refresh(&fixture.file()).unwrap();
        assert_eq!(cache.search("recovered"), [1]);
    }

    #[tokio::test]
    async fn legacy_document_reads_preserve_duplicates_while_the_editor_refuses_ambiguous_numbers()
    {
        let fixture = BoardFixture::new();
        fs::write(fixture.file(), "# Duplicates\n\n## Tasks\n\n### Task 1 - First\n\nOne body\n\n### Task 1 - Second\n\nAnother body\n").unwrap();
        let mut cache = BoardCache::default();
        cache.refresh(&fixture.file()).unwrap();
        let document = json(cache.document_response(&HeaderMap::new()).unwrap()).await;
        assert_eq!(document["tasks"][0]["subject"], "First");
        assert_eq!(document["tasks"][1]["subject"], "Second");
        assert!(cache.board_response(None).is_err());
        assert!(cache.task_response(1, &HeaderMap::new()).is_err());
    }
}

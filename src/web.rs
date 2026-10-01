use anyhow::{Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
};

mod snapshot;
use snapshot::BoardCache;

use crate::board::{apply_close, next_task_id, prepare_close};
use crate::{
    announce_assignment, mark_updated, notification_warning, now_utc, send_metateam_message,
    storage, validate_property_value, BoardHistory, FrumpDoc, PropertyKey, Task, TaskId, TaskType,
};

#[derive(Clone)]
struct AppState {
    file: Arc<PathBuf>,
    cache: Arc<Mutex<BoardCache>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DocumentDto {
    header: String,
    team: Vec<TeamMemberDto>,
    tasks: Vec<TaskDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct TeamMemberDto {
    name: String,
    email: String,
    role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct TaskDto {
    id: u32,
    task_type: String,
    subject: String,
    body: String,
    properties: Vec<PropertyDto>,
}

/// A saved task plus the warning of a notification that could not be delivered.
#[derive(Debug, Serialize)]
struct SavedTaskDto {
    #[serde(flatten)]
    task: TaskDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
}

/// A closed task's number plus the warning that HEAD does not hold its current text.
#[derive(Debug, Serialize)]
struct ClosedTaskDto {
    id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PropertyDto {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct TaskInput {
    task_type: String,
    subject: String,
    body: String,
    properties: Vec<PropertyDto>,
    /// Optional compare-and-save baseline used by the browser. Older API clients remain valid.
    expected: Option<ExpectedTask>,
}

#[derive(Debug, Deserialize)]
struct ExpectedTask {
    task_type: String,
    subject: String,
    body: String,
    properties: Vec<PropertyDto>,
}

impl ExpectedTask {
    fn matches(&self, task: &Task) -> bool {
        self.task_type == task.task_type.as_str()
            && self.subject == task.subject
            && self.body == task.body
            && self.properties.len() == task.properties.len()
            && self
                .properties
                .iter()
                .zip(&task.properties)
                .all(|(expected, actual)| {
                    expected.key == actual.key.as_str() && expected.value == actual.value
                })
    }
}

#[derive(Debug, Deserialize)]
struct NotifyInput {
    recipient: String,
    message: String,
}

#[derive(Debug)]
struct ApiError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.0.to_string()).into_response()
    }
}

type ApiResult<T> = std::result::Result<T, ApiError>;

/// Serve a self-contained board on loopback. Cached snapshots are invalidated by board-file
/// metadata, including individual task files on directory boards. Markdown remains authoritative.
pub async fn serve(file: PathBuf, port: u16) -> Result<()> {
    // One board identity, whatever path spelling the caller used.
    let state = AppState {
        file: Arc::new(crate::board::resolve_board(&file)?),
        cache: Arc::new(Mutex::new(BoardCache::default())),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/assets/app.js", get(javascript))
        .route("/assets/app.css", get(stylesheet))
        .route("/api/document", get(document))
        .route("/api/board", get(board))
        .route("/api/search", get(search))
        .route("/api/tasks", post(create_task))
        .route(
            "/api/tasks/{id}",
            get(task).put(update_task).delete(delete_task),
        )
        .route("/api/tasks/{id}/notify", post(notify_task))
        // Task bodies have the same size semantics as CLI and Markdown edits.
        .layer(DefaultBodyLimit::disable())
        .with_state(state);
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("Failed to bind local web server at http://{address}"))?;
    println!("Frump board: http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .await
        .context("Local web server stopped unexpectedly")
}

async fn index() -> Html<&'static str> {
    Html(include_str!("web/dist/index.html"))
}

async fn javascript() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("web/dist/assets/app.js"),
    )
}

async fn stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("web/dist/assets/app.css"),
    )
}

/// Filesystem and parsing work stays off Tokio's request workers. A shared snapshot prevents
/// each browser tab from independently reading, parsing and serializing an unchanged board.
async fn cached<T: Send + 'static>(
    state: AppState,
    read: impl FnOnce(&BoardCache) -> Result<T> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(move || {
        let mut cache = state
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("Board cache lock failed"))?;
        cache.refresh(&state.file)?;
        read(&cache)
    })
    .await
    .map_err(|error| ApiError(error.into()))?
    .map_err(ApiError)
}

async fn document(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    cached(state, move |cache| cache.document_response(&headers)).await
}

#[derive(Default, Deserialize)]
struct BoardQuery {
    since: Option<u64>,
}

async fn board(
    State(state): State<AppState>,
    Query(query): Query<BoardQuery>,
) -> ApiResult<Response> {
    cached(state, move |cache| cache.board_response(query.since)).await
}

async fn task(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    cached(state, move |cache| cache.task_response(id, &headers)).await
}

#[derive(Default, Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<Vec<u32>>> {
    cached(state, move |cache| Ok(Json(cache.search(&query.q)))).await
}

async fn create_task(
    State(state): State<AppState>,
    Json(input): Json<TaskInput>,
) -> ApiResult<Json<SavedTaskDto>> {
    let _lock = acquire_write_lock(&state.file)?;
    let mut doc = read_parsed_document(&state.file)?;
    let id = next_task_id(&doc, &BoardHistory::load(&state.file)?)?;
    validate_new_property_values(&input.properties)?;
    let mut task = task_from_input(id, input)?;
    mark_updated(&mut task, &now_utc());
    let assignment = task.assignee().map(str::to_string);
    let response = task_to_dto(&task);
    doc.tasks.add(task);
    write_document(&state.file, &doc)?;
    let mut warning = None;
    if let Some(assignee) = assignment {
        let task = doc.tasks.find_by_id(id).expect("task was added");
        warning = assignment_warning(task, &assignee);
    }
    Ok(Json(SavedTaskDto {
        task: response,
        warning,
    }))
}

async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Json(input): Json<TaskInput>,
) -> ApiResult<Json<SavedTaskDto>> {
    let _lock = acquire_write_lock(&state.file)?;
    let mut doc = read_parsed_document(&state.file)?;
    let task_id = TaskId::new(id)?;
    if let Some(expected) = &input.expected {
        let current = doc
            .tasks
            .find_by_id(task_id)
            .ok_or_else(|| ApiError(anyhow::anyhow!("Task {id} not found")))?;
        if !expected.matches(current) {
            return Err(ApiError(anyhow::anyhow!(
                "This task changed before the save completed. Your draft is kept. Reload the task or keep your draft before saving again."
            )));
        }
    }
    let new_status = input
        .properties
        .iter()
        .find(|property| property.key == "Status")
        .map(|property| property.value.clone());
    doc.ensure_next_transition(task_id, new_status.as_deref())?;
    let task = doc
        .tasks
        .find_by_id_mut(task_id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("Task {id} not found")))?;
    validate_changed_property_values(task, &input.properties)?;
    let mut replacement = task_from_input(task_id, input)?;
    mark_updated(&mut replacement, &now_utc());
    let assignment_changed = task.assignee() != replacement.assignee();
    let assignment = replacement.assignee().map(str::to_string);
    *task = replacement;
    let response = task_to_dto(task);
    let completed = new_status.as_deref() == Some("done");
    if completed {
        doc.remove_from_next(task_id);
    }
    write_document(&state.file, &doc)?;
    let mut warning = None;
    if assignment_changed {
        if let Some(assignee) = assignment {
            let task = doc
                .tasks
                .find_by_id(task_id)
                .expect("task exists after update");
            warning = assignment_warning(task, &assignee);
        }
    }
    Ok(Json(SavedTaskDto {
        task: response,
        warning,
    }))
}

/// Delete is `frump close`: the same checks, and the same warning when the task's text is not in
/// Git history. The warning goes back to the page; a request handler never prints.
async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> ApiResult<Json<ClosedTaskDto>> {
    let _lock = acquire_write_lock(&state.file)?;
    let mut doc = read_parsed_document(&state.file)?;
    let history = BoardHistory::load(&state.file)?;
    let results = prepare_close(&doc, &history, &[TaskId::new(id)?])?;
    apply_close(&mut doc, &results);
    write_document(&state.file, &doc)?;
    Ok(Json(ClosedTaskDto {
        id,
        warning: results.into_iter().next().and_then(|result| result.warning),
    }))
}

async fn notify_task(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Json(input): Json<NotifyInput>,
) -> ApiResult<StatusCode> {
    if input.recipient.trim().is_empty() || input.message.trim().is_empty() {
        return Err(ApiError(anyhow::anyhow!(
            "Recipient and message are required"
        )));
    }
    let doc = read_parsed_document(&state.file)?;
    let task = doc
        .tasks
        .find_by_id(TaskId::new(id)?)
        .ok_or_else(|| ApiError(anyhow::anyhow!("Task {id} not found")))?;
    let text = format!(
        "Frump task #{}: {}\n\n{}",
        task.id,
        task.subject,
        input.message.trim()
    );
    let _ = send_metateam_message(
        &[
            "crew",
            "message",
            "--from",
            "frump",
            input.recipient.trim(),
            &text,
        ],
        "send notification",
    )
    .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Announce an assignment from a request handler. The handler returns the warning
/// to the browser and never prints: a closed server stdout or stderr must not
/// abort a response whose task change is already saved.
fn assignment_warning(task: &Task, assignee: &str) -> Option<String> {
    announce_assignment(task, assignee)
        .err()
        .map(|error| notification_warning(&error))
}

#[cfg(test)]
fn read_document(file: &FsPath) -> Result<DocumentDto> {
    Ok(document_to_dto(&read_parsed_document(file)?))
}

fn read_parsed_document(file: &FsPath) -> Result<FrumpDoc> {
    storage::read(file).with_context(|| format!("Failed to read task board {}", file.display()))
}

fn write_document(file: &FsPath, doc: &FrumpDoc) -> Result<()> {
    storage::write(file, doc)
        .with_context(|| format!("Failed to write task board {}", file.display()))
}

fn acquire_write_lock(file: &FsPath) -> Result<fs::File> {
    let lock_path = storage::sharded_root(file)
        .map(|root| root.join("general.md"))
        .unwrap_or_else(|| file.to_path_buf());
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock_exclusive()?;
    Ok(lock)
}

fn task_from_input(id: TaskId, input: TaskInput) -> Result<Task> {
    if input.subject.trim().is_empty() {
        anyhow::bail!("Task subject cannot be empty");
    }
    let mut task = Task::new(
        id,
        TaskType::parse(&input.task_type),
        input.subject.trim().to_string(),
    );
    task.set_body(input.body);
    for property in input.properties {
        task.add_property(PropertyKey::new(&property.key)?, property.value);
    }
    Ok(task)
}

fn validate_new_property_values(properties: &[PropertyDto]) -> Result<()> {
    for property in properties {
        validate_property_value(&property.value)
            .map_err(|error| anyhow::anyhow!("Property '{}': {error}", property.key))?;
    }
    Ok(())
}

fn validate_changed_property_values(existing: &Task, properties: &[PropertyDto]) -> Result<()> {
    for property in properties {
        let key = PropertyKey::new(&property.key)?;
        if existing.get_property(&key) != Some(property.value.as_str()) {
            validate_property_value(&property.value)
                .map_err(|error| anyhow::anyhow!("Property '{}': {error}", property.key))?;
        }
    }
    Ok(())
}

fn document_to_dto(doc: &FrumpDoc) -> DocumentDto {
    DocumentDto {
        header: doc.header.clone(),
        team: doc
            .team
            .members()
            .iter()
            .map(|member| TeamMemberDto {
                name: member.name.clone(),
                email: member.email.as_str().to_string(),
                role: member.role.clone(),
            })
            .collect(),
        tasks: doc.tasks.tasks().iter().map(task_to_dto).collect(),
    }
}

fn task_to_dto(task: &Task) -> TaskDto {
    TaskDto {
        id: task.id.value(),
        task_type: task.task_type.as_str().to_string(),
        subject: task.subject.clone(),
        body: task.body.clone(),
        properties: task
            .properties
            .iter()
            .map(|property| PropertyDto {
                key: property.key.as_str().to_string(),
                value: property.value.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TaskCollection, Team};

    #[test]
    fn task_input_preserves_property_order() {
        let task = task_from_input(
            TaskId::new(1).unwrap(),
            TaskInput {
                task_type: "Task".to_string(),
                subject: "A task".to_string(),
                body: String::new(),
                properties: vec![
                    PropertyDto {
                        key: "Priority".to_string(),
                        value: "high".to_string(),
                    },
                    PropertyDto {
                        key: "Status".to_string(),
                        value: "open".to_string(),
                    },
                ],
                expected: None,
            },
        )
        .unwrap();
        assert_eq!(task_to_dto(&task).properties[0].key, "Priority");
        assert_eq!(task_to_dto(&task).properties[1].key, "Status");
    }

    #[test]
    fn document_dto_contains_all_tasks() {
        let task = Task::new(TaskId::new(3).unwrap(), TaskType::Bug, "Fix it".to_string());
        let doc = FrumpDoc::new(
            "# Project\n".to_string(),
            Team::empty(),
            TaskCollection::new(vec![task]),
        );
        assert_eq!(document_to_dto(&doc).tasks[0].id, 3);
    }

    #[test]
    fn legacy_long_property_survives_an_unrelated_web_edit() {
        let mut task = Task::new(
            TaskId::new(1).unwrap(),
            TaskType::Task,
            "A task".to_string(),
        );
        task.add_property(
            PropertyKey::new("Review").unwrap(),
            "A legacy review value deliberately longer than forty bytes".to_string(),
        );
        let unchanged = vec![PropertyDto {
            key: "Review".to_string(),
            value: "A legacy review value deliberately longer than forty bytes".to_string(),
        }];
        assert!(validate_changed_property_values(&task, &unchanged).is_ok());

        let changed = vec![PropertyDto {
            key: "Review".to_string(),
            value: "A changed review value deliberately longer than forty bytes".to_string(),
        }];
        let error = validate_changed_property_values(&task, &changed)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Review"));
        assert!(error.contains("task body"));
    }

    #[test]
    fn web_document_preserves_colon_ended_report_headings() {
        let path = std::env::temp_dir().join(format!(
            "frump-web-body-{}-{}.md",
            std::process::id(),
            crate::now_utc().replace(':', "-")
        ));
        fs::write(
            &path,
            "# Project\n\n## Tasks\n\n### Investigation 114 - Preserve report\n\nFraming.\n\nINVESTIGATION REPORT: first finding.\n\nCONTRACT 2: second finding.\n\nStatus: investigations\nAssigned To: Ada\n",
        )
        .unwrap();

        let document = read_document(&path).unwrap();
        let task = &document.tasks[0];
        assert!(task.body.contains("INVESTIGATION REPORT: first finding."));
        assert!(task.body.contains("CONTRACT 2: second finding."));
        assert_eq!(task.properties.len(), 2);

        fs::remove_file(path).unwrap();
    }
}

use anyhow::{Context, Result};
use clap::{ArgGroup, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use frump::board::{
    allocate_task_ids, apply_close, dependency_ids, ensure_outside_board, is_ready, next_task_id,
    prepare_close, prerequisites, resolve_board, templates_file, validate_dependencies,
    CloseResult, Prerequisite,
};
use frump::templates::parse_pairs;
use frump::{
    announce_assignment, append_update, export_csv, export_json, import_json, mark_updated,
    notification_warning, notify_task_update, now_utc, send_metateam_message, storage,
    validate_property_value, BoardHistory, ChangeType, PropertyKey, Task, TaskId, TaskTemplate,
    TaskType, TemplateManager, LAST_UPDATED_PROPERTY,
};
use indexmap::IndexMap;
use serde::Serialize;

#[derive(Parser)]
#[command(name = "frump")]
#[command(about = "Distributed task management tool based on Git and Markdown", long_about = None)]
#[command(
    after_help = "AI agents: run `frump usage` for the complete usage guide.\nBuild from source with `cargo build --release`."
)]
struct Cli {
    /// The task board: a board file, or a board directory
    /// (default: frump.md or frump/ found from the current directory upward)
    #[arg(long, global = true, value_name = "PATH")]
    board: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the complete usage guide (USAGE.md)
    Usage,

    /// Start a local Kanban board for the task file
    Web {
        /// TCP port for the loopback-only web server
        #[arg(long, default_value_t = 3000)]
        port: u16,
    },

    /// Commit the tracked task file with a short Git message
    Commit {
        /// Commit message
        #[arg(long, default_value = "Update frump tasks")]
        message: String,
    },

    /// List tasks, or closed tasks with --closed
    List {
        /// Filter by task type
        #[arg(long = "type", value_name = "TYPE")]
        task_type: Option<String>,

        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Filter by assignee
        #[arg(long)]
        assignee: Option<String>,

        /// Require an exact property key and value, for example `Review=approved`
        #[arg(long, value_name = "KEY=VALUE")]
        property: Option<String>,

        /// Require that a property key is absent
        #[arg(long, value_name = "KEY")]
        missing: Option<String>,

        /// Sort by id, status, assignee, last-updated, or a property name
        #[arg(long)]
        sort: Option<String>,

        /// Reverse the selected sort order
        #[arg(long, requires = "sort")]
        desc: bool,

        /// Emit a JSON array of matching tasks
        #[arg(long, default_value = "text", value_parser = ["text", "json"])]
        format: String,

        /// List tasks that Git history has and the board no longer has, in their last state
        #[arg(long, conflicts_with_all = ["status", "assignee", "property", "missing", "sort"])]
        closed: bool,
    },

    /// Replace a single Markdown board with a verified sharded board
    Migrate {
        /// Destination directory (default: the source file name without .md, next to it)
        #[arg(long, value_name = "DIR")]
        to: Option<PathBuf>,
    },

    /// Show the ordered task IDs allowed to leave todo, or replace the list
    Next {
        /// Replace the ordered list with these task IDs
        #[arg(long, value_name = "ID", num_args = 1.., conflicts_with = "clear")]
        set: Option<Vec<u32>>,

        /// Remove every task from the ordered plan
        #[arg(long)]
        clear: bool,
    },

    /// Show details of a specific task
    Show {
        /// Task ID
        id: u32,
    },

    /// Add a new task
    #[command(group(ArgGroup::new("source").required(true).args(["subject", "template"])))]
    Add {
        /// Task subject/title
        #[arg(long)]
        subject: Option<String>,

        /// Task type, for example Task, Bug, Issue or Feature (default: Task, or the template's)
        #[arg(long = "type", value_name = "TYPE")]
        task_type: Option<String>,

        /// Task body/description
        #[arg(long)]
        body: Option<String>,

        /// Assignee name
        #[arg(long)]
        assignee: Option<String>,

        /// Status
        #[arg(long)]
        status: Option<String>,

        /// Create the task from this template
        #[arg(long, value_name = "NAME")]
        template: Option<String>,

        /// Value for a template placeholder {KEY}; repeat for each placeholder
        #[arg(long, value_name = "KEY=VALUE", requires = "template")]
        fill: Vec<String>,
    },

    /// Close a done task by removing it from the board
    Close {
        /// Task ID
        id: u32,
    },

    /// Assign a task to a team member
    Assign {
        /// Task ID
        id: u32,

        /// Assignee name
        #[arg(long)]
        assignee: String,
    },

    /// Set a property on a task
    Set {
        /// Task ID
        id: u32,

        /// Property name (must be capitalized, max 3 words)
        #[arg(long)]
        property: String,

        /// Property value
        #[arg(long)]
        value: String,
    },

    /// Remove a property from a task, including Status
    Unset {
        /// Task ID
        id: u32,

        /// Property name
        #[arg(long)]
        property: String,
    },

    /// Show the history of a task
    History {
        /// Task ID
        id: u32,
    },

    /// Update a task's subject or body
    #[command(group(
        ArgGroup::new("change")
            .required(true)
            .multiple(true)
            .args(["subject", "body", "clear_body", "append"])
    ))]
    Update {
        /// Task ID
        id: u32,

        /// New subject
        #[arg(long)]
        subject: Option<String>,

        /// Replacement body
        #[arg(long, conflicts_with_all = ["clear_body", "append"])]
        body: Option<String>,

        /// Explicitly remove the current body
        #[arg(long, conflicts_with = "append")]
        clear_body: bool,

        /// Append a dated update to the task body
        #[arg(long, value_name = "TEXT")]
        append: Option<String>,

        /// After saving, send the appended text through Metateam: to the task's assignees
        /// (every crew member when the task has no assignee), or to every crew member
        #[arg(long, value_enum, requires = "append")]
        notify: Option<NotifyTarget>,
    },

    /// Search tasks by keyword
    Search {
        /// Search text, matched against subject and body
        #[arg(long)]
        text: String,

        /// Print the start of each matching task's body
        #[arg(long)]
        show_body: bool,
    },

    /// Show task statistics
    Stats,

    /// Validate the task board
    Validate,

    /// Create a new empty Frump board
    Init {
        #[arg(long, default_value = "My Project")]
        title: String,
    },

    /// Show the prerequisite tree for a task
    DependsOn {
        /// Task ID
        id: u32,
    },

    /// List active tasks that depend on a task
    Dependents {
        /// Task ID
        id: u32,
    },

    /// List tasks whose prerequisites are satisfied
    Ready,

    /// Export tasks to JSON or CSV
    Export {
        /// Output format
        #[arg(long, default_value = "json", value_parser = ["json", "csv"])]
        format: String,

        /// Output file (stdout if not specified)
        #[arg(long, value_name = "PATH")]
        to: Option<PathBuf>,
    },

    /// Import tasks from a JSON export into the board
    Import {
        /// The JSON file to import
        #[arg(long, value_name = "PATH")]
        from: PathBuf,

        /// The board to import into (default: the board found as for every command)
        #[arg(long, value_name = "BOARD")]
        to: Option<PathBuf>,

        /// Add the tasks to the existing board with new numbers instead of replacing it
        #[arg(long)]
        merge: bool,
    },

    /// Manage task templates
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },

    /// Bulk operations on tasks
    Bulk {
        #[command(subcommand)]
        action: BulkAction,
    },

    /// Give duplicate task numbers new numbers (after a merge)
    RenumberDuplicates {
        /// Commit the board after renumbering
        #[arg(long)]
        commit: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum NotifyTarget {
    /// The task's assignees, or every crew member when the task has no assignee
    Assignee,
    /// Every crew member
    All,
}

#[derive(Subcommand)]
enum TemplateAction {
    /// Add a new template
    Add {
        /// Template name
        #[arg(long)]
        name: String,

        /// Task type
        #[arg(long = "type", value_name = "TYPE", default_value = "Task")]
        task_type: String,

        /// Subject template (use {placeholder} for values given with add --fill)
        #[arg(long)]
        subject: String,

        /// Body template
        #[arg(long)]
        body: Option<String>,

        /// A property of every task from this template; repeat for each property
        #[arg(long, value_name = "KEY=VALUE")]
        property: Vec<String>,
    },

    /// List all templates
    List,

    /// Remove a template
    Remove {
        /// Template name
        #[arg(long)]
        name: String,
    },

    /// Show template details
    Show {
        /// Template name
        #[arg(long)]
        name: String,
    },
}

#[derive(Subcommand)]
enum BulkAction {
    /// Close every task with this status
    Close {
        /// Select tasks with this status
        #[arg(long)]
        with_status: String,
    },

    /// Assign every task of this type
    Assign {
        /// Select tasks of this type
        #[arg(long, value_name = "TYPE")]
        with_type: String,

        /// Assignee name
        #[arg(long)]
        assignee: String,
    },

    /// Set a property on every task with this status
    Set {
        /// Select tasks with this status
        #[arg(long)]
        with_status: String,

        /// Property name
        #[arg(long)]
        property: String,

        /// Property value
        #[arg(long)]
        value: String,
    },
}

/// Long names only: clap's `-h` is replaced by `--help` on every command.
fn long_help_only(command: clap::Command) -> clap::Command {
    command
        .disable_help_flag(true)
        .arg(
            clap::Arg::new("help")
                .long("help")
                .action(clap::ArgAction::Help)
                .help("Print help"),
        )
        .mut_subcommands(long_help_only)
}

fn parse_cli() -> Cli {
    let matches = long_help_only(Cli::command()).get_matches();
    Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = parse_cli();
    let board = match &cli.command {
        Commands::Init { .. } => resolve_board(
            cli.board
                .as_deref()
                .unwrap_or_else(|| Path::new("frump.md")),
        )?,
        Commands::Import { to: Some(to), .. } => {
            if cli.board.is_some() {
                anyhow::bail!("Name the board with either --board or import --to, not both.");
            }
            resolve_board(to)?
        }
        _ => resolve_board(&match &cli.board {
            Some(path) => path.clone(),
            None => discover_task_file()?,
        })?,
    };
    let _write_lock = if board.exists()
        && matches!(
            cli.command,
            Commands::Add { .. }
                | Commands::Close { .. }
                | Commands::Assign { .. }
                | Commands::Set { .. }
                | Commands::Unset { .. }
                | Commands::Update { .. }
                | Commands::Import { .. }
                | Commands::Bulk { .. }
                | Commands::RenumberDuplicates { .. }
                | Commands::Migrate { .. }
                | Commands::Next { .. }
        ) {
        Some(acquire_write_lock(&board)?)
    } else {
        None
    };

    match &cli.command {
        Commands::Usage => {
            print!("{}", include_str!("../USAGE.md"));
        }

        Commands::Web { port } => {
            frump::web::serve(board.clone(), *port).await?;
        }

        Commands::Commit { message } => {
            commit_task_file(&board, message)?;
            println!("Committed {}", board.display());
        }

        Commands::List {
            task_type,
            status,
            assignee,
            property,
            missing,
            sort,
            desc,
            format,
            closed,
        } => {
            let doc = read_document(&board)?;

            if *closed {
                let history = BoardHistory::load(&board)?;
                let mut removed = history.removed_tasks(&doc);
                if let Some(tt) = task_type {
                    let filter_type = TaskType::parse(tt);
                    removed.retain(|t| t.task_type == filter_type);
                }
                if format == "json" {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &removed.into_iter().map(ListTask::from).collect::<Vec<_>>()
                        )?
                    );
                } else if removed.is_empty() {
                    println!("No closed tasks found.");
                } else {
                    println!("Closed tasks:\n");
                    for task in &removed {
                        println!("{} {} - {}", task.task_type, task.id, task.subject);
                    }
                    println!("\nTotal: {} closed tasks", removed.len());
                }
                return Ok(());
            }

            let mut tasks = doc.tasks.tasks().to_vec();

            // Apply filters
            if let Some(tt) = task_type {
                let filter_type = TaskType::parse(tt);
                tasks.retain(|t| t.task_type == filter_type);
            }

            if let Some(s) = status {
                tasks.retain(|t| t.status().map(|st| st == s).unwrap_or(false));
            }

            if let Some(a) = assignee {
                tasks.retain(|t| t.assignee().map(|name| name == a).unwrap_or(false));
            }

            if let Some(filter) = property {
                let (key, value) = parse_property_filter(filter)?;
                tasks.retain(|task| task.get_property(&key) == Some(value.as_str()));
            }
            if let Some(key) = missing {
                let key = PropertyKey::new(key)?;
                tasks.retain(|task| task.get_property(&key).is_none());
            }
            if let Some(key) = sort {
                sort_tasks(&mut tasks, key, *desc)?;
            }

            if format == "json" {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &tasks.iter().map(ListTask::from).collect::<Vec<_>>()
                    )?
                );
                return Ok(());
            }

            if tasks.is_empty() {
                println!("No tasks found.");
            } else {
                for task in &tasks {
                    println!("{} {} - {}", task.task_type, task.id, task.subject);
                    if let Some(status) = task.status() {
                        println!("  Status: {}", status);
                    }
                    if let Some(assignee) = task.assignee() {
                        println!("  Assigned to: {}", assignee);
                    }
                }
            }
        }

        Commands::Migrate { to } => {
            let default_destination = default_migration_destination(&board);
            let destination = match to {
                Some(to) => resolve_board(to)?,
                None => default_destination.clone(),
            };
            storage::migrate_single_file(&board, &destination)?;
            println!(
                "Migrated {} to {}. The source file has been replaced.",
                board.display(),
                destination.display()
            );
            if destination != default_destination {
                eprintln!(
                    "Warning: {} is not {}, so it does not carry the Git history of {}. \
                     Task numbers closed in {} can be given out again, and `frump history` \
                     starts at this migration.",
                    destination.display(),
                    default_destination.display(),
                    board.display(),
                    board.display()
                );
            }
        }

        Commands::Next { set, clear } => {
            let mut doc = read_document(&board)?;
            if *clear {
                doc.next.clear();
                storage::write(&board, &doc)?;
                println!("Cleared next tasks.");
            } else if let Some(ids) = set {
                let next: Result<Vec<_>, _> = ids.iter().map(|id| TaskId::new(*id)).collect();
                doc.next = next?;
                doc.validate_next()?;
                storage::write(&board, &doc)?;
                println!(
                    "Set next tasks: {}",
                    doc.next
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else if doc.next.is_empty() {
                println!("No next tasks planned.");
            } else {
                for id in &doc.next {
                    println!("{}", id);
                }
            }
        }

        Commands::Show { id } => {
            let doc = read_document(&board)?;

            let task_id = TaskId::new(*id)?;
            if let Some(task) = doc.tasks.find_by_id(task_id) {
                println!("### {} {} - {}\n", task.task_type, task.id, task.subject);

                if !task.body.is_empty() {
                    println!("{}\n", task.body);
                }

                if !task.properties.is_empty() {
                    for prop in &task.properties {
                        println!("{}: {}", prop.key, prop.value);
                    }
                }
            } else {
                anyhow::bail!("Task {} not found.", id);
            }
        }

        Commands::Add {
            subject,
            task_type,
            body,
            assignee,
            status,
            template,
            fill,
        } => {
            let mut doc = read_document(&board)?;
            let history = BoardHistory::load(&board)?;
            let next_id = next_task_id(&doc, &history)?;

            let mut new_task = match template {
                Some(name) => {
                    let fills = parse_pairs(fill, "--fill")?;
                    let template = TemplateManager::new(templates_file(&board)).get(name)?;
                    let mut task = template.instantiate(next_id, &fills, body.as_deref())?;
                    if let Some(task_type) = task_type {
                        task.task_type = TaskType::parse(task_type);
                    }
                    task
                }
                None => {
                    let subject = subject.clone().context("--subject is required")?;
                    let mut task = Task::new(
                        next_id,
                        TaskType::parse(task_type.as_deref().unwrap_or("Task")),
                        subject,
                    );
                    if let Some(b) = body {
                        task.set_body(b.clone());
                    }
                    task
                }
            };
            if new_task.subject.trim().is_empty() {
                anyhow::bail!("Task subject cannot be empty.");
            }
            for property in &new_task.properties {
                validate_property_value(&property.value)?;
            }

            let query = normalize_search_text(&new_task.subject);
            let candidates: Vec<_> = doc
                .tasks
                .tasks()
                .iter()
                .filter(|task| {
                    all_query_tokens_match(&query, &task.subject, &task.body)
                        && search_similarity(&query, &task.subject)
                            .max(search_similarity(&query, &task.body))
                            >= 0.78
                })
                .collect();
            if !candidates.is_empty() {
                eprintln!("Warning: similar task(s) already exist:");
                for task in candidates {
                    eprintln!("  {} {} - {}", task.task_type, task.id, task.subject);
                }
            }

            if let Some(a) = assignee {
                validate_property_value(a)?;
                new_task.set_assignee(a.clone());
            } else if new_task.assignee().is_none() {
                if let Some(default) = doc.team.default_assignee() {
                    validate_property_value(&default.name)?;
                    new_task.set_assignee(default.name.clone());
                }
            }

            if let Some(s) = status {
                validate_property_value(s)?;
                warn_if_new_status(&doc, s);
                new_task.set_status(s.clone());
            }
            touch_task(&mut new_task);

            let assignment = new_task.assignee().map(str::to_string);
            let assignment_task = new_task.clone();
            let added = format!(
                "Added {} {} - {}",
                new_task.task_type, next_id, new_task.subject
            );

            doc.tasks.add(new_task);

            // Write back to file
            storage::write(&board, &doc)?;

            if let Some(assignee) = assignment {
                print_assignment_announcement(&assignment_task, &assignee);
            }

            println!("{added}");
        }

        Commands::Close { id } => {
            let mut doc = read_document(&board)?;
            let history = BoardHistory::load(&board)?;
            let results = prepare_close(&doc, &history, &[TaskId::new(*id)?])?;
            apply_close(&mut doc, &results);
            storage::write(&board, &doc)?;
            report_close(&results);
            print_line("\nRemember to commit this change with a descriptive message.");
        }

        Commands::Assign { id, assignee } => {
            validate_property_value(assignee)?;
            let mut doc = read_document(&board)?;

            let task_id = TaskId::new(*id)?;
            if let Some(task) = doc.tasks.find_by_id_mut(task_id) {
                let changed = task.assignee() != Some(assignee.as_str());
                task.set_assignee(assignee.clone());
                touch_task(task);
                let assigned_task = changed.then(|| task.clone());

                storage::write(&board, &doc)?;

                if let Some(task) = assigned_task {
                    print_assignment_announcement(&task, assignee);
                }

                println!("Assigned task {} to {}", id, assignee);
            } else {
                anyhow::bail!("Task {} not found.", id);
            }
        }

        Commands::Set {
            id,
            property,
            value,
        } => {
            let mut doc = read_document(&board)?;

            let task_id = TaskId::new(*id)?;
            let prop_key = PropertyKey::new(property)?;
            validate_property_value(value)?;

            if property == LAST_UPDATED_PROPERTY {
                anyhow::bail!(
                    "{} is managed automatically and cannot be set directly.",
                    LAST_UPDATED_PROPERTY
                );
            }

            if property == "Status" {
                warn_if_new_status(&doc, value);
                doc.ensure_next_transition(task_id, Some(value))?;
            }

            let completed = property == "Status" && value == "done";
            let mut assigned_task = None;
            if let Some(task) = doc.tasks.find_by_id_mut(task_id) {
                let assignment_changed =
                    property == "Assigned To" && task.assignee() != Some(value.as_str());
                task.set_property(prop_key, value.clone());
                touch_task(task);
                if assignment_changed {
                    assigned_task = Some(task.clone());
                }
            } else {
                anyhow::bail!("Task {} not found.", id);
            }
            if completed {
                doc.remove_from_next(task_id);
            }
            storage::write(&board, &doc)?;
            if let Some(task) = assigned_task {
                print_assignment_announcement(&task, value);
            }
            println!("Set {} = {} on task {}", property, value, id);
        }

        Commands::Unset { id, property } => {
            let mut doc = read_document(&board)?;
            let key = PropertyKey::new(property)?;
            if property == LAST_UPDATED_PROPERTY {
                anyhow::bail!(
                    "{} is managed automatically and cannot be removed.",
                    LAST_UPDATED_PROPERTY
                );
            }
            let task_id = TaskId::new(*id)?;
            if property == "Status" {
                doc.ensure_next_transition(task_id, None)?;
            }
            let task = doc
                .tasks
                .find_by_id_mut(task_id)
                .ok_or_else(|| anyhow::anyhow!("Task {} not found.", id))?;
            if task.get_property(&key).is_none() {
                anyhow::bail!("Task {} has no {} property.", id, property);
            }
            task.remove_property(&key);
            touch_task(task);
            storage::write(&board, &doc)?;
            println!("Removed {} from task {}", property, id);
        }

        Commands::History { id } => {
            let history = BoardHistory::load(&board)?;
            if !history.in_git() {
                anyhow::bail!("Task history requires a board inside a Git repository.");
            }
            let task_id = TaskId::new(*id)?;
            let history = history.task_history(task_id);

            if history.commits.is_empty() {
                println!("No history found for task {}", id);
            } else {
                println!("History for Task {}:\n", id);
                for commit in &history.commits {
                    let change_icon = match commit.change_type {
                        ChangeType::Created => "✓ Created",
                        ChangeType::Modified => "• Modified",
                        ChangeType::Deleted => "✗ Deleted",
                    };

                    println!(
                        "{} by {} on {}",
                        change_icon,
                        commit.author,
                        commit.date.format("%Y-%m-%d %H:%M")
                    );
                    println!("  Commit: {}", &commit.hash[..8]);
                    if !commit.message.is_empty() {
                        // Show first line of commit message
                        let first_line = commit.message.lines().next().unwrap_or("");
                        println!("  Message: {}", first_line);
                    }
                    println!();
                }
            }
        }

        Commands::Update {
            id,
            subject,
            body,
            clear_body,
            append,
            notify,
        } => {
            if body
                .as_ref()
                .is_some_and(|new_body| new_body.trim().is_empty())
            {
                anyhow::bail!("Body cannot be empty; use --clear-body to remove it explicitly.");
            }

            let mut doc = read_document(&board)?;

            let task_id = TaskId::new(*id)?;
            let task = doc
                .tasks
                .find_by_id_mut(task_id)
                .ok_or_else(|| anyhow::anyhow!("Task {} not found.", id))?;
            // Report each change only after the whole update is saved.
            let mut changes = Vec::new();
            if let Some(new_subject) = subject {
                task.subject = new_subject.clone();
                changes.push("Updated subject");
            }
            if let Some(new_body) = body {
                task.set_body(new_body.clone());
                changes.push("Replaced body");
            }
            if *clear_body {
                task.set_body(String::new());
                changes.push("Cleared body");
            }
            if let Some(extra) = append {
                append_update(task, extra, current_crew_agent().as_deref())?;
                changes.push("Appended body");
            } else {
                touch_task(task);
            }

            storage::write(&board, &doc)?;
            for change in changes {
                println!("{change} for task {id}");
            }
            if let (Some(message), Some(target)) = (append, notify) {
                match target {
                    NotifyTarget::Assignee => {
                        let task = doc
                            .tasks
                            .find_by_id(task_id)
                            .expect("task exists after update");
                        report_notification(notify_task_update(task, message));
                    }
                    NotifyTarget::All => report_notification(send_metateam_message(
                        &["crew", "message", "all", message],
                        "send message",
                    )),
                }
            }
        }

        Commands::Search { text, show_body } => {
            let doc = read_document(&board)?;

            let normalized_query = normalize_search_text(text);
            let mut found = Vec::new();

            for task in doc.tasks.tasks() {
                if !all_query_tokens_match(&normalized_query, &task.subject, &task.body) {
                    continue;
                }
                let subject_score = search_similarity(&normalized_query, &task.subject);
                let body_score = search_similarity(&normalized_query, &task.body);
                let score = subject_score.max(body_score);
                if score >= 0.78 {
                    found.push((task, score, subject_score >= body_score));
                }
            }

            found.sort_by(|left, right| {
                right
                    .1
                    .total_cmp(&left.1)
                    .then_with(|| right.2.cmp(&left.2))
            });

            if found.is_empty() {
                println!("No tasks found matching '{}'", text);
            } else {
                println!(
                    "Found {} similar task(s) matching '{}':\n",
                    found.len(),
                    text
                );
                for (task, score, subject_match) in found {
                    println!("{} {} - {}", task.task_type, task.id, task.subject);
                    println!(
                        "  Similarity: {:.0}% ({})",
                        score * 100.0,
                        if subject_match { "subject" } else { "body" }
                    );
                    if *show_body && !task.body.is_empty() {
                        println!("  {}", body_snippet(&task.body));
                    }
                }
            }
        }

        Commands::Stats => {
            let doc = read_document(&board)?;

            let total = doc.tasks.len();

            // Count by type
            let mut type_counts = std::collections::HashMap::new();
            for task in doc.tasks.tasks() {
                *type_counts.entry(task.task_type.as_str()).or_insert(0) += 1;
            }

            // Count by status
            let mut status_counts = std::collections::HashMap::new();
            let mut no_status = 0;
            for task in doc.tasks.tasks() {
                if let Some(status) = task.status() {
                    *status_counts.entry(status).or_insert(0) += 1;
                } else {
                    no_status += 1;
                }
            }

            // Count by assignee
            let mut assignee_counts = std::collections::HashMap::new();
            let mut no_assignee = 0;
            for task in doc.tasks.tasks() {
                if let Some(assignee) = task.assignee() {
                    *assignee_counts.entry(assignee).or_insert(0) += 1;
                } else {
                    no_assignee += 1;
                }
            }

            println!("Task Statistics\n");
            println!("Total tasks: {}\n", total);

            println!("By Type:");
            let mut types: Vec<_> = type_counts.iter().collect();
            types.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
            for (task_type, count) in types {
                println!("  {}: {}", task_type, count);
            }

            println!("\nBy Status:");
            if !status_counts.is_empty() {
                let mut statuses: Vec<_> = status_counts.iter().collect();
                statuses.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
                for (status, count) in statuses {
                    println!("  {}: {}", status, count);
                }
            }
            if no_status > 0 {
                println!("  (no status): {}", no_status);
            }

            println!("\nBy Assignee:");
            if !assignee_counts.is_empty() {
                let mut assignees: Vec<_> = assignee_counts.iter().collect();
                assignees.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
                for (assignee, count) in assignees {
                    println!("  {}: {}", assignee, count);
                }
            }
            if no_assignee > 0 {
                println!("  (no assignee): {}", no_assignee);
            }

            let history = BoardHistory::load(&board)?;
            if history.in_git() {
                println!("\nClosed tasks: {}", history.removed_tasks(&doc).len());
            }
        }

        Commands::Validate => {
            let doc = match read_document(&board) {
                Ok(doc) => doc,
                Err(e) => {
                    println!("✗ Validation failed: {:#}", e);
                    anyhow::bail!("Validation failed");
                }
            };
            println!("✓ File structure is valid");
            let mut failed = false;

            // Check for duplicate IDs
            let mut ids = std::collections::HashSet::new();
            let mut duplicates = Vec::new();
            for task in doc.tasks.tasks() {
                if !ids.insert(task.id) {
                    duplicates.push(task.id);
                }
            }

            if !duplicates.is_empty() {
                println!("✗ Found duplicate task IDs: {:?}", duplicates);
                println!("  Run 'frump renumber-duplicates' to give them new numbers.");
                failed = true;
            } else {
                println!("✓ All task IDs are unique");
            }

            let history = BoardHistory::load(&board)?;
            let dependency_errors = validate_dependencies(&doc, &history);
            if dependency_errors.is_empty() {
                println!("✓ Dependencies resolve and are acyclic");
            } else {
                for error in dependency_errors {
                    println!("✗ {}", error);
                }
                failed = true;
            }

            // Check for sequential IDs
            let mut ids_vec: Vec<_> = doc.tasks.tasks().iter().map(|t| t.id.value()).collect();
            ids_vec.sort();
            let mut gaps = Vec::new();
            for i in 1..ids_vec.len() {
                if ids_vec[i] > ids_vec[i - 1] + 1 {
                    gaps.push((ids_vec[i - 1] + 1, ids_vec[i] - 1));
                }
            }

            if !gaps.is_empty() {
                println!("⚠ ID gaps found (possibly closed tasks):");
                for (start, end) in gaps {
                    if start == end {
                        println!("  ID {}", start);
                    } else {
                        println!("  IDs {}-{}", start, end);
                    }
                }
            } else {
                println!("✓ Task IDs are sequential");
            }

            if failed {
                anyhow::bail!("Validation failed");
            }
            println!(
                "\n✓ Validation complete: {} tasks, {} team members",
                doc.tasks.len(),
                doc.team.len()
            );
        }

        Commands::Init { title } => {
            if board.exists() {
                anyhow::bail!(
                    "{} already exists; refusing to overwrite it.",
                    board.display()
                );
            }
            fs::write(&board, format!("# {}\n\n## Team\n\n## Tasks\n", title))
                .with_context(|| format!("Failed to initialize {}", board.display()))?;
            println!("Initialized {}", board.display());
        }

        Commands::DependsOn { id } => {
            let doc = read_document(&board)?;
            let history = BoardHistory::load(&board)?;
            let task_id = TaskId::new(*id)?;
            let task = doc
                .tasks
                .find_by_id(task_id)
                .ok_or_else(|| anyhow::anyhow!("Task {} not found.", id))?;
            print_prerequisite_tree(
                &doc,
                &history,
                task,
                0,
                &mut std::collections::HashSet::new(),
            );
        }

        Commands::Dependents { id } => {
            let doc = read_document(&board)?;
            let id = TaskId::new(*id)?;
            for task in doc
                .tasks
                .tasks()
                .iter()
                .filter(|task| dependency_ids(task).contains(&id))
            {
                println!("{} {} - {}", task.task_type, task.id, task.subject);
            }
        }

        Commands::Ready => {
            let doc = read_document(&board)?;
            let history = BoardHistory::load(&board)?;
            for task in doc
                .tasks
                .tasks()
                .iter()
                .filter(|task| task.status() != Some("done") && is_ready(task, &doc, &history))
            {
                println!("{} {} - {}", task.task_type, task.id, task.subject);
            }
        }

        Commands::Export { format, to } => {
            let doc = read_document(&board)?;

            let exported = match format.as_str() {
                "json" => export_json(&doc)?,
                _ => export_csv(&doc)?,
            };

            if let Some(output_path) = to {
                ensure_outside_board(&board, output_path, "export --to")?;
                fs::write(output_path, &exported)
                    .with_context(|| format!("Failed to write {}", output_path.display()))?;
                println!(
                    "Exported {} tasks to {}",
                    doc.tasks.len(),
                    output_path.display()
                );
            } else {
                println!("{}", exported);
            }
        }

        Commands::Import { from, merge, .. } => {
            ensure_outside_board(&board, from, "import --from")?;
            let import_content = fs::read_to_string(from)
                .with_context(|| format!("Failed to read {}", from.display()))?;

            let imported_doc = import_json(&import_content)?;

            if *merge {
                let mut current_doc = read_document(&board)?;
                let history = BoardHistory::load(&board)?;
                let new_ids = allocate_task_ids(&current_doc, &history, imported_doc.tasks.len())?;
                let mut renumber = HashMap::new();
                for (task, new_id) in imported_doc.tasks.tasks().iter().zip(&new_ids) {
                    if renumber.insert(task.id, *new_id).is_some() {
                        anyhow::bail!(
                            "{} has task {} more than once; nothing was imported.",
                            from.display(),
                            task.id
                        );
                    }
                }

                let mut announcements = Vec::new();
                let mut added = Vec::new();
                for (task, new_id) in imported_doc.tasks.tasks().iter().zip(&new_ids) {
                    let mut new_task =
                        Task::new(*new_id, task.task_type.clone(), task.subject.clone());
                    new_task.set_body(task.body.clone());

                    for prop in &task.properties {
                        let value = if prop.key.as_str() == "Depends On" {
                            renumber_dependencies(&prop.value, &renumber, task.id, from)?
                        } else {
                            prop.value.clone()
                        };
                        validate_property_value(&value)?;
                        new_task.add_property(prop.key.clone(), value);
                    }
                    touch_task(&mut new_task);

                    if let Some(assignee) = new_task.assignee().map(str::to_string) {
                        announcements.push((new_task.clone(), assignee));
                    }
                    added.push(new_task);
                }
                let count = added.len();
                for task in added {
                    current_doc.tasks.add(task);
                }

                storage::write(&board, &current_doc)?;
                for (task, assignee) in announcements {
                    print_assignment_announcement(&task, &assignee);
                }

                println!("Merged {} tasks into {}", count, board.display());
            } else {
                // Replace: a restore of a whole board; it assigns nobody, so it announces nothing.
                storage::write(&board, &imported_doc)?;
                println!(
                    "Imported {} tasks, {} team members",
                    imported_doc.tasks.len(),
                    imported_doc.team.len()
                );
            }
        }

        Commands::Template { action } => {
            let manager = TemplateManager::new(templates_file(&board));

            match action {
                TemplateAction::Add {
                    name,
                    task_type,
                    subject,
                    body,
                    property,
                } => {
                    let template = TaskTemplate {
                        name: name.clone(),
                        task_type: task_type.clone(),
                        subject_template: subject.clone(),
                        body_template: body.clone().unwrap_or_default(),
                        properties: parse_pairs(property, "--property")?,
                    };

                    manager.add(template)?;
                    println!("Added template '{}'", name);
                }

                TemplateAction::List => {
                    let templates = manager.list()?;

                    if templates.is_empty() {
                        println!("No templates found.");
                    } else {
                        println!("Available templates:\n");
                        for template in templates {
                            println!("{} ({})", template.name, template.task_type);
                            println!("  Subject: {}", template.subject_template);
                            if !template.body_template.is_empty() {
                                println!("  Body: {}", template.body_template);
                            }
                            println!();
                        }
                    }
                }

                TemplateAction::Remove { name } => {
                    manager.remove(name)?;
                    println!("Removed template '{}'", name);
                }

                TemplateAction::Show { name } => {
                    let template = manager.get(name)?;
                    println!("Template: {}", template.name);
                    println!("Type: {}", template.task_type);
                    println!("Subject: {}", template.subject_template);
                    if !template.body_template.is_empty() {
                        println!("Body: {}", template.body_template);
                    }
                    if !template.properties.is_empty() {
                        println!("Properties:");
                        for (key, value) in &template.properties {
                            println!("  {}: {}", key, value);
                        }
                    }
                }
            }
        }

        Commands::Bulk { action } => {
            let mut doc = read_document(&board)?;

            match action {
                BulkAction::Close { with_status } => {
                    let ids: Vec<TaskId> = doc
                        .tasks
                        .tasks()
                        .iter()
                        .filter(|t| t.status() == Some(with_status.as_str()))
                        .map(|t| t.id)
                        .collect();

                    if ids.is_empty() {
                        println!("No tasks found with status '{}'", with_status);
                        return Ok(());
                    }

                    let history = BoardHistory::load(&board)?;
                    let results = prepare_close(&doc, &history, &ids)?;
                    apply_close(&mut doc, &results);
                    storage::write(&board, &doc)?;
                    report_close(&results);
                    print_line(&format!(
                        "\nClosed {} task(s) with status '{}'",
                        results.len(),
                        with_status
                    ));
                }

                BulkAction::Assign {
                    with_type,
                    assignee,
                } => {
                    validate_property_value(assignee)?;
                    let filter_type = TaskType::parse(with_type);
                    let mut count = 0;
                    let mut announcements = Vec::new();

                    for task in doc.tasks.tasks_mut() {
                        if task.task_type == filter_type {
                            let changed = task.assignee() != Some(assignee.as_str());
                            task.set_assignee(assignee.clone());
                            touch_task(task);
                            if changed {
                                announcements.push(task.clone());
                            }
                            count += 1;
                        }
                    }

                    if count == 0 {
                        println!("No tasks found with type '{}'", with_type);
                        return Ok(());
                    }

                    storage::write(&board, &doc)?;
                    for task in announcements {
                        print_assignment_announcement(&task, assignee);
                    }

                    println!(
                        "Assigned {} task(s) of type '{}' to {}",
                        count, with_type, assignee
                    );
                }

                BulkAction::Set {
                    with_status,
                    property,
                    value,
                } => {
                    let prop_key = PropertyKey::new(property)?;
                    validate_property_value(value)?;
                    if property == LAST_UPDATED_PROPERTY {
                        anyhow::bail!(
                            "{} is managed automatically and cannot be set directly.",
                            LAST_UPDATED_PROPERTY
                        );
                    }
                    let targets: Vec<_> = doc
                        .tasks
                        .tasks()
                        .iter()
                        .filter(|task| task.status() == Some(with_status.as_str()))
                        .map(|task| task.id)
                        .collect();
                    if property == "Status" {
                        for id in &targets {
                            doc.ensure_next_transition(*id, Some(value))?;
                        }
                    }
                    let mut count = 0;
                    let mut announcements = Vec::new();

                    for task in doc.tasks.tasks_mut() {
                        if task.status() == Some(with_status.as_str()) {
                            let assignment_changed = property == "Assigned To"
                                && task.assignee() != Some(value.as_str());
                            task.set_property(prop_key.clone(), value.clone());
                            touch_task(task);
                            if assignment_changed {
                                announcements.push(task.clone());
                            }
                            count += 1;
                        }
                    }

                    if property == "Status" && value == "done" {
                        for id in targets {
                            doc.remove_from_next(id);
                        }
                    }

                    if count == 0 {
                        println!("No tasks found with status '{}'", with_status);
                        return Ok(());
                    }

                    storage::write(&board, &doc)?;
                    for task in announcements {
                        print_assignment_announcement(&task, value);
                    }

                    println!(
                        "Set {} = {} on {} task(s) with status '{}'",
                        property, value, count, with_status
                    );
                }
            }
        }

        Commands::RenumberDuplicates { commit } => {
            let mut doc = read_document(&board)?;

            // Find duplicate IDs; the first occurrence keeps its number.
            let mut seen = std::collections::HashSet::new();
            let duplicates: Vec<usize> = doc
                .tasks
                .tasks()
                .iter()
                .enumerate()
                .filter(|(_, task)| !seen.insert(task.id))
                .map(|(index, _)| index)
                .collect();

            if duplicates.is_empty() {
                println!("✓ No duplicate task IDs found");
                println!("Nothing to resolve.");
                return Ok(());
            }

            let history = BoardHistory::load(&board)?;
            let new_ids = allocate_task_ids(&doc, &history, duplicates.len())?;
            let mut renumbered = Vec::new();
            for (&index, &new_id) in duplicates.iter().zip(&new_ids) {
                let task = &mut doc.tasks.tasks_mut()[index];
                let old_id = task.id;
                task.id = new_id;
                touch_task(task);
                renumbered.push((old_id, new_id, task.subject.clone()));
            }

            storage::write(&board, &doc)?;

            println!("✓ Resolved {} duplicate task ID(s):\n", renumbered.len());
            for (old_id, new_id, subject) in &renumbered {
                println!("  {} → {}: {}", old_id, new_id, subject);
            }

            if *commit {
                commit_task_file(
                    &board,
                    &format!(
                        "Resolve task ID conflicts\n\nRenumbered {} conflicting task(s)",
                        renumbered.len()
                    ),
                )?;
                println!("\n✓ Changes committed");
            } else {
                println!("\nRemember to commit these changes.");
                println!("Run with --commit to commit automatically.");
            }
        }
    }

    Ok(())
}

/// Rewrite one imported `Depends On` value to the new task numbers. An entry that is not a task
/// number stays as written; a number outside the imported file refuses the whole import.
fn renumber_dependencies(
    value: &str,
    renumber: &HashMap<TaskId, TaskId>,
    task: TaskId,
    source: &Path,
) -> Result<String> {
    value
        .split(',')
        .map(|raw| {
            let raw = raw.trim();
            match raw.parse::<u32>().ok().and_then(|id| TaskId::new(id).ok()) {
                None => Ok(raw.to_string()),
                Some(id) => renumber.get(&id).map(ToString::to_string).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Task {} in {} depends on task {}, which the file does not have; nothing was imported.",
                        task,
                        source.display(),
                        id
                    )
                }),
            }
        })
        .collect::<Result<Vec<_>>>()
        .map(|entries| entries.join(", "))
}

/// Report closed tasks after the board is saved. Every recovery warning goes to stderr first:
/// it may hold the only copy of a task, and a closed stdout must not stop it.
fn report_close(results: &[CloseResult]) {
    for warning in results.iter().filter_map(|result| result.warning.as_ref()) {
        eprintln!("Warning: {warning}");
    }
    for result in results {
        print_line(&format!(
            "Closed {} {} - {}",
            result.task.task_type, result.task.id, result.task.subject
        ));
    }
}

/// Print one informational line after a saved change; a reader that closed stdout is ignored.
fn print_line(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stdout(), "{text}");
}

fn discover_task_file() -> Result<PathBuf> {
    let mut directory =
        std::env::current_dir().context("Failed to determine the current directory")?;
    loop {
        let candidate = directory.join("frump.md");
        match fs::metadata(&candidate) {
            Ok(metadata) if metadata.is_file() => return Ok(candidate),
            Ok(_) => anyhow::bail!("{} exists but is not a regular file", candidate.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to inspect {}", candidate.display()))
            }
        }
        let sharded = directory.join("frump");
        if storage::is_sharded(&sharded) {
            return Ok(sharded);
        }
        let Some(parent) = directory.parent() else {
            anyhow::bail!("No frump.md found from the current directory through the filesystem root. Pass --board explicitly or run frump init.");
        };
        if parent == directory {
            anyhow::bail!("No frump.md found from the current directory through the filesystem root. Pass --board explicitly or run frump init.");
        }
        directory = parent.to_path_buf();
    }
}

/// Stage and commit exactly the board's files. A directory board is committed from inside
/// itself (`general.md` and `tasks`), so it works at the repository root too; the sibling
/// `<dir>.md` that `migrate` replaced is included while Git still tracks it.
fn commit_task_file(file: &Path, message: &str) -> Result<()> {
    let (directory, paths): (PathBuf, Vec<String>) = if let Some(root) = storage::sharded_root(file)
    {
        let mut paths = vec!["general.md".to_string()];
        // Git refuses a pathspec that matches nothing, and a new board has an empty `tasks/`.
        // `tasks` is committed when a task file is on disk, in the index or in HEAD, so the
        // deletion of the last task still commits.
        let has_task_files = fs::read_dir(root.join("tasks"))
            .with_context(|| format!("Failed to read {}", root.join("tasks").display()))?
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "md"));
        let git_lists = |args: &[&str]| -> Result<bool> {
            let output = std::process::Command::new("git")
                .current_dir(&root)
                .args(args)
                .stderr(std::process::Stdio::null())
                .output()
                .context("Failed to inspect the tasks directory")?;
            Ok(output.status.success() && !output.stdout.is_empty())
        };
        if has_task_files
            || git_lists(&["ls-files", "--", "tasks"])?
            || git_lists(&["ls-tree", "-r", "--name-only", "HEAD", "--", "tasks"])?
        {
            paths.push("tasks".to_string());
        }
        if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
            let legacy_source = format!("../{name}.md");
            let tracked_legacy_source = std::process::Command::new("git")
                .current_dir(&root)
                .args(["ls-files", "--error-unmatch", "--", &legacy_source])
                .stderr(std::process::Stdio::null())
                .output()
                .context("Failed to inspect the legacy board path")?
                .status
                .success();
            if tracked_legacy_source {
                paths.push(legacy_source);
            }
        }
        (root, paths)
    } else {
        let parent = file.parent().unwrap_or_else(|| Path::new("."));
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Task file path has no filename"))?;
        (parent.to_path_buf(), vec![name.to_string()])
    };
    let add = std::process::Command::new("git")
        .current_dir(&directory)
        .args(["add", "-A", "--"])
        .args(&paths)
        .status()
        .context("Failed to stage task file")?;
    if !add.success() {
        anyhow::bail!("Git could not stage {}", file.display());
    }
    let commit = std::process::Command::new("git")
        .current_dir(&directory)
        .args(["commit", "-m", message, "--"])
        .args(&paths)
        .status()
        .context("Failed to commit task file")?;
    if !commit.success() {
        anyhow::bail!("Git could not create a commit for {}", file.display());
    }
    Ok(())
}

fn print_assignment_announcement(task: &Task, assignee: &str) {
    report_notification(announce_assignment(task, assignee));
}

fn report_notification(result: Result<Option<String>>) {
    match result {
        Ok(Some(output)) => println!("Messaged with metateam: {output}"),
        Ok(None) => {}
        Err(error) => eprintln!("Warning: {}", notification_warning(&error)),
    }
}

fn normalize_search_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if matches!(character, '-' | '_') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// The first two body lines joined, cut at 80 characters (never inside a character).
fn body_snippet(body: &str) -> String {
    let snippet = body.lines().take(2).collect::<Vec<_>>().join(" ");
    if snippet.chars().count() > 80 {
        format!("{}...", snippet.chars().take(80).collect::<String>())
    } else {
        snippet
    }
}

fn warn_if_new_status(doc: &frump::FrumpDoc, status: &str) {
    let existing: std::collections::HashSet<_> = doc
        .tasks
        .tasks()
        .iter()
        .filter_map(|task| task.status())
        .collect();
    if !existing.contains(status) {
        eprintln!("Warning: '{}' is a new status. Prefer an existing status unless there is a documented reason.", status);
    }
}

fn touch_task(task: &mut Task) {
    mark_updated(task, &now_utc());
}

fn current_crew_agent() -> Option<String> {
    std::env::var("METATEAM_CREW_AGENT").ok()
}

fn read_document(file: &Path) -> Result<frump::FrumpDoc> {
    storage::read(file).with_context(|| format!("Failed to read {}", file.display()))
}

#[derive(Serialize)]
struct ListTask {
    id: u32,
    task_type: String,
    subject: String,
    body: String,
    /// Keeps the order of the properties in the task file.
    properties: IndexMap<String, String>,
}

impl From<&Task> for ListTask {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id.value(),
            task_type: task.task_type.as_str().to_string(),
            subject: task.subject.clone(),
            body: task.body.clone(),
            properties: task
                .properties
                .iter()
                .map(|property| (property.key.as_str().to_string(), property.value.clone()))
                .collect(),
        }
    }
}

fn parse_property_filter(filter: &str) -> Result<(PropertyKey, String)> {
    let (key, value) = filter
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("--property must use KEY=VALUE"))?;
    Ok((PropertyKey::new(key)?, value.to_string()))
}

fn sort_tasks(tasks: &mut [Task], key: &str, descending: bool) -> Result<()> {
    enum SortKey {
        Id,
        Property(PropertyKey),
    }
    let key = match key {
        "id" => SortKey::Id,
        "status" => SortKey::Property(PropertyKey::status()),
        "assignee" => SortKey::Property(PropertyKey::assigned_to()),
        "last-updated" => SortKey::Property(PropertyKey::new(LAST_UPDATED_PROPERTY)?),
        property => SortKey::Property(PropertyKey::new(property)?),
    };
    tasks.sort_by(|left, right| match &key {
        SortKey::Id => {
            let order = left.id.cmp(&right.id);
            if descending {
                order.reverse()
            } else {
                order
            }
        }
        SortKey::Property(property) => {
            match (left.get_property(property), right.get_property(property)) {
                (Some(a), Some(b)) => {
                    let order = a.cmp(b).then_with(|| left.id.cmp(&right.id));
                    if descending {
                        order.reverse()
                    } else {
                        order
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.id.cmp(&right.id),
            }
        }
    });
    Ok(())
}

fn default_migration_destination(source: &Path) -> PathBuf {
    source.with_extension("")
}

/// Print a task and its prerequisites as a tree. A prerequisite that is no longer on the board
/// prints with its state instead of failing.
fn print_prerequisite_tree(
    doc: &frump::FrumpDoc,
    history: &BoardHistory,
    task: &Task,
    depth: usize,
    seen: &mut std::collections::HashSet<TaskId>,
) {
    println!(
        "{}{} {} - {}",
        "  ".repeat(depth),
        task.task_type,
        task.id,
        task.subject
    );
    if !seen.insert(task.id) {
        return;
    }
    let indent = "  ".repeat(depth + 1);
    for prerequisite in prerequisites(task, doc, history) {
        match prerequisite {
            Prerequisite::Active { id, .. } => {
                let found = doc
                    .tasks
                    .find_by_id(id)
                    .expect("active prerequisite is on the board");
                print_prerequisite_tree(doc, history, found, depth + 1, seen);
            }
            Prerequisite::Closed(id) => match history.last_state(id) {
                Some(closed) => println!(
                    "{indent}{} {} - {} (closed)",
                    closed.task_type, closed.id, closed.subject
                ),
                None => println!("{indent}{id} (closed)"),
            },
            Prerequisite::Unknown(id) => println!("{indent}{id} (unknown task)"),
            Prerequisite::Malformed(raw) => println!("{indent}'{raw}' (not a task number)"),
        }
    }
}

fn search_similarity(query: &str, value: &str) -> f64 {
    let value = normalize_search_text(value);
    if value.contains(query) {
        return 1.0;
    }
    let query_words: Vec<_> = query.split_whitespace().collect();
    let words: Vec<_> = value.split_whitespace().collect();
    if query_words.is_empty() || words.is_empty() {
        return 0.0;
    }
    let window = query_words.len();
    (1..=words.len())
        .map(|start| {
            let end = (start + window).min(words.len());
            strsim::jaro_winkler(query, &words[start - 1..end].join(" "))
        })
        .fold(0.0, f64::max)
}

fn all_query_tokens_match(query: &str, subject: &str, body: &str) -> bool {
    let query_tokens: Vec<_> = query.split_whitespace().collect();
    let searchable = format!(
        "{} {}",
        normalize_search_text(subject),
        normalize_search_text(body)
    );
    let searchable_tokens: Vec<_> = searchable.split_whitespace().collect();
    query_tokens.iter().all(|query_token| {
        searchable_tokens
            .iter()
            .any(|candidate| strsim::normalized_levenshtein(query_token, candidate) >= 0.80)
    })
}

fn acquire_write_lock(file: &Path) -> Result<std::fs::File> {
    use fs2::FileExt;
    let lock_path = storage::sharded_root(file)
        .map(|root| root.join("general.md"))
        .unwrap_or_else(|| file.to_path_buf());
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("Failed to open {} for locking", lock_path.display()))?;
    lock.lock_exclusive()
        .with_context(|| format!("Failed to acquire write lock for {}", file.display()))?;
    Ok(lock)
}

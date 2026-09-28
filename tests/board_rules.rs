//! Board rules through the CLI: explicit argument names, input protection, board-aware Git
//! history, task numbers, prerequisites, closing, templates and bulk operations.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "frump-rules-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

/// Run frump in `dir` with no `metateam` on PATH, so no test can message a real crew.
fn frump(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_frump"))
        .current_dir(dir)
        .args(args)
        .env("PATH", "/usr/bin:/bin")
        .env_remove("METATEAM_CREW_AGENT")
        .output()
        .unwrap()
}

fn ok(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn git_repo(label: &str) -> PathBuf {
    let root = unique_root(label);
    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.name", "Frump Test"]);
    git(
        &root,
        &["config", "user.email", "frump-test@example.invalid"],
    );
    root
}

fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", message]);
}

fn directory_board(root: &Path, tasks: &[(u32, &str, &str, &str)]) -> PathBuf {
    let board = root.join("frump");
    fs::create_dir_all(board.join("tasks")).unwrap();
    fs::write(board.join("general.md"), "# Board\n\n## Team\n").unwrap();
    for (id, subject, status, extra) in tasks {
        write_task(&board, *id, subject, status, extra);
    }
    board
}

fn write_task(board: &Path, id: u32, subject: &str, status: &str, extra: &str) {
    fs::write(
        board.join(format!("tasks/{id}.md")),
        format!("### Task {id} - {subject}\n\nStatus: {status}\n{extra}"),
    )
    .unwrap();
}

fn single_board(root: &Path, tasks: &str) -> PathBuf {
    let board = root.join("frump.md");
    fs::write(&board, format!("# Board\n\n## Tasks\n\n{tasks}")).unwrap();
    board
}

fn added_id(stdout: &str) -> u32 {
    stdout
        .lines()
        .find(|line| line.starts_with("Added "))
        .and_then(|line| line.split_whitespace().nth(2))
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn old_spellings_and_short_flags_are_refused() {
    let root = unique_root("old-names");
    let board = single_board(&root, "### Task 1 - One\n\nStatus: done\n");
    let before = fs::read(&board).unwrap();
    for args in [
        vec!["--file", "frump.md", "list"],
        vec!["-f", "frump.md", "list"],
        vec!["update", "1", "--append-body", "x"],
        vec!["update", "1", "--append-body-notify", "x"],
        vec!["update", "1", "--append-body-msg", "x"],
        vec!["set", "1", "Status", "todo"],
        vec!["unset", "1", "Status"],
        vec!["assign", "1", "Ada"],
        vec!["next", "1"],
        vec!["search", "One"],
        vec!["search", "--text", "One", "-f"],
        vec!["closed"],
        vec!["deps", "1"],
        vec!["export", "-o", "out.json"],
        vec!["import", "out.json"],
        vec!["migrate", "--output", "board"],
        vec!["commit", "-m", "x"],
        vec!["check-conflicts"],
        vec!["resolve-conflicts"],
        vec!["list", "-s", "todo"],
        vec!["add", "Subject"],
        vec!["template", "add", "bug", "Fix it"],
        vec!["bulk", "close-by-status", "done"],
        vec!["-h"],
        vec!["list", "-h"],
    ] {
        let output = frump(&root, &args);
        assert!(!output.status.success(), "{args:?} must be refused");
        assert_eq!(
            fs::read(&board).unwrap(),
            before,
            "{args:?} changed the board"
        );
    }
    assert!(!root.join("out.json").exists());
    assert!(ok(&frump(&root, &["--help"])).contains("--board"));
    assert!(ok(&frump(&root, &["list", "--help"])).contains("--closed"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn update_with_nothing_to_do_and_export_with_an_unknown_format_fail() {
    let root = unique_root("empty-update");
    let board = single_board(&root, "### Task 1 - One\n\nStatus: todo\n");
    let before = fs::read(&board).unwrap();
    assert!(!frump(&root, &["update", "1"]).status.success());
    assert!(!frump(&root, &["export", "--format", "xml"])
        .status
        .success());
    assert_eq!(fs::read(&board).unwrap(), before);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn import_fills_the_board_and_never_writes_its_source() {
    let root = unique_root("import");
    let source = single_board(
        &root,
        "### Task 1 - Base\n\nStatus: todo\nAssigned To: Ada\n\n### Task 2 - Needs base\n\nStatus: todo\nDepends On: 1\n",
    );
    ok(&frump(&root, &["export", "--to", "tasks.json"]));
    let json = root.join("tasks.json");
    let json_bytes = fs::read(&json).unwrap();

    // Import into a new board: it is created, the source is unchanged, nothing is announced
    // (a restore assigns nobody; with a recording metateam a message would leave a file).
    let tools = root.join("tools");
    fs::create_dir(&tools).unwrap();
    let messages = root.join("messages");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metateam = tools.join("metateam");
        fs::write(
            &metateam,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$FRUMP_MESSAGE_ARGS\"\n",
        )
        .unwrap();
        fs::set_permissions(&metateam, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let imported = Command::new(env!("CARGO_BIN_EXE_frump"))
        .current_dir(&root)
        .args(["import", "--from", "tasks.json", "--to", "copy.md"])
        .env("PATH", &tools)
        .env("FRUMP_MESSAGE_ARGS", &messages)
        .output()
        .unwrap();
    ok(&imported);
    assert_eq!(fs::read(&json).unwrap(), json_bytes);
    assert!(
        !messages.exists(),
        "a restore must not announce assignments"
    );
    let copy = fs::read_to_string(root.join("copy.md")).unwrap();
    assert!(copy.contains("### Task 2 - Needs base") && copy.contains("Depends On: 1"));

    // --board and --to together are refused.
    let both = frump(
        &root,
        &[
            "--board",
            "frump.md",
            "import",
            "--from",
            "tasks.json",
            "--to",
            "copy.md",
        ],
    );
    assert!(!both.status.success());
    assert!(stderr(&both).contains("not both"), "{}", stderr(&both));

    // The board, another name for it, or the same file on both sides are refused unchanged.
    let board_bytes = fs::read(&source).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, root.join("alias.md")).unwrap();
        fs::hard_link(&source, root.join("hard.md")).unwrap();
    }
    for args in [
        vec!["import", "--from", "frump.md"],
        vec!["import", "--from", "alias.md"],
        vec!["import", "--from", "hard.md"],
        vec!["import", "--from", "tasks.json", "--to", "tasks.json"],
    ] {
        let refused = frump(&root, &args);
        assert!(!refused.status.success(), "{args:?} must be refused");
        assert!(
            stderr(&refused).contains("Refusing"),
            "{args:?}: {}",
            stderr(&refused)
        );
    }
    assert_eq!(fs::read(&source).unwrap(), board_bytes);
    assert_eq!(fs::read(&json).unwrap(), json_bytes);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn import_merge_renumbers_dependencies_above_closed_history() {
    let root = git_repo("import-merge");
    let board = directory_board(
        &root,
        &[(1, "Kept", "todo", ""), (5, "Closed later", "done", "")],
    );
    commit_all(&root, "board");
    ok(&frump(&root, &["close", "5"]));
    commit_all(&root, "close 5");
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    single_board(
        &source,
        "### Task 1 - Base\n\nStatus: todo\n\n### Task 2 - Needs base\n\nStatus: todo\nDepends On: 1\n",
    );
    ok(&frump(&source, &["export", "--to", "../merge.json"]));

    ok(&frump(
        &root,
        &["import", "--from", "merge.json", "--merge"],
    ));
    let base = fs::read_to_string(board.join("tasks/6.md")).unwrap();
    let needs = fs::read_to_string(board.join("tasks/7.md")).unwrap();
    assert!(base.contains("### Task 6 - Base"), "{base}");
    assert!(needs.contains("Depends On: 6"), "{needs}");
    assert!(ok(&frump(&root, &["validate"])).contains("Validation complete"));

    // A prerequisite outside the imported file refuses the whole import.
    single_board(
        &source,
        "### Task 3 - Orphan\n\nStatus: todo\nDepends On: 42\n",
    );
    ok(&frump(&source, &["export", "--to", "../orphan.json"]));
    let before = fs::read_dir(board.join("tasks")).unwrap().count();
    let refused = frump(&root, &["import", "--from", "orphan.json", "--merge"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("depends on task 42"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(fs::read_dir(board.join("tasks")).unwrap().count(), before);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn export_never_writes_the_board() {
    let root = unique_root("export-guard");
    let board = directory_board(&root, &[(1, "One", "todo", "")]);
    let task = fs::read(board.join("tasks/1.md")).unwrap();
    let general = fs::read(board.join("general.md")).unwrap();
    for target in [
        "frump/general.md",
        "frump/tasks/1.md",
        "frump/tasks/2.md",
        "frump",
    ] {
        // The board named by its directory, and by its general.md: one board either way.
        for named in ["frump", "frump/general.md"] {
            let refused = frump(&root, &["--board", named, "export", "--to", target]);
            assert!(
                !refused.status.success(),
                "--board {named} export --to {target} must be refused"
            );
        }
    }
    assert_eq!(fs::read(board.join("tasks/1.md")).unwrap(), task);
    assert_eq!(fs::read(board.join("general.md")).unwrap(), general);
    assert!(!board.join("tasks/2.md").exists());
    ok(&frump(&root, &["export", "--to", "out.json"]));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn directory_board_history_reserves_closed_numbers_from_any_directory() {
    let root = git_repo("dir-history");
    let board = directory_board(&root, &[(1, "One", "todo", ""), (2, "Two", "done", "")]);
    // A file whose name disagrees with its heading: the heading is the task number.
    fs::write(
        board.join("tasks/7.md"),
        "### Task 9 - Named seven\n\nStatus: done\n",
    )
    .unwrap();
    commit_all(&root, "board");
    let sub = root.join("docs/deep");
    fs::create_dir_all(&sub).unwrap();

    ok(&frump(&sub, &["--board", "../../frump", "close", "9"]));
    ok(&frump(&root, &["close", "2"]));
    commit_all(&root, "close");
    let added = ok(&frump(
        &sub,
        &["--board", "../../frump", "add", "--subject", "Three"],
    ));
    assert_eq!(added_id(&added), 10, "{added}");
    let closed = ok(&frump(
        &sub,
        &["--board", "../../frump", "list", "--closed"],
    ));
    assert!(
        closed.contains("Task 2 - Two") && closed.contains("Task 9 - Named seven"),
        "{closed}"
    );
    let stats = ok(&frump(&root, &["stats"]));
    assert!(stats.contains("Closed tasks: 2"), "{stats}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_board_below_the_repository_root_has_history() {
    let root = git_repo("nested-board");
    let docs = root.join("docs");
    fs::create_dir(&docs).unwrap();
    single_board(
        &docs,
        "### Task 1 - One\n\nStatus: todo\n\n### Task 2 - Two\n\nStatus: done\n",
    );
    commit_all(&root, "board");
    ok(&frump(&docs, &["close", "2"]));
    commit_all(&root, "close");
    let history = ok(&frump(&docs, &["history", "2"]));
    assert!(
        history.contains("Created") && history.contains("Deleted"),
        "{history}"
    );
    assert!(ok(&frump(&docs, &["list", "--closed"])).contains("Task 2 - Two"));
    assert_eq!(
        added_id(&ok(&frump(&docs, &["add", "--subject", "Three"]))),
        3
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn history_events_follow_parents_and_ignore_unrelated_commits() {
    let root = git_repo("history-events");
    let board = single_board(&root, "### Task 1 - One\n\nStatus: todo\n");
    commit_all(&root, "create");
    fs::write(root.join("other.txt"), "x").unwrap();
    commit_all(&root, "unrelated");
    fs::write(&board, "# Board\n\n## Tasks\n\n### Task 1 - One\n\nStatus: todo\n\n### Task 2 - Two\n\nStatus: todo\n").unwrap();
    commit_all(&root, "another task");
    // Remove, then recreate with identical text: two events.
    fs::write(
        &board,
        "# Board\n\n## Tasks\n\n### Task 2 - Two\n\nStatus: todo\n",
    )
    .unwrap();
    commit_all(&root, "remove");
    fs::write(&board, "# Board\n\n## Tasks\n\n### Task 1 - One\n\nStatus: todo\n\n### Task 2 - Two\n\nStatus: todo\n").unwrap();
    commit_all(&root, "recreate");
    // A branch changes task 1; the merge brings it in without a change of its own.
    git(&root, &["checkout", "-qb", "side"]);
    fs::write(&board, "# Board\n\n## Tasks\n\n### Task 1 - One\n\nStatus: working\n\n### Task 2 - Two\n\nStatus: todo\n").unwrap();
    commit_all(&root, "side change");
    git(&root, &["checkout", "-q", "-"]);
    fs::write(root.join("other.txt"), "y").unwrap();
    commit_all(&root, "main change");
    git(&root, &["merge", "-q", "--no-edit", "side"]);

    let history = ok(&frump(&root, &["history", "1"]));
    let events: Vec<&str> = history
        .lines()
        .filter(|line| line.starts_with('✓') || line.starts_with('•') || line.starts_with('✗'))
        .map(|line| line.split(" by ").next().unwrap())
        .collect();
    assert_eq!(
        events,
        ["✓ Created", "✗ Deleted", "✓ Created", "• Modified"],
        "{history}"
    );
    let messages: Vec<&str> = history
        .lines()
        .filter_map(|line| line.strip_prefix("  Message: "))
        .collect();
    assert_eq!(messages, ["create", "remove", "recreate", "side change"]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_last_state_of_a_closed_task_follows_commit_order() {
    let root = git_repo("last-state");
    // The oldest state differs from the newest, and the newest reuses an earlier blob.
    let board = directory_board(&root, &[(1, "One", "todo", "")]);
    commit_all(&root, "todo");
    write_task(&board, 1, "One", "done", "");
    commit_all(&root, "done");
    write_task(&board, 1, "One", "todo", "");
    commit_all(&root, "todo again");
    write_task(&board, 1, "One", "done", "");
    commit_all(&root, "done again, same blob as the second commit");
    fs::remove_file(board.join("tasks/1.md")).unwrap();
    commit_all(&root, "removed");
    let closed = ok(&frump(&root, &["list", "--closed", "--format", "json"]));
    let closed: serde_json::Value = serde_json::from_str(&closed).unwrap();
    assert_eq!(closed[0]["properties"]["Status"], "done", "{closed}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_migrated_board_keeps_its_history_and_a_custom_destination_warns() {
    let root = git_repo("migrate-history");
    single_board(
        &root,
        "### Task 1 - One\n\nStatus: todo\n\n### Task 2 - Two\n\nStatus: done\n",
    );
    commit_all(&root, "board");
    ok(&frump(&root, &["close", "2"]));
    commit_all(&root, "close");
    ok(&frump(&root, &["migrate"]));
    ok(&frump(&root, &["commit", "--message", "migrate"]));
    assert_eq!(
        added_id(&ok(&frump(&root, &["add", "--subject", "Three"]))),
        3
    );

    let other = git_repo("migrate-custom");
    single_board(&other, "### Task 1 - One\n\nStatus: todo\n");
    let moved = frump(&other, &["migrate", "--to", "work"]);
    ok(&moved);
    assert!(
        stderr(&moved).contains("does not carry the Git history"),
        "{}",
        stderr(&moved)
    );
    assert!(other.join("work/general.md").is_file());
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(other);
}

#[test]
fn an_unreadable_history_stops_add_and_close_before_any_write() {
    let root = git_repo("bad-history");
    let board = single_board(&root, "### Task x - Broken\n");
    commit_all(&root, "a board that does not parse");
    fs::write(
        &board,
        "# Board\n\n## Tasks\n\n### Task 1 - One\n\nStatus: done\n",
    )
    .unwrap();
    commit_all(&root, "repaired");
    let before = fs::read(&board).unwrap();
    for args in [vec!["add", "--subject", "Two"], vec!["close", "1"]] {
        let refused = frump(&root, &args);
        assert!(!refused.status.success(), "{args:?}");
        assert!(
            stderr(&refused).contains("Failed to read the board in commit"),
            "{}",
            stderr(&refused)
        );
        assert_eq!(fs::read(&board).unwrap(), before, "{args:?}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn task_numbers_stop_at_the_largest_number_and_start_in_an_empty_repository() {
    let root = unique_root("largest-number");
    let board = single_board(&root, "### Task 4294967295 - Last\n\nStatus: todo\n");
    let before = fs::read(&board).unwrap();
    let refused = frump(&root, &["add", "--subject", "Beyond"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("largest possible"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(fs::read(&board).unwrap(), before);

    let empty = git_repo("unborn");
    single_board(&empty, "### Task 3 - Three\n\nStatus: todo\n");
    assert_eq!(
        added_id(&ok(&frump(&empty, &["add", "--subject", "Four"]))),
        4
    );
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(empty);
}

#[test]
fn prerequisites_are_satisfied_only_when_done_or_closed() {
    let root = git_repo("prerequisites");
    directory_board(
        &root,
        &[
            (1, "Closed prerequisite", "done", ""),
            (2, "Active done", "done", ""),
            (3, "Active todo", "todo", ""),
            (4, "Needs closed and done", "todo", "Depends On: 1, 2\n"),
            (5, "Needs todo", "todo", "Depends On: 3\n"),
            (6, "Needs unknown", "todo", "Depends On: 99\n"),
            (7, "Needs malformed", "todo", "Depends On: soon\n"),
        ],
    );
    commit_all(&root, "board");
    ok(&frump(&root, &["close", "1"]));
    let ready = ok(&frump(&root, &["ready"]));
    assert!(ready.contains("Task 4 - Needs closed and done"), "{ready}");
    for blocked in ["Task 5", "Task 6", "Task 7"] {
        assert!(!ready.contains(blocked), "{blocked} is not ready: {ready}");
    }
    let validate = frump(&root, &["validate"]);
    assert!(!validate.status.success());
    let report = String::from_utf8_lossy(&validate.stdout);
    assert!(
        report.contains("Task 6 references unknown dependency 99"),
        "{report}"
    );
    assert!(
        report.contains("Task 7 has invalid Depends On value 'soon'"),
        "{report}"
    );
    assert!(
        !report.contains("dependency 1"),
        "a closed prerequisite is valid: {report}"
    );
    let tree = ok(&frump(&root, &["depends-on", "4"]));
    assert!(
        tree.contains("Task 1 - Closed prerequisite (closed)"),
        "{tree}"
    );
    assert!(ok(&frump(&root, &["depends-on", "6"])).contains("99 (unknown task)"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validate_fails_on_duplicate_numbers_and_on_a_board_that_does_not_parse() {
    let root = unique_root("validate-exit");
    single_board(
        &root,
        "### Task 1 - A\n\nStatus: todo\n\n### Task 1 - B\n\nStatus: todo\n",
    );
    let duplicate = frump(&root, &["validate"]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stdout).contains("duplicate task IDs"));
    single_board(&root, "### Task x - Broken\n");
    let broken = frump(&root, &["validate"]);
    assert!(!broken.status.success());
    assert!(String::from_utf8_lossy(&broken.stdout).contains("✗ Validation failed"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn close_never_refuses_for_git_and_prints_text_that_git_does_not_have() {
    let root = git_repo("close-recovery");
    let board = single_board(
        &root,
        "### Task 1 - Committed\n\nStatus: done\n\n### Task 2 - Edited later\n\nStatus: todo\n\n### Task 5 - Dependent\n\nStatus: todo\nDepends On: 3\n",
    );
    commit_all(&root, "board");
    // Task 3 is new and staged; task 2 gets an edit that is not committed.
    let text = fs::read_to_string(&board).unwrap();
    fs::write(
        &board,
        text.replace(
            "### Task 2 - Edited later\n\nStatus: todo",
            "### Task 2 - Edited later\n\nFinal report.\n\nStatus: done",
        ) + "\n### Task 3 - Brand new\n\nOnly in the working tree.\n\nStatus: done\n",
    )
    .unwrap();
    git(&root, &["add", "frump.md"]);

    let committed = frump(&root, &["close", "1"]);
    ok(&committed);
    assert!(stderr(&committed).is_empty(), "{}", stderr(&committed));

    let edited = frump(&root, &["close", "2"]);
    ok(&edited);
    let warning = stderr(&edited);
    assert!(
        warning.contains("HEAD has another version of it"),
        "{warning}"
    );
    assert!(warning.contains("Final report."), "{warning}");
    assert!(
        !warning.contains("given out again"),
        "task 2 is in history: {warning}"
    );

    let new = frump(&root, &["close", "3"]);
    ok(&new);
    let warning = stderr(&new);
    assert!(warning.contains("### Task 3 - Brand new"), "{warning}");
    assert!(warning.contains("Only in the working tree."), "{warning}");
    assert!(warning.contains("can be given out again"), "{warning}");
    assert!(
        warning.contains("These tasks now depend on an unknown task: 5"),
        "{warning}"
    );
    assert!(!fs::read_to_string(&board).unwrap().contains("Brand new"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn close_of_a_directory_task_restored_after_a_committed_removal_keeps_its_number() {
    let root = git_repo("close-restored");
    let board = directory_board(&root, &[(1, "One", "done", ""), (2, "Two", "todo", "")]);
    commit_all(&root, "board");
    let saved = fs::read(board.join("tasks/1.md")).unwrap();
    fs::remove_file(board.join("tasks/1.md")).unwrap();
    commit_all(&root, "removed");
    fs::write(board.join("tasks/1.md"), saved).unwrap();

    let closed = frump(&root, &["close", "1"]);
    ok(&closed);
    let warning = stderr(&closed);
    assert!(
        warning.contains("HEAD does not have this task"),
        "{warning}"
    );
    assert!(!warning.contains("given out again"), "{warning}");
    assert_eq!(
        added_id(&ok(&frump(&root, &["add", "--subject", "Three"]))),
        3
    );

    // A board outside Git closes too, with the warning.
    let loose = unique_root("close-outside-git");
    single_board(&loose, "### Task 1 - Loose\n\nStatus: done\n");
    let outside = frump(&loose, &["close", "1"]);
    ok(&outside);
    assert!(
        stderr(&outside).contains("the board is not in Git"),
        "{}",
        stderr(&outside)
    );
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(loose);
}

#[test]
fn bulk_close_is_all_or_nothing_and_uses_the_close_rule() {
    let root = git_repo("bulk-close");
    let board = directory_board(
        &root,
        &[
            (1, "Done", "done", ""),
            (2, "Done but blocked", "done", "Depends On: 3\n"),
            (3, "Open", "todo", ""),
        ],
    );
    commit_all(&root, "board");
    let before: Vec<Vec<u8>> = (1..=3)
        .map(|id| fs::read(board.join(format!("tasks/{id}.md"))).unwrap())
        .collect();
    let refused = frump(&root, &["bulk", "close", "--with-status", "done"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("dependency 3 is not done"),
        "{}",
        stderr(&refused)
    );
    for (id, bytes) in (1..=3).zip(&before) {
        assert_eq!(
            &fs::read(board.join(format!("tasks/{id}.md"))).unwrap(),
            bytes
        );
    }
    write_task(&board, 3, "Open", "done", "");
    commit_all(&root, "done");
    let closed = ok(&frump(&root, &["bulk", "close", "--with-status", "done"]));
    assert!(closed.contains("Closed 3 task(s)"), "{closed}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn bulk_assign_and_set_select_with_explicit_names() {
    let root = unique_root("bulk-names");
    let board = single_board(
        &root,
        "### Bug 1 - A\n\nStatus: review\n\n### Task 2 - B\n\nStatus: review\n\n### Bug 3 - C\n\nStatus: todo\n",
    );
    ok(&frump(
        &root,
        &["bulk", "assign", "--with-type", "Bug", "--assignee", "Ada"],
    ));
    ok(&frump(
        &root,
        &[
            "bulk",
            "set",
            "--with-status",
            "review",
            "--property",
            "Review",
            "--value",
            "pending",
        ],
    ));
    let doc = frump::parser::parse(&fs::read_to_string(&board).unwrap()).unwrap();
    let task = |id| {
        doc.tasks
            .find_by_id(frump::TaskId::new(id).unwrap())
            .unwrap()
    };
    let review = frump::PropertyKey::new("Review").unwrap();
    assert_eq!(task(1).assignee(), Some("Ada"));
    assert_eq!(task(2).assignee(), None);
    assert_eq!(task(3).assignee(), Some("Ada"));
    assert_eq!(task(1).get_property(&review), Some("pending"));
    assert_eq!(task(2).get_property(&review), Some("pending"));
    assert_eq!(task(3).get_property(&review), None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_relative_board_commits_and_general_md_names_the_same_board() {
    let root = git_repo("relative-board");
    let board = directory_board(&root, &[(1, "One", "todo", "")]);
    ok(&frump(
        &root,
        &["--board", "frump", "commit", "--message", "relative"],
    ));
    ok(&frump(
        &root,
        &[
            "--board",
            "frump/general.md",
            "set",
            "1",
            "--property",
            "Status",
            "--value",
            "working",
        ],
    ));
    assert!(fs::read_to_string(board.join("tasks/1.md"))
        .unwrap()
        .contains("Status: working"));
    ok(&frump(
        &root,
        &[
            "--board",
            "frump/general.md",
            "commit",
            "--message",
            "same board",
        ],
    ));
    let log = Command::new("git")
        .current_dir(&root)
        .args(["log", "--format=%s"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout),
        "same board\nrelative\n"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_board_write_keeps_files_that_are_not_tasks() {
    let root = unique_root("keep-files");
    let board = directory_board(&root, &[(1, "One", "todo", "")]);
    fs::write(board.join("tasks/123.bak"), "backup").unwrap();
    fs::write(board.join("tasks/124.txt"), "notes").unwrap();
    ok(&frump(
        &root,
        &["set", "1", "--property", "Status", "--value", "working"],
    ));
    assert!(board.join("tasks/123.bak").is_file());
    assert!(board.join("tasks/124.txt").is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn renumber_duplicates_skips_closed_numbers_and_commits_only_the_board() {
    let root = git_repo("renumber");
    let board = single_board(
        &root,
        "### Task 1 - A\n\nStatus: todo\n\n### Task 2 - B\n\nStatus: done\n",
    );
    commit_all(&root, "board");
    ok(&frump(&root, &["close", "2"]));
    commit_all(&root, "close");
    let mut text = fs::read_to_string(&board).unwrap();
    text.push_str("\n### Task 1 - Duplicate\n\nStatus: todo\n");
    fs::write(&board, text).unwrap();
    fs::write(root.join("unrelated.txt"), "staged by someone else").unwrap();
    git(&root, &["add", "unrelated.txt"]);

    let output = ok(&frump(&root, &["renumber-duplicates", "--commit"]));
    assert!(output.contains("1 → 3: Duplicate"), "{output}");
    let files = Command::new("git")
        .current_dir(&root)
        .args(["show", "--name-only", "--format=", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&files.stdout), "frump.md\n");
    let staged = Command::new("git")
        .current_dir(&root)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&staged.stdout), "unrelated.txt\n");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn templates_live_next_to_the_board_and_create_tasks() {
    let root = unique_root("templates");
    let project = root.join("project");
    fs::create_dir(&project).unwrap();
    fs::write(
        project.join("frump.md"),
        "# Board\n\n## Team\n\n* Default Person <default@example.com>\n\n## Tasks\n",
    )
    .unwrap();
    let elsewhere = root.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    let board = "../project/frump.md";
    ok(&frump(
        &elsewhere,
        &[
            "--board",
            board,
            "template",
            "add",
            "--name",
            "bug",
            "--type",
            "Bug",
            "--subject",
            "Fix {component} issue",
            "--body",
            "Seen in {component}: {detail}",
            "--property",
            "Priority=2",
            "--property",
            "Assigned To=hipp",
        ],
    ));
    assert!(project.join(".frump_templates.json").is_file());
    assert!(!elsewhere.join(".frump_templates.json").exists());

    let added = ok(&frump(
        &elsewhere,
        &[
            "--board",
            board,
            "add",
            "--template",
            "bug",
            "--fill",
            "component=parser",
            "--fill",
            "detail=crash on {empty} input",
        ],
    ));
    let id = added_id(&added);
    let shown = ok(&frump(
        &elsewhere,
        &["--board", board, "show", &id.to_string()],
    ));
    assert!(shown.contains("### Bug 1 - Fix parser issue"), "{shown}");
    assert!(
        shown.contains("Seen in parser: crash on {empty} input"),
        "{shown}"
    );
    assert!(
        shown.contains("Priority: 2\nAssigned To: hipp"),
        "template assignee before the board default: {shown}"
    );

    let before = fs::read(project.join("frump.md")).unwrap();
    for (args, reason) in [
        (
            vec!["add", "--template", "bug", "--fill", "component=parser"],
            "detail",
        ),
        (
            vec![
                "add",
                "--template",
                "bug",
                "--fill",
                "component=a",
                "--fill",
                "detail=b",
                "--fill",
                "extra=c",
            ],
            "extra",
        ),
        (
            vec![
                "add",
                "--template",
                "bug",
                "--fill",
                "component=a",
                "--fill",
                "component=b",
            ],
            "more than once",
        ),
        (vec!["add", "--template", "missing"], "not found"),
    ] {
        let mut full = vec!["--board", board];
        full.extend(args);
        let refused = frump(&elsewhere, &full);
        assert!(!refused.status.success(), "{full:?}");
        assert!(
            stderr(&refused).contains(reason),
            "{full:?}: {}",
            stderr(&refused)
        );
    }
    assert!(!frump(
        &elsewhere,
        &[
            "--board",
            board,
            "add",
            "--template",
            "bug",
            "--subject",
            "x"
        ]
    )
    .status
    .success());
    assert_eq!(fs::read(project.join("frump.md")).unwrap(), before);

    let shown = ok(&frump(
        &elsewhere,
        &["--board", board, "template", "show", "--name", "bug"],
    ));
    assert!(
        shown.contains("Priority: 2") && shown.contains("Assigned To: hipp"),
        "{shown}"
    );
    ok(&frump(
        &elsewhere,
        &["--board", board, "template", "remove", "--name", "bug"],
    ));
    assert!(
        ok(&frump(&elsewhere, &["--board", board, "template", "list"]))
            .contains("No templates found.")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn search_show_body_cuts_on_a_character_boundary() {
    let root = unique_root("search-snippet");
    single_board(
        &root,
        &format!(
            "### Task 1 - census\n\n{}ș rest\n\nStatus: todo\n",
            "a".repeat(79)
        ),
    );
    let found = ok(&frump(
        &root,
        &["search", "--text", "census", "--show-body"],
    ));
    assert!(
        found.contains(&format!("  {}ș...", "a".repeat(79))),
        "{found}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn list_json_keeps_the_property_order_of_the_task_file() {
    let root = unique_root("list-order");
    single_board(&root, "### Task 1 - A\n\nStatus: todo\nZeta: 1\nAlpha: 2\n");
    let listed = ok(&frump(&root, &["list", "--format", "json"]));
    let (zeta, alpha) = (
        listed.find("\"Zeta\"").unwrap(),
        listed.find("\"Alpha\"").unwrap(),
    );
    assert!(
        listed.find("\"Status\"").unwrap() < zeta && zeta < alpha,
        "{listed}"
    );
    let _ = fs::remove_dir_all(root);
}

/// Start `frump web` with no metateam on PATH; its output pipes close after the banner, so a
/// handler that printed would fail.
#[cfg(unix)]
fn start_web(dir: &Path) -> (std::process::Child, u16) {
    use std::io::BufRead;
    let mut server = Command::new(env!("CARGO_BIN_EXE_frump"))
        .current_dir(dir)
        .args(["web", "--port", "0"])
        .env("PATH", "/usr/bin:/bin")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut banner = String::new();
    std::io::BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut banner)
        .unwrap();
    drop(server.stderr.take());
    let port = banner.trim().rsplit(':').next().unwrap().parse().unwrap();
    (server, port)
}

#[cfg(unix)]
fn delete(port: u16, id: u32) -> (String, String) {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "DELETE /api/tasks/{id} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_string(), body.to_string())
}

#[cfg(unix)]
#[test]
fn web_delete_is_close_and_returns_the_recovery_text() {
    let root = git_repo("web-delete");
    let board = single_board(
        &root,
        "### Task 1 - Open\n\nStatus: todo\n\n### Task 2 - Committed\n\nStatus: done\n",
    );
    commit_all(&root, "board");
    let mut text = fs::read_to_string(&board).unwrap();
    text = text.replace("## Tasks\n", "## Next\n\n* 2\n\n## Tasks\n");
    text.push_str("\n### Task 3 - Never committed\n\nOnly here.\n\nStatus: done\n");
    fs::write(&board, text).unwrap();
    let (mut server, port) = start_web(&root);

    let before = fs::read(&board).unwrap();
    let refused = delete(port, 1);
    let after_refusal = fs::read(&board).unwrap();
    let unrecorded = delete(port, 3);
    let recorded = delete(port, 2);
    let _ = server.kill();
    let _ = server.wait();

    assert!(refused.0.contains("400"), "{refused:?}");
    assert!(refused.1.contains("Status must be done"), "{refused:?}");
    assert_eq!(after_refusal, before);
    assert!(unrecorded.0.contains("200"), "{unrecorded:?}");
    let json: serde_json::Value = serde_json::from_str(&unrecorded.1).unwrap();
    let warning = json["warning"].as_str().unwrap();
    assert!(
        warning.contains("### Task 3 - Never committed") && warning.contains("Only here."),
        "{warning}"
    );
    assert!(recorded.0.contains("200"), "{recorded:?}");
    let json: serde_json::Value = serde_json::from_str(&recorded.1).unwrap();
    assert!(json.get("warning").is_none(), "{}", recorded.1);
    let saved = fs::read_to_string(&board).unwrap();
    assert!(
        !saved.contains("Task 2 -") && !saved.contains("Task 3 -"),
        "{saved}"
    );
    assert!(
        !saved.contains("## Next"),
        "the closed task leaves Next: {saved}"
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn export_refuses_a_hard_link_to_any_file_of_a_directory_board() {
    let root = unique_root("export-hard-link");
    let board = directory_board(&root, &[(1, "One", "todo", "")]);
    fs::hard_link(board.join("tasks/1.md"), root.join("task-link.json")).unwrap();
    fs::hard_link(board.join("general.md"), root.join("general-link.json")).unwrap();
    let task = fs::read(board.join("tasks/1.md")).unwrap();
    let general = fs::read(board.join("general.md")).unwrap();
    for target in ["task-link.json", "general-link.json"] {
        let refused = frump(&root, &["--board", "frump", "export", "--to", target]);
        assert!(
            !refused.status.success(),
            "export --to {target} must be refused"
        );
        assert!(
            stderr(&refused).contains("Refusing"),
            "{}",
            stderr(&refused)
        );
    }
    assert_eq!(fs::read(board.join("tasks/1.md")).unwrap(), task);
    assert_eq!(fs::read(board.join("general.md")).unwrap(), general);
    assert!(ok(&frump(&root, &["validate"])).contains("Validation complete"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_write_moves_a_task_stored_under_another_name_to_its_number() {
    let root = unique_root("task-file-name");
    let board = directory_board(&root, &[(1, "One", "todo", "")]);
    fs::write(
        board.join("tasks/foo.md"),
        "### Task 9 - Named foo\n\nStatus: todo\n",
    )
    .unwrap();
    ok(&frump(
        &root,
        &["set", "9", "--property", "Status", "--value", "working"],
    ));
    assert!(!board.join("tasks/foo.md").exists());
    assert!(fs::read_to_string(board.join("tasks/9.md"))
        .unwrap()
        .contains("Status: working"));
    assert!(ok(&frump(&root, &["validate"])).contains("Validation complete: 2 tasks"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_directory_board_with_duplicate_numbers_refuses_writes_until_renumbered() {
    let root = unique_root("dir-duplicates");
    let board = directory_board(&root, &[(1, "One", "todo", ""), (9, "Nine", "todo", "")]);
    fs::write(
        board.join("tasks/foo.md"),
        "### Task 9 - Also nine\n\nStatus: todo\n",
    )
    .unwrap();
    let before: Vec<_> = ["1.md", "9.md", "foo.md"]
        .iter()
        .map(|name| fs::read(board.join("tasks").join(name)).unwrap())
        .collect();
    let refused = frump(
        &root,
        &["set", "1", "--property", "Status", "--value", "working"],
    );
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("renumber-duplicates"),
        "{}",
        stderr(&refused)
    );
    for (name, bytes) in ["1.md", "9.md", "foo.md"].iter().zip(&before) {
        assert_eq!(
            &fs::read(board.join("tasks").join(name)).unwrap(),
            bytes,
            "{name}"
        );
    }
    ok(&frump(&root, &["renumber-duplicates"]));
    let listed = ok(&frump(&root, &["list"]));
    assert!(
        listed.contains("- Nine") && listed.contains("- Also nine"),
        "{listed}"
    );
    assert!(ok(&frump(&root, &["validate"])).contains("Validation complete: 3 tasks"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn import_refuses_an_invalid_property_key_before_writing() {
    let root = unique_root("import-invalid-key");
    let board = single_board(&root, "### Task 1 - Kept\n\nStatus: todo\n");
    let before = fs::read(&board).unwrap();
    fs::write(
        root.join("bad.json"),
        r##"{"header":"# B\n\n","team":[],"tasks":[{"id":1,"task_type":"Task","subject":"S","body":"","properties":{"lower case":"evidence","Status":"todo"}}]}"##,
    )
    .unwrap();
    for args in [
        vec!["import", "--from", "bad.json"],
        vec!["import", "--from", "bad.json", "--merge"],
    ] {
        let refused = frump(&root, &args);
        assert!(!refused.status.success(), "{args:?}");
        assert!(
            stderr(&refused).contains("invalid property key 'lower case'"),
            "{}",
            stderr(&refused)
        );
        assert_eq!(fs::read(&board).unwrap(), before, "{args:?}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn update_reports_changes_only_after_they_are_saved() {
    let root = unique_root("update-report");
    let board = single_board(&root, "### Task 1 - Original\n\nStatus: todo\n");
    let before = fs::read(&board).unwrap();
    let refused = frump(
        &root,
        &["update", "1", "--subject", "Changed", "--append", "  "],
    );
    assert!(!refused.status.success());
    assert!(!String::from_utf8_lossy(&refused.stdout).contains("Updated subject"));
    assert_eq!(fs::read(&board).unwrap(), before);
    let saved = ok(&frump(
        &root,
        &["update", "1", "--subject", "Changed", "--append", "Note"],
    ));
    assert_eq!(
        saved,
        "Updated subject for task 1\nAppended body for task 1\n"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn close_refuses_a_number_that_two_tasks_hold() {
    let root = unique_root("close-duplicates");
    let board = single_board(
        &root,
        "### Task 1 - First\n\nFIRST EVIDENCE.\n\nStatus: done\n\n### Task 1 - Second\n\nSECOND EVIDENCE.\n\nStatus: done\n",
    );
    let before = fs::read(&board).unwrap();
    for args in [
        vec!["close", "1"],
        vec!["bulk", "close", "--with-status", "done"],
    ] {
        let refused = frump(&root, &args);
        assert!(!refused.status.success(), "{args:?}");
        assert!(
            stderr(&refused).contains("2 tasks have this number"),
            "{}",
            stderr(&refused)
        );
        assert_eq!(fs::read(&board).unwrap(), before, "{args:?}");
    }

    let shards = unique_root("close-duplicate-shards");
    let dir = directory_board(&shards, &[(1, "First", "done", "")]);
    fs::write(
        dir.join("tasks/other.md"),
        "### Task 1 - Second\n\nStatus: done\n",
    )
    .unwrap();
    let refused = frump(&shards, &["close", "1"]);
    assert!(!refused.status.success());
    assert!(dir.join("tasks/1.md").is_file() && dir.join("tasks/other.md").is_file());
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(shards);
}

/// Run frump with stdout connected to a pipe whose reader is already closed.
#[cfg(unix)]
fn frump_with_closed_stdout(dir: &Path, args: &[&str]) -> Output {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    Command::new(env!("CARGO_BIN_EXE_frump"))
        .current_dir(dir)
        .args(args)
        .env("PATH", "/usr/bin:/bin")
        .stdout(writer)
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn a_closed_stdout_never_stops_the_recovery_text() {
    let root = unique_root("close-closed-stdout");
    let board = single_board(
        &root,
        "### Task 1 - One\n\nONLY COPY ONE.\n\nStatus: done\n\n### Task 2 - Two\n\nONLY COPY TWO.\n\nStatus: done\n\n### Task 3 - Three\n\nONLY COPY THREE.\n\nStatus: done\n",
    );
    let single = frump_with_closed_stdout(&root, &["close", "1"]);
    let warning = stderr(&single);
    assert!(warning.contains("ONLY COPY ONE."), "{warning}");
    assert!(!warning.contains("panicked"), "{warning}");
    let bulk = frump_with_closed_stdout(&root, &["bulk", "close", "--with-status", "done"]);
    let warning = stderr(&bulk);
    assert!(
        warning.contains("ONLY COPY TWO.") && warning.contains("ONLY COPY THREE."),
        "{warning}"
    );
    assert!(!warning.contains("panicked"), "{warning}");
    assert!(!fs::read_to_string(&board).unwrap().contains("### Task"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_directory_board_at_the_repository_root_has_history_and_commits() {
    let root = git_repo("root-board");
    fs::create_dir(root.join("tasks")).unwrap();
    fs::write(root.join("general.md"), "# Board\n\n## Team\n").unwrap();
    write_task(&root, 1, "One", "todo", "");
    write_task(&root, 2, "Two", "done", "");
    assert_eq!(
        added_id(&ok(&frump(
            &root,
            &["--board", ".", "add", "--subject", "Before commits"]
        ))),
        3,
        "an unborn HEAD is an empty history"
    );
    fs::remove_file(root.join("tasks/3.md")).unwrap();
    commit_all(&root, "board");
    ok(&frump(&root, &["--board", ".", "close", "2"]));
    fs::write(root.join("unrelated.txt"), "staged by someone else").unwrap();
    git(&root, &["add", "unrelated.txt"]);
    let absolute = root.to_str().unwrap();
    ok(&frump(
        &root,
        &["--board", absolute, "commit", "--message", "close 2"],
    ));
    let files = Command::new("git")
        .current_dir(&root)
        .args(["show", "--name-status", "--format=", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&files.stdout), "D\ttasks/2.md\n");
    let history = ok(&frump(&root, &["--board", "general.md", "history", "2"]));
    assert!(
        history.contains("Created") && history.contains("Deleted"),
        "{history}"
    );
    assert_eq!(
        added_id(&ok(&frump(
            &root,
            &["--board", ".", "add", "--subject", "Three"]
        ))),
        3
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn history_reads_task_content_only_in_both_layouts() {
    // Single file: a commit with a bad Team email and a bad Next entry still counts its tasks.
    let single = git_repo("history-tasks-only-file");
    let board = single.join("frump.md");
    fs::write(
        &board,
        "# Board\n\n## Team\n\n* Bad <not-email>\n\n## Next\n\n* soon\n\n## Tasks\n\n### Task 7 - Seven\n\nStatus: done\n",
    )
    .unwrap();
    commit_all(&single, "bad metadata");
    fs::write(
        &board,
        "# Board\n\n## Tasks\n\n### Task 1 - One\n\nStatus: todo\n",
    )
    .unwrap();
    commit_all(&single, "fixed");
    assert_eq!(
        added_id(&ok(&frump(&single, &["add", "--subject", "Next"]))),
        8
    );

    // Directory: a bad general.md in history is not read; task 7 of that commit still counts.
    let shards = git_repo("history-tasks-only-dir");
    let dir = directory_board(&shards, &[(7, "Seven", "done", "")]);
    fs::write(
        dir.join("general.md"),
        "# Board\n\n## Team\n\n* Bad <not-email>\n\n## Next\n\n* soon\n",
    )
    .unwrap();
    commit_all(&shards, "bad metadata");
    fs::write(dir.join("general.md"), "# Board\n\n## Team\n").unwrap();
    fs::remove_file(dir.join("tasks/7.md")).unwrap();
    write_task(&dir, 1, "One", "done", "");
    commit_all(&shards, "fixed");
    assert_eq!(
        added_id(&ok(&frump(&shards, &["add", "--subject", "Next"]))),
        8
    );

    // A task that does not parse still stops add and close before any write.
    fs::write(dir.join("tasks/3.md"), "### Task x - Broken\n").unwrap();
    commit_all(&shards, "broken task");
    fs::remove_file(dir.join("tasks/3.md")).unwrap();
    commit_all(&shards, "removed broken task");
    let before = fs::read_dir(dir.join("tasks")).unwrap().count();
    for args in [vec!["add", "--subject", "Blocked"], vec!["close", "1"]] {
        let refused = frump(&shards, &args);
        assert!(!refused.status.success(), "{args:?}");
        assert!(
            stderr(&refused).contains("Failed to parse task file 3.md"),
            "{}",
            stderr(&refused)
        );
    }
    assert_eq!(fs::read_dir(dir.join("tasks")).unwrap().count(), before);
    let _ = fs::remove_dir_all(single);
    let _ = fs::remove_dir_all(shards);
}

#[test]
fn a_layout_marker_that_is_not_a_file_is_an_error() {
    let root = git_repo("marker-kind");
    let board = root.join("frump");
    fs::create_dir_all(board.join("general.md")).unwrap();
    fs::write(board.join("general.md/note.txt"), "a directory, not a file").unwrap();
    fs::create_dir_all(board.join("tasks")).unwrap();
    write_task(&board, 1, "One", "done", "");
    commit_all(&root, "a directory named general.md");
    fs::remove_dir_all(board.join("general.md")).unwrap();
    fs::write(board.join("general.md"), "# Board\n\n## Team\n").unwrap();
    commit_all(&root, "fixed");
    let refused = frump(&root, &["add", "--subject", "Two"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("general.md is not a file"),
        "{}",
        stderr(&refused)
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn web_delete_refuses_a_number_that_two_tasks_hold() {
    let root = unique_root("web-duplicates");
    let board = single_board(
        &root,
        "### Task 1 - First\n\nStatus: done\n\n### Task 1 - Second\n\nStatus: done\n",
    );
    let before = fs::read(&board).unwrap();
    let (mut server, port) = start_web(&root);
    let refused = delete(port, 1);
    let _ = server.kill();
    let _ = server.wait();
    assert!(refused.0.contains("400"), "{refused:?}");
    assert!(
        refused.1.contains("2 tasks have this number"),
        "{refused:?}"
    );
    assert_eq!(fs::read(&board).unwrap(), before);
    let _ = fs::remove_dir_all(root);
}

fn head_changes(root: &Path) -> String {
    let files = Command::new("git")
        .current_dir(root)
        .args(["show", "--name-status", "--format=", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&files.stdout).into_owned()
}

#[test]
fn an_empty_directory_board_commits_and_closing_the_last_task_commits() {
    for nested in [false, true] {
        let root = git_repo(if nested {
            "empty-board-nested"
        } else {
            "empty-board-root"
        });
        let board = if nested {
            root.join("frump")
        } else {
            root.clone()
        };
        let prefix = if nested { "frump/" } else { "" };
        fs::create_dir_all(board.join("tasks")).unwrap();
        fs::write(board.join("general.md"), "# Board\n\n## Team\n").unwrap();
        let named = board.to_str().unwrap();
        ok(&frump(
            &root,
            &["--board", named, "commit", "--message", "empty board"],
        ));
        assert_eq!(head_changes(&root), format!("A\t{prefix}general.md\n"));

        write_task(&board, 1, "Only", "done", "");
        ok(&frump(
            &root,
            &["--board", named, "commit", "--message", "one task"],
        ));
        ok(&frump(&root, &["--board", named, "close", "1"]));
        ok(&frump(
            &root,
            &[
                "--board",
                named,
                "commit",
                "--message",
                "closed the last task",
            ],
        ));
        let changes = head_changes(&root);
        assert!(
            changes.contains(&format!("D\t{prefix}tasks/1.md\n")),
            "nested={nested}: {changes}"
        );

        // A metadata-only commit while tasks/ stays empty.
        fs::write(board.join("general.md"), "# Renamed board\n").unwrap();
        ok(&frump(
            &root,
            &["--board", named, "commit", "--message", "rename"],
        ));
        assert_eq!(head_changes(&root), format!("M\t{prefix}general.md\n"));
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn a_migrated_empty_board_commits() {
    let root = git_repo("empty-migrated");
    single_board(&root, "");
    commit_all(&root, "empty single-file board");
    ok(&frump(&root, &["migrate"]));
    ok(&frump(&root, &["commit", "--message", "migrate"]));
    let changes = head_changes(&root);
    assert!(
        changes.contains("D\tfrump.md\n") && changes.contains("A\tfrump/general.md\n"),
        "{changes}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_historical_task_path_that_is_not_a_file_is_an_error() {
    let root = git_repo("task-path-kind");
    let board = directory_board(&root, &[(1, "One", "done", "")]);
    fs::create_dir_all(board.join("tasks/7.md")).unwrap();
    fs::write(board.join("tasks/7.md/note.txt"), "a directory named 7.md").unwrap();
    // The live board could not be read with this directory in place; only history keeps it.
    commit_all(&root, "a directory named 7.md");
    fs::remove_dir_all(board.join("tasks/7.md")).unwrap();
    commit_all(&root, "fixed");
    let before = fs::read_dir(board.join("tasks")).unwrap().count();
    let refused = frump(&root, &["add", "--subject", "Two"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("tasks/7.md is not a file"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(fs::read_dir(board.join("tasks")).unwrap().count(), before);
    let _ = fs::remove_dir_all(root);
}

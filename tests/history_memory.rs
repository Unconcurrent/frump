//! Historical task bodies must not be retained once per board revision. Exercise the real
//! allocator under an OS memory bound, without touching a working board or notifying anyone.
#![cfg(target_os = "linux")]

use git2::{Repository, Signature};
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn allocation_preserves_complete_history_under_a_fixed_memory_budget() {
    let root = std::env::temp_dir().join(format!(
        "frump-history-memory-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    let sig = Signature::now("Test", "test@example.invalid").unwrap();
    let text = "Historical task evidence. ".repeat(2700);
    let mut parent = None;
    for revision in 0..160 {
        let mut board = String::from("# Board\n\n## Tasks\n\n");
        for id in 1..=20 {
            board.push_str(&format!(
                "### Task {id} - Task {id}\n\n{text}\n\nStatus: done\n\n"
            ));
        }
        board.push_str(&format!(
            "### Task 100 - Changing\n\nRevision {revision}\n\nStatus: todo\n"
        ));
        let blob = repo.blob(board.as_bytes()).unwrap();
        let mut builder = repo.treebuilder(None).unwrap();
        builder.insert("frump.md", blob, 0o100644).unwrap();
        let tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let parents: Vec<_> = parent.iter().collect();
        let id = repo
            .commit(Some("HEAD"), &sig, &sig, "Update task", &tree, &parents)
            .unwrap();
        parent = Some(repo.find_commit(id).unwrap());
    }
    // Delete a high-numbered task in HEAD: allocation must still remember that number.
    let board = "# Board\n\n## Tasks\n\n### Task 1 - Current\n\nStatus: todo\n";
    fs::write(root.join("frump.md"), board).unwrap();
    let blob = repo.blob(board.as_bytes()).unwrap();
    let mut builder = repo.treebuilder(None).unwrap();
    builder.insert("frump.md", blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "Close tasks",
        &tree,
        &[parent.as_ref().unwrap()],
    )
    .unwrap();
    let output = Command::new("bash")
        .args([
            "-c",
            "ulimit -v 262144; exec \"$1\" add --subject New",
            "test",
        ])
        .arg(env!("CARGO_BIN_EXE_frump"))
        .current_dir(&root)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("101"));
}

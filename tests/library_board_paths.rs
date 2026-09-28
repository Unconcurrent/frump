//! The library reads one board the same way whatever path spelling a caller uses. This test
//! changes the process's current directory, so it lives in its own test binary.

use std::{fs, process::Command};

#[test]
fn board_history_is_the_same_for_a_relative_and_an_absolute_path() {
    let root = std::env::temp_dir().join(format!("frump-library-paths-{}", std::process::id()));
    fs::create_dir_all(root.join("frump/tasks")).unwrap();
    let root = root.canonicalize().unwrap();
    fs::write(root.join("frump/general.md"), "# Board\n\n## Team\n").unwrap();
    fs::write(
        root.join("frump/tasks/9.md"),
        "### Task 9 - Nine\n\nStatus: done\n",
    )
    .unwrap();
    for args in [
        vec!["init", "-q"],
        vec![
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@example.invalid",
            "add",
            "-A",
        ],
        vec![
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "-qm",
            "board",
        ],
    ] {
        assert!(Command::new("git")
            .current_dir(&root)
            .args(&args)
            .status()
            .unwrap()
            .success());
    }
    fs::remove_file(root.join("frump/tasks/9.md")).unwrap();

    std::env::set_current_dir(&root).unwrap();
    let relative = frump::BoardHistory::load(std::path::Path::new("frump")).unwrap();
    let general = frump::BoardHistory::load(std::path::Path::new("frump/general.md")).unwrap();
    let absolute = frump::BoardHistory::load(&root.join("frump")).unwrap();
    for history in [&relative, &general, &absolute] {
        assert!(history.in_git());
        assert_eq!(history.max_id(), Some(frump::TaskId::new(9).unwrap()));
    }
    let _ = fs::remove_dir_all(root);
}

use canopus::cli;
use std::fs;
use std::process::Command;

fn git_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "canopus@example.invalid"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Canopus Test"])
        .current_dir(&root)
        .output()
        .unwrap();
    fs::write(root.join("README.md"), "# fixture\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&root)
        .output()
        .unwrap();

    root
}

#[test]
fn submit_creates_branch_patch_backend_task_and_artifacts() {
    let repo = git_repo("cli-submit");
    let state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "add test coverage".to_string(),
    ])
    .unwrap();

    assert!(repo.join("canopus-mock-output.txt").exists());
    assert!(state
        .join("artifacts")
        .join("TASK-1-plan")
        .join("plan.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("TASK-2-code")
        .join("runtime-log.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("TASK-2-code")
        .join("diff.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("TASK-2-code")
        .join("test-result.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("TASK-3-review")
        .join("review.md")
        .exists());
    assert!(state.join("tasks.json").exists());

    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&branch.stdout).trim(),
        "canopus/CANOPUS-1"
    );

    let _ = fs::remove_dir_all(repo);
}

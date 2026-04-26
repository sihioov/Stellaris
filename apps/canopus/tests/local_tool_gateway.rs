use canopus::adapters::tool_gateway::LocalToolGateway;
use canopus::ports::ToolGateway;
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
fn creates_branch_and_reports_diff_names() {
    let repo = git_repo("local-tool");
    let gateway = LocalToolGateway;

    gateway.ensure_clean_worktree(&repo).unwrap();
    gateway.create_branch(&repo, "canopus/test").unwrap();
    fs::write(repo.join("canopus-mock-output.txt"), "changed\n").unwrap();
    fs::create_dir_all(repo.join(".canopus")).unwrap();
    fs::write(repo.join(".canopus/state.json"), "{}").unwrap();
    let diff = gateway.changed_files(&repo).unwrap();

    assert_eq!(diff.status, 0);
    assert!(diff.stdout.contains("canopus-mock-output.txt"));
    assert!(!diff.stdout.contains(".canopus"));
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn rejects_disallowed_check_command() {
    let repo = git_repo("local-tool-deny");
    let gateway = LocalToolGateway;

    let err = gateway
        .run_check(&repo, &["powershell", "-Command", "Write-Output nope"])
        .unwrap_err();

    assert!(err.to_string().contains("command is not allowlisted"));
    let _ = fs::remove_dir_all(repo);
}

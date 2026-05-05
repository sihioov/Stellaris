pub mod delivery_finalize;
pub mod project_register;
pub mod status_artifacts;
pub mod work_intake;
pub mod worktree;

use crate::cli::args::{env_flag, env_non_empty};
use crate::core::{CanopusError, CanopusResult};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn require_existing_repo(repo: &Path) -> CanopusResult<()> {
    if !repo.is_dir() {
        return Err(CanopusError::InvalidInput(format!(
            "repo path does not exist: {}",
            repo.display()
        )));
    }
    let repo_arg = repo.display().to_string();
    let output = Command::new("git")
        .args(["-C", &repo_arg, "rev-parse", "--is-inside-work-tree"])
        .output()?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "true" {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("repo path is not a git worktree: {}", repo.display())
        } else {
            format!(
                "repo path is not a git worktree: {} ({stderr})",
                repo.display()
            )
        };
        return Err(CanopusError::InvalidInput(detail));
    }
    Ok(())
}

pub(super) fn git_worktree_root(repo: &Path) -> CanopusResult<PathBuf> {
    require_existing_repo(repo)?;
    let repo_arg = repo.display().to_string();
    let output = Command::new("git")
        .args(["-C", &repo_arg, "rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        return Err(CanopusError::InvalidInput(format!(
            "failed to resolve git worktree root for {}: {}",
            repo.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return Err(CanopusError::InvalidInput(format!(
            "git returned empty worktree root for {}",
            repo.display()
        )));
    }
    Ok(PathBuf::from(root))
}

pub(super) fn require_gate(name: &str) -> CanopusResult<()> {
    if env_flag(name) {
        Ok(())
    } else {
        Err(CanopusError::InvalidInput(format!(
            "{name}=1 required before GitHub-backed delivery mutation"
        )))
    }
}

pub(super) fn require_token() -> CanopusResult<()> {
    env_non_empty("GITHUB_TOKEN").map(|_| ()).ok_or_else(|| {
        CanopusError::InvalidInput("GITHUB_TOKEN required for live GitHub mutation".to_string())
    })
}

pub(super) fn print_json<T: Serialize>(value: &T) -> CanopusResult<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| CanopusError::Runtime(format!("serialize JSON: {e}")))?;
    println!("{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "canopus-repo-validation-{name}-{}",
            std::process::id()
        ));
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
    fn require_existing_repo_accepts_linked_worktrees() {
        let repo = git_repo("linked-base");
        let linked = repo.parent().unwrap().join(format!(
            "{}-linked",
            repo.file_name().unwrap().to_string_lossy()
        ));
        let _ = fs::remove_dir_all(&linked);
        let status = Command::new("git")
            .args(["worktree", "add", "-b", "linked", linked.to_str().unwrap()])
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(linked.join(".git").is_file());

        require_existing_repo(&linked).unwrap();

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", linked.to_str().unwrap()])
            .current_dir(&repo)
            .status();
        let _ = fs::remove_dir_all(&linked);
        let _ = fs::remove_dir_all(repo);
    }
}

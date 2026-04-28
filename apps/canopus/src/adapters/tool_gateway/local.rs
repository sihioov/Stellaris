use crate::core::{CanopusError, CanopusResult};
use crate::ports::{CommandOutput, ToolGateway};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct LocalToolGateway;

impl LocalToolGateway {
    fn run_command(&self, repo: &Path, command: &[&str]) -> CanopusResult<CommandOutput> {
        if command.is_empty() {
            return Err(CanopusError::Tool("command must not be empty".to_string()));
        }

        if !matches!(command[0], "git" | "cargo" | "gh") {
            return Err(CanopusError::Tool(format!(
                "command is not allowlisted: {}",
                command[0]
            )));
        }

        if let Err(err) = check_policy(repo, command) {
            notify_policy_violation(command, &err);
            return Err(err);
        }

        let output = Command::new(command[0])
            .args(&command[1..])
            .current_dir(repo)
            .output()?;

        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

fn check_policy(repo: &Path, command: &[&str]) -> CanopusResult<()> {
    match command {
        ["git", "push", args @ ..] => check_git_push_policy(repo, args),
        ["git", "reset", "--hard", ..] => deny("policy: git reset --hard denied"),
        ["git", "clean", args @ ..] if args.iter().any(|arg| is_force_clean_arg(arg)) => {
            deny("policy: git clean -f denied")
        }
        _ => Ok(()),
    }
}

fn check_git_push_policy(repo: &Path, args: &[&str]) -> CanopusResult<()> {
    if args.iter().any(|arg| {
        matches!(*arg, "--force" | "-f" | "--force-with-lease")
            || arg.starts_with("--force-with-lease=")
    }) {
        return deny("policy: force push denied");
    }

    if args.iter().any(|arg| targets_protected_branch(arg)) {
        return deny("policy: direct push to protected branch denied");
    }

    if push_may_target_current_branch(args) && current_branch_is_protected(repo) {
        return deny("policy: implicit push from protected branch denied");
    }

    Ok(())
}

fn push_may_target_current_branch(args: &[&str]) -> bool {
    let positional: Vec<&str> = args
        .iter()
        .copied()
        .filter(|arg| !arg.starts_with('-'))
        .collect();
    positional.is_empty()
        || positional.len() == 1
        || positional.iter().any(|arg| matches!(*arg, "HEAD" | "@"))
}

fn current_branch_is_protected(repo: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo)
        .output()
    else {
        return false;
    };
    let branch = String::from_utf8_lossy(&output.stdout);
    matches!(branch.trim(), "main" | "master" | "develop")
}

fn is_force_clean_arg(arg: &str) -> bool {
    arg.starts_with('-') && arg.contains('f')
}

fn targets_protected_branch(arg: &str) -> bool {
    let protected = ["main", "master", "develop"];
    protected.iter().any(|branch| {
        arg == *branch
            || arg == format!("refs/heads/{branch}")
            || arg.ends_with(&format!(":{branch}"))
            || arg.ends_with(&format!(":refs/heads/{branch}"))
    })
}

fn deny(message: &str) -> CanopusResult<()> {
    Err(CanopusError::Tool(message.to_string()))
}

fn notify_policy_violation(command: &[&str], err: &CanopusError) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({
            "content": format!("⚠️ Policy violation: `{}` — {}", command.join(" "), err)
        });
        let _ = ureq::post(&url).send_json(body);
    }
}

impl ToolGateway for LocalToolGateway {
    fn ensure_clean_worktree(&self, repo: &Path) -> CanopusResult<()> {
        let output = self.run_command(repo, &["git", "status", "--porcelain"])?;
        if output.status != 0 {
            return Err(CanopusError::Tool(output.stderr));
        }
        if !output.stdout.trim().is_empty() {
            return Err(CanopusError::Tool("worktree is not clean".to_string()));
        }
        Ok(())
    }

    fn create_branch(&self, repo: &Path, branch: &str) -> CanopusResult<CommandOutput> {
        let output = self.run_command(repo, &["git", "checkout", "-b", branch])?;
        if output.status == 0 {
            Ok(output)
        } else {
            Err(CanopusError::Tool(output.stderr))
        }
    }

    fn run_check(&self, repo: &Path, command: &[&str]) -> CanopusResult<CommandOutput> {
        self.run_command(repo, command)
    }

    fn changed_files(&self, repo: &Path) -> CanopusResult<CommandOutput> {
        let mut output = self.run_command(repo, &["git", "status", "--porcelain"])?;
        output.stdout = output
            .stdout
            .lines()
            .filter(|line| {
                let path = line.get(3..).unwrap_or("").trim();
                // git porcelain always uses forward slashes; exclude .canopus state directory
                !(path == ".canopus" || path.starts_with(".canopus/"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !output.stdout.is_empty() {
            output.stdout.push('\n');
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_rejects_dangerous_git_commands() {
        for command in [
            vec!["git", "push", "--force", "origin", "feature"],
            vec!["git", "push", "-f", "origin", "feature"],
            vec![
                "git",
                "push",
                "--force-with-lease=main",
                "origin",
                "feature",
            ],
            vec!["git", "push", "origin", "main"],
            vec!["git", "push", "origin", "HEAD:refs/heads/master"],
            vec!["git", "reset", "--hard", "HEAD~1"],
            vec!["git", "clean", "-fdx"],
        ] {
            assert!(
                check_policy(Path::new("."), &command).is_err(),
                "{command:?} should be denied"
            );
        }
    }

    #[test]
    fn policy_rejects_implicit_push_from_protected_branch() {
        let repo = std::env::temp_dir().join(format!("canopus-policy-main-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        Command::new("git")
            .arg("init")
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "-b", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();

        assert!(check_policy(&repo, &["git", "push"]).is_err());
        assert!(check_policy(&repo, &["git", "push", "origin"]).is_err());
        assert!(check_policy(&repo, &["git", "push", "origin", "HEAD"]).is_err());
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn policy_allows_non_destructive_allowlisted_commands() {
        for command in [
            vec!["git", "status", "--porcelain"],
            vec!["git", "push", "-u", "origin", "feature/test"],
            vec!["cargo", "test"],
            vec!["gh", "pr", "create", "--base", "main"],
        ] {
            assert!(
                check_policy(Path::new("."), &command).is_ok(),
                "{command:?} should be allowed"
            );
        }
    }
}

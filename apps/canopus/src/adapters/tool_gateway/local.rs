use crate::core::{CanopusError, CanopusResult};
use crate::ports::{CommandOutput, ToolGateway};
use std::path::{Path, PathBuf};
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

        if is_external_mutation(command) && !live_mutations_enabled() {
            return Ok(CommandOutput {
                status: 0,
                stdout: format!(
                    "dry-run: skipped external mutation `{}` (set CANOPUS_ENABLE_LIVE_MUTATIONS=1 to execute)\n",
                    command.join(" ")
                ),
                stderr: String::new(),
            });
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

fn live_mutations_enabled() -> bool {
    std::env::var("CANOPUS_ENABLE_LIVE_MUTATIONS").as_deref() == Ok("1")
}

fn is_external_mutation(command: &[&str]) -> bool {
    match command.first().copied() {
        Some("gh") => true,
        Some("git") => find_git_subcommand(command)
            .map(|(_, subcommand)| subcommand == "push")
            .unwrap_or(false),
        _ => false,
    }
}

fn check_policy(repo: &Path, command: &[&str]) -> CanopusResult<()> {
    if command.first() != Some(&"git") {
        return Ok(());
    }

    let Some((subcommand_index, subcommand)) = find_git_subcommand(command) else {
        return Ok(());
    };
    let global_args = &command[1..subcommand_index];
    let args = &command[subcommand_index + 1..];

    match subcommand {
        "push" => check_git_push_policy(repo, global_args, args),
        "reset" if args.contains(&"--hard") => deny("policy: git reset --hard denied"),
        "clean" if args.iter().any(|arg| is_force_clean_arg(arg)) => {
            deny("policy: git clean -f denied")
        }
        _ => Ok(()),
    }
}

/// Skip git global options to locate the actual subcommand before applying
/// policy. Without this normalization, commands such as `git -C repo push`
/// can bypass deny rules that match only `["git", "push", ..]`.
fn find_git_subcommand<'a>(command: &[&'a str]) -> Option<(usize, &'a str)> {
    let mut index = 1; // skip "git"
    while index < command.len() {
        match command[index] {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path"
            | "--config-env" => index += 2,
            arg if arg.starts_with("--git-dir=")
                || arg.starts_with("--work-tree=")
                || arg.starts_with("--namespace=")
                || arg.starts_with("--exec-path=")
                || arg.starts_with("--config-env=")
                || (arg.starts_with("-c") && arg.len() > 2) =>
            {
                index += 1;
            }
            "--no-pager"
            | "--bare"
            | "-p"
            | "--paginate"
            | "--no-replace-objects"
            | "--literal-pathspecs"
            | "--glob-pathspecs"
            | "--noglob-pathspecs"
            | "--icase-pathspecs"
            | "--no-optional-locks"
            | "--version"
            | "--help" => {
                index += 1;
            }
            _ => return Some((index, command[index])),
        }
    }
    None
}

fn check_git_push_policy(repo: &Path, global_args: &[&str], args: &[&str]) -> CanopusResult<()> {
    if args.iter().any(|arg| is_force_push_arg(arg)) {
        return deny("policy: force push denied");
    }

    if args.iter().any(|arg| targets_protected_branch(arg)) {
        return deny("policy: direct push to protected branch denied");
    }

    if push_may_target_current_branch(args) {
        let Some(policy_repo) = git_policy_repo(repo, global_args) else {
            return deny("policy: implicit push with repository override denied");
        };
        if current_branch_is_protected(&policy_repo) {
            return deny("policy: implicit push from protected branch denied");
        }
    }

    Ok(())
}

fn is_force_push_arg(arg: &str) -> bool {
    matches!(arg, "--force" | "--force-with-lease")
        || arg.starts_with("--force-with-lease=")
        || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('f'))
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

fn git_policy_repo(repo: &Path, global_args: &[&str]) -> Option<PathBuf> {
    let mut policy_repo = repo.to_path_buf();
    let mut index = 0;

    while index < global_args.len() {
        match global_args[index] {
            "-C" => {
                let dir = *global_args.get(index + 1)?;
                let dir = Path::new(dir);
                policy_repo = if dir.is_absolute() {
                    dir.to_path_buf()
                } else {
                    policy_repo.join(dir)
                };
                index += 2;
            }
            "--git-dir" | "--work-tree" => return None,
            arg if arg.starts_with("--git-dir=") || arg.starts_with("--work-tree=") => {
                return None;
            }
            "-c" | "--namespace" | "--exec-path" | "--config-env" => index += 2,
            _ => index += 1,
        }
    }

    Some(policy_repo)
}

fn is_force_clean_arg(arg: &str) -> bool {
    arg == "--force" || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('f'))
}

fn targets_protected_branch(arg: &str) -> bool {
    let protected = ["main", "master", "develop"];
    let refspec = arg.trim_start_matches('+');
    let destination = refspec.split(':').next_back().unwrap_or(refspec);
    let destination = destination.trim_start_matches("refs/heads/");
    protected.contains(&destination)
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
            vec!["git", "push", "-uf", "origin", "feature"],
            vec!["git", "push", "-f", "origin", "feature"],
            vec![
                "git",
                "push",
                "--force-with-lease=main",
                "origin",
                "feature",
            ],
            vec!["git", "push", "origin", "main"],
            vec!["git", "push", "origin", "+main"],
            vec!["git", "push", "origin", "feature:main"],
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
        assert!(check_policy(&repo, &["git", "-C", repo.to_str().unwrap(), "push"]).is_err());
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn policy_normalizes_git_global_options_before_deny_rules() {
        for command in [
            vec!["git", "-C", ".", "push", "--force", "origin", "feature"],
            vec![
                "git",
                "-c",
                "protocol.version=2",
                "push",
                "--force-with-lease=feature",
                "origin",
                "feature",
            ],
            vec!["git", "--work-tree=.", "push", "origin", "main"],
            vec!["git", "--no-pager", "reset", "--hard", "HEAD~1"],
            vec!["git", "-C", ".", "clean", "-fdx"],
        ] {
            assert!(
                check_policy(Path::new("."), &command).is_err(),
                "{command:?} should be denied"
            );
        }
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

    #[test]
    fn dry_runs_external_mutations_by_default() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::remove_var("CANOPUS_ENABLE_LIVE_MUTATIONS");
        let repo = std::env::temp_dir().join(format!("canopus-dry-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        let gateway = LocalToolGateway;

        let push = gateway
            .run_check(&repo, &["git", "push", "-u", "origin", "feature/test"])
            .unwrap();
        assert_eq!(push.status, 0);
        assert!(push.stdout.contains("dry-run: skipped external mutation"));

        let pr = gateway
            .run_check(&repo, &["gh", "pr", "create", "--title", "t"])
            .unwrap();
        assert_eq!(pr.status, 0);
        assert!(pr.stdout.contains("CANOPUS_ENABLE_LIVE_MUTATIONS=1"));

        let _ = std::fs::remove_dir_all(repo);
    }
}

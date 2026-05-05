use super::{git_worktree_root, print_json, require_existing_repo};
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::cli::args::WorktreeCreateArgs;
use crate::core::{CanopusError, CanopusResult};
use crate::ports::ToolGateway;
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Serialize)]
struct WorktreeCreateOutput {
    status: &'static str,
    name: String,
    repo_path: String,
    base_repo_path: String,
    branch: String,
}

pub(crate) fn worktree(args: &[String]) -> CanopusResult<()> {
    let Some((command, rest)) = args.split_first() else {
        return Err(CanopusError::InvalidInput(
            "worktree requires a subcommand (expected create)".to_string(),
        ));
    };

    match command.as_str() {
        "create" => create(rest),
        _ => Err(CanopusError::InvalidInput(format!(
            "unknown worktree subcommand: {command}"
        ))),
    }
}

pub(crate) fn create(args: &[String]) -> CanopusResult<()> {
    let parsed = WorktreeCreateArgs::parse(args)?;
    let output = create_worktree(&parsed, &LocalToolGateway)?;
    print_json(&output)
}

fn create_worktree(
    parsed: &WorktreeCreateArgs,
    tools: &impl ToolGateway,
) -> CanopusResult<WorktreeCreateOutput> {
    require_existing_repo(&parsed.repo)?;
    validate_worktree_name(&parsed.name)?;

    let base_root = git_worktree_root(&parsed.repo)?;
    let base_parent = base_root.parent().ok_or_else(|| {
        CanopusError::InvalidInput(format!(
            "repo has no parent directory for default worktree path: {}",
            base_root.display()
        ))
    })?;
    let target = target_path(
        &base_root,
        base_parent,
        &parsed.name,
        parsed.path.as_deref(),
    )?;
    if target.exists() {
        return Err(CanopusError::InvalidInput(format!(
            "worktree target path already exists: {}",
            target.display()
        )));
    }

    tools.create_worktree(&parsed.repo, &parsed.name, &target)?;

    Ok(WorktreeCreateOutput {
        status: "created",
        name: parsed.name.clone(),
        repo_path: target.display().to_string(),
        base_repo_path: base_root.display().to_string(),
        branch: parsed.name.clone(),
    })
}

fn validate_worktree_name(name: &str) -> CanopusResult<()> {
    let invalid = || {
        CanopusError::InvalidInput(
            "worktree name must match ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$ and must not contain '..'"
                .to_string(),
        )
    };
    if name.is_empty() || name.len() > 64 || name.contains("..") {
        return Err(invalid());
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(invalid());
    };
    if !first.is_ascii_alphanumeric() {
        return Err(invalid());
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')) {
        return Err(invalid());
    }
    Ok(())
}

fn target_path(
    base_root: &Path,
    base_parent: &Path,
    name: &str,
    requested: Option<&Path>,
) -> CanopusResult<PathBuf> {
    match requested {
        Some(path) if path.as_os_str().is_empty() => Err(CanopusError::InvalidInput(
            "--path must not be empty".to_string(),
        )),
        Some(path) => {
            let candidate = if path.is_absolute() {
                path.to_path_buf()
            } else {
                if path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                }) {
                    return Err(CanopusError::InvalidInput(
                        "relative --path must stay under the base repo parent".to_string(),
                    ));
                }
                base_parent.join(path)
            };
            if candidate.parent() != Some(base_parent) {
                return Err(CanopusError::InvalidInput(format!(
                    "worktree target path must be a direct child of base repo parent: {}",
                    base_parent.display()
                )));
            }
            Ok(candidate)
        }
        None => {
            let base_name = base_root
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    CanopusError::InvalidInput(format!(
                        "repo path has no valid final component: {}",
                        base_root.display()
                    ))
                })?;
            Ok(base_parent.join(format!("{base_name}-{name}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git_repo(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("canopus-worktree-{name}-{}", std::process::id()));
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
    fn rejects_unsafe_worktree_names() {
        for name in [
            "",
            ".hidden",
            "../bad",
            "bad/name",
            "bad\\name",
            "bad name",
            "bad$name",
            &"a".repeat(65),
        ] {
            assert!(
                validate_worktree_name(name).is_err(),
                "{name:?} should be rejected"
            );
        }
        for name in ["smoke", "smoke-1", "Smoke_1.2"] {
            assert!(
                validate_worktree_name(name).is_ok(),
                "{name:?} should be accepted"
            );
        }
    }

    #[test]
    fn rejects_requested_path_outside_base_parent() {
        let repo = git_repo("outside-path");
        let outside = std::env::temp_dir()
            .join(format!("canopus-worktree-outside-{}", std::process::id()))
            .join("smoke");
        let parsed = WorktreeCreateArgs {
            repo: repo.clone(),
            name: "smoke".to_string(),
            path: Some(outside),
        };

        let err = create_worktree(&parsed, &LocalToolGateway).unwrap_err();

        assert!(err.to_string().contains("base repo parent"));
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn creates_sibling_worktree_with_branch() {
        let repo = git_repo("create");
        let target = repo.parent().unwrap().join(format!(
            "{}-smoke",
            repo.file_name().unwrap().to_string_lossy()
        ));
        let _ = fs::remove_dir_all(&target);
        let parsed = WorktreeCreateArgs {
            repo: repo.clone(),
            name: "smoke".to_string(),
            path: None,
        };

        let output = create_worktree(&parsed, &LocalToolGateway).unwrap();

        assert_eq!(output.status, "created");
        assert_eq!(PathBuf::from(&output.repo_path), target);
        assert!(target.join(".git").is_file());
        let branch = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&target)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "smoke");
        let _ = fs::remove_dir_all(&target);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn rejects_existing_target_path() {
        let repo = git_repo("existing-target");
        let target = repo.parent().unwrap().join(format!(
            "{}-smoke",
            repo.file_name().unwrap().to_string_lossy()
        ));
        let _ = fs::remove_dir_all(&target);
        fs::create_dir_all(&target).unwrap();
        let parsed = WorktreeCreateArgs {
            repo: repo.clone(),
            name: "smoke".to_string(),
            path: None,
        };

        let err = create_worktree(&parsed, &LocalToolGateway).unwrap_err();

        assert!(err.to_string().contains("already exists"));
        let _ = fs::remove_dir_all(&target);
        let _ = fs::remove_dir_all(repo);
    }
}

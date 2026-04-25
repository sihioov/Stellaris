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

        if !matches!(command[0], "git" | "cargo") {
            return Err(CanopusError::Tool(format!(
                "command is not allowlisted: {}",
                command[0]
            )));
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

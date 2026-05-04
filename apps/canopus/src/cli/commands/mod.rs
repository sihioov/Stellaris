pub mod delivery_finalize;
pub mod project_register;
pub mod status_artifacts;
pub mod work_intake;

use crate::cli::args::{env_flag, env_non_empty};
use crate::core::{CanopusError, CanopusResult};
use serde::Serialize;
use std::path::Path;

pub(super) fn require_existing_repo(repo: &Path) -> CanopusResult<()> {
    if !repo.is_dir() {
        return Err(CanopusError::InvalidInput(format!(
            "repo path does not exist: {}",
            repo.display()
        )));
    }
    if !repo.join(".git").is_dir() {
        return Err(CanopusError::InvalidInput(format!(
            "repo path is not a git repository: {}",
            repo.display()
        )));
    }
    Ok(())
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

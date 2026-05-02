use crate::core::{CanopusError, CanopusResult};
use chrono::Utc;

pub fn derive_run_identity(
    agenda_id: Option<&str>,
    task_id: Option<&str>,
) -> CanopusResult<String> {
    let raw = match (
        agenda_id.filter(|v| !v.trim().is_empty()),
        task_id.filter(|v| !v.trim().is_empty()),
    ) {
        (Some(agenda), Some(task)) => format!("{agenda}-{task}"),
        (Some(agenda), None) => agenda.to_string(),
        (None, Some(task)) => task.to_string(),
        (None, None) => format!("canopus-{}", Utc::now().format("%Y%m%d%H%M%S%3f")),
    };

    sanitize_run_identity(&raw)
}

pub fn sanitize_run_identity(raw: &str) -> CanopusResult<String> {
    let mut id = String::new();
    let mut last_was_dash = false;

    for ch in raw.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '-' | '_' | '.' | '/' | ':' | ' ') {
            Some('-')
        } else {
            None
        };

        if let Some(ch) = normalized {
            if ch == '-' {
                if !last_was_dash && !id.is_empty() {
                    id.push(ch);
                    last_was_dash = true;
                }
            } else {
                id.push(ch);
                last_was_dash = false;
            }
        }
    }

    while id.ends_with('-') {
        id.pop();
    }

    if id.is_empty() {
        return Err(CanopusError::InvalidInput(
            "run identity must contain at least one ASCII letter or digit".to_string(),
        ));
    }

    Ok(id)
}

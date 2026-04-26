use dysonsphere::{
    error::{Result, StellarisError},
    message::TaskMessage,
};
use std::process::Command;

pub async fn handle(task: &TaskMessage, label: &str) -> Result<()> {
    if label == "canopus.agent" {
        run_canopus(task).await
    } else {
        log::info!("[Custom:{}] task_id={}", label, task.task_id);
        Ok(())
    }
}

async fn run_canopus(task: &TaskMessage) -> Result<()> {
    let repo = std::env::var("CANOPUS_REPO_PATH").unwrap_or_else(|_| ".".into());
    let state = std::env::var("CANOPUS_STATE_PATH").unwrap_or_else(|_| ".canopus".into());

    notify_discord(&format!("🚀 **작업 시작**: {}", task.payload));
    log::info!(
        "[Canopus] Starting for task {}: {}",
        task.task_id,
        task.payload
    );

    // `--` prevents payload from being parsed as flags (argument injection guard)
    let output = Command::new("canopus")
        .args([
            "submit",
            "--repo",
            &repo,
            "--state",
            &state,
            "--",
            &task.payload,
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            log::info!("[Canopus] Output: {}", stdout);
            notify_discord(&format!(
                "✅ **작업 완료 — 검토 대기 중** `task_id={}`\n`!approve {}` 또는 `!reject {}`로 응답해주세요.",
                task.task_id, task.task_id, task.task_id
            ));
            Ok(())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            log::error!("[Canopus] Exited with {}: {}", out.status, stderr);
            notify_discord(&format!(
                "❌ **작업 실패** (exit {}): {}",
                out.status,
                stderr.chars().take(200).collect::<String>()
            ));
            Err(StellarisError::IoError(format!(
                "canopus exited with {}",
                out.status
            )))
        }
        Err(e) => {
            log::error!("[Canopus] Failed to launch: {e}");
            notify_discord(&format!("❌ **canopus 실행 실패**: {}", e));
            Err(StellarisError::IoError(e.to_string()))
        }
    }
}

fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({"content": message});
        if let Err(e) = ureq::post(&url).send_json(body) {
            log::warn!("[Canopus] Discord notify failed: {e}");
        }
    }
}

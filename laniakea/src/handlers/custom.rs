use dysonsphere::{error::Result, message::TaskMessage};
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

    log::info!("[Canopus] Starting for task {}: {}", task.task_id, task.payload);

    let output = Command::new("canopus")
        .args(["submit", "--repo", &repo, "--state", &state, &task.payload])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            log::info!("[Canopus] Output: {}", stdout);
            notify_discord(&format!(
                "✅ **작업 완료!** task_id={}\n\n`!approve` 또는 `!reject`로 응답해주세요.",
                task.task_id
            ));
        }
        Err(e) => {
            log::error!("[Canopus] Failed to run: {e}");
            notify_discord(&format!("❌ **작업 실패**: {}", e));
            return Err(dysonsphere::error::StellarisError::DefaultError);
        }
    }

    Ok(())
}

fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = format!("{{\"content\": \"{}\"}}", message.replace('"', "\\\""));
        let _ = std::process::Command::new("curl")
            .args(["-s", "-X", "POST", &url,
                   "-H", "Content-Type: application/json",
                   "-d", &body])
            .output();
    }
}

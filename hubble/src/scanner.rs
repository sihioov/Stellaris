use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use tokio::process::Command;

#[derive(Debug, Clone, Hash)]
pub enum DiscoveryKind {
    Bug,
    Security,
    TestCoverage,
    UXImprovement,
}

#[derive(Debug, Clone)]
pub struct Discovery {
    pub kind: DiscoveryKind,
    pub title: String,
    pub description: String,
}

impl Discovery {
    pub fn to_task_message(&self, id: &str) -> TaskMessage {
        let task_type = match self.kind {
            DiscoveryKind::Bug => TaskType::Bug,
            DiscoveryKind::Security => TaskType::Security,
            DiscoveryKind::TestCoverage => TaskType::TestCoverage,
            DiscoveryKind::UXImprovement => TaskType::UXImprovement,
        };
        TaskMessage {
            task_id: id.to_string(),
            task_type,
            payload: format!("{}: {}", self.title, self.description),
            meta: TaskMeta::default(),
        }
    }
}

/// 코드베이스 스캔 — 현재는 cargo clippy 출력 기반 stub.
/// 실제 AI 연동 시 이 함수 내부에서 Claude API 호출.
pub async fn scan(repo_path: &std::path::Path) -> Vec<Discovery> {
    let mut findings = Vec::new();

    let output = Command::new("cargo")
        .args(["clippy", "--message-format=short", "--", "-W", "clippy::all"])
        .current_dir(repo_path)
        .output()
        .await;

    match output {
        Err(e) => {
            log::error!("[hubble] cargo clippy 실행 실패: {e}");
        }
        Ok(out) => {
            if !out.status.success() {
                log::warn!("[hubble] cargo clippy 비정상 종료: {:?}", out.status);
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            for line in stderr.lines() {
                if line.contains("warning:") || line.contains("error:") {
                    findings.push(Discovery {
                        kind: DiscoveryKind::Bug,
                        title: "Clippy 경고".to_string(),
                        description: line.to_string(),
                    });
                }
            }
        }
    }

    findings
}

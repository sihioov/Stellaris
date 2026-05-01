use dysonsphere::discovery::{discovery_id_fnv1a, Discovery as DiscoveryRecord};
use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use dysonsphere::status::TaskStatus;
use tokio::process::Command;

#[derive(Debug, Clone, Hash)]
#[allow(dead_code)] // v1 scanner currently emits Bug findings; other Canopus lanes are reserved.
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

impl DiscoveryRecord for Discovery {
    fn id(&self) -> String {
        discovery_id_fnv1a(
            "kepler",
            &[
                format!("{:?}", self.kind).as_bytes(),
                self.title.as_bytes(),
                self.description.as_bytes(),
            ],
        )
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn to_task_message(&self, id: &str) -> TaskMessage {
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
            meta: TaskMeta {
                status: TaskStatus::PendingProposal,
                ..TaskMeta::default()
            },
        }
    }
}

/// 코드베이스 스캔 — 현재는 cargo clippy 출력 기반 stub.
/// 실제 AI 연동 시 이 함수 내부에서 코드 분석 에이전트 호출.
pub async fn scan(repo_path: &std::path::Path) -> Vec<Discovery> {
    let mut findings = Vec::new();

    let output = Command::new("cargo")
        .args([
            "clippy",
            "--message-format=short",
            "--",
            "-W",
            "clippy::all",
        ])
        .current_dir(repo_path)
        .output()
        .await;

    match output {
        Err(err) => {
            log::error!("[kepler] cargo clippy 실행 실패: {err}");
        }
        Ok(out) => {
            if !out.status.success() {
                log::warn!("[kepler] cargo clippy 비정상 종료: {:?}", out.status);
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

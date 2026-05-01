use async_trait::async_trait;
use dysonsphere::discovery::{discovery_id_fnv1a, Discovery};
use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use dysonsphere::status::TaskStatus;

/// External data source hook for future web/SNS/RSS collectors.
///
/// Hubble intentionally does not scan source code. Codebase discovery lives in
/// the `kepler` crate; Hubble is reserved for external observations that can be
/// converted into human-approved `PendingProposal` tasks.
#[async_trait]
pub trait ExternalSource: Send + Sync {
    async fn collect(&self) -> Vec<ExternalDiscovery>;
}

#[derive(Debug, Clone)]
pub struct ExternalDiscovery {
    pub source: String,
    pub title: String,
    pub summary: String,
    pub url: Option<String>,
}

impl Discovery for ExternalDiscovery {
    fn id(&self) -> String {
        let url = self.url.as_deref().unwrap_or("");
        discovery_id_fnv1a(
            "hubble",
            &[
                self.source.as_bytes(),
                self.title.as_bytes(),
                self.summary.as_bytes(),
                url.as_bytes(),
            ],
        )
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn to_task_message(&self, id: &str) -> TaskMessage {
        let payload = match &self.url {
            Some(url) => format!(
                "source={}\ntitle={}\nsummary={}\nurl={}",
                self.source, self.title, self.summary, url
            ),
            None => format!(
                "source={}\ntitle={}\nsummary={}",
                self.source, self.title, self.summary
            ),
        };
        TaskMessage {
            task_id: id.to_string(),
            task_type: TaskType::NewsA,
            payload,
            meta: TaskMeta {
                status: TaskStatus::PendingProposal,
                ..TaskMeta::default()
            },
        }
    }
}

fn main() {
    dotenvy::dotenv().ok();
    env_logger::init();
    log::info!("[hubble] no external sources configured; collector stub is idle");
    println!("hubble: no external sources configured");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_discovery_converts_to_pending_proposal_task() {
        let discovery = ExternalDiscovery {
            source: "rss".to_string(),
            title: "Example story".to_string(),
            summary: "A notable external signal".to_string(),
            url: Some("https://example.invalid/story".to_string()),
        };

        let id = discovery.id();
        assert!(id.starts_with("hubble-"));
        let task = discovery.to_task_message(&id);

        assert_eq!(task.task_id, id);
        assert_eq!(task.task_type, TaskType::NewsA);
        assert_eq!(task.meta.status, TaskStatus::PendingProposal);
        assert!(task.payload.contains("source=rss"));
        assert!(task.payload.contains("url=https://example.invalid/story"));
    }
}

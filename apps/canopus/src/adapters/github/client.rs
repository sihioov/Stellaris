use crate::core::{CanopusError, CanopusResult};
use serde::Deserialize;

pub struct GitHubClient {
    token: String,
    owner: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
pub struct IssueComment {
    pub id: u64,
    pub body: String,
    pub user: CommentUser,
}

#[derive(Debug, Deserialize)]
pub struct CommentUser {
    pub login: String,
}

impl GitHubClient {
    pub fn new(token: &str, owner: &str, repo: &str) -> Self {
        Self {
            token: token.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        }
    }

    /// 환경변수에서 GitHubClient 생성. GITHUB_TOKEN, GITHUB_OWNER, GITHUB_REPO 필요.
    /// 환경변수 없으면 None 반환.
    pub fn from_env() -> Option<Self> {
        let token = std::env::var("GITHUB_TOKEN").ok()?;
        let owner = std::env::var("GITHUB_OWNER").ok()?;
        let repo = std::env::var("GITHUB_REPO").ok()?;
        Some(Self::new(&token, &owner, &repo))
    }

    fn base_url(&self) -> String {
        format!("https://api.github.com/repos/{}/{}", self.owner, self.repo)
    }

    fn agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new().user_agent("canopus/0.1").build()
    }

    /// GitHub Issue 생성. 성공 시 issue_number 반환.
    pub fn create_issue(&self, title: &str, body: &str) -> CanopusResult<u64> {
        let url = format!("{}/issues", self.base_url());
        let payload = serde_json::json!({ "title": title, "body": body });
        let resp: serde_json::Value = self
            .agent()
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .send_json(payload)
            .map_err(|e| CanopusError::Tool(e.to_string()))?
            .into_json()
            .map_err(|e| CanopusError::Tool(e.to_string()))?;
        resp["number"]
            .as_u64()
            .ok_or_else(|| CanopusError::Tool("missing issue number in response".into()))
    }

    /// Issue의 모든 comment 조회.
    pub fn get_issue_comments(&self, issue_number: u64) -> CanopusResult<Vec<IssueComment>> {
        let url = format!("{}/issues/{}/comments", self.base_url(), issue_number);
        let resp: Vec<IssueComment> = self
            .agent()
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|e| CanopusError::Tool(e.to_string()))?
            .into_json()
            .map_err(|e| CanopusError::Tool(e.to_string()))?;
        Ok(resp)
    }

    /// Issue close.
    pub fn close_issue(&self, issue_number: u64) -> CanopusResult<()> {
        let url = format!("{}/issues/{}", self.base_url(), issue_number);
        let payload = serde_json::json!({ "state": "closed" });
        self.agent()
            .patch(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .send_json(payload)
            .map_err(|e| CanopusError::Tool(e.to_string()))?;
        Ok(())
    }
}

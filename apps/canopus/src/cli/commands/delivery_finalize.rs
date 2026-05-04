use super::{print_json, require_existing_repo};
use crate::cli::args::{env_flag, env_non_empty, DeliveryFinalizeArgs};
use crate::core::{CanopusError, CanopusResult};
use serde::Serialize;

pub(crate) fn delivery_finalize(args: &[String]) -> CanopusResult<()> {
    let parsed = DeliveryFinalizeArgs::parse(args)?;
    require_existing_repo(&parsed.repo)?;
    let report = DeliveryGateReport::from_env(
        parsed.discord_approved,
        parsed.github_ready,
        parsed.merge_succeeded,
    );
    if !report.can_create_pr() {
        return Err(CanopusError::InvalidInput(report.denial_reason()));
    }
    if parsed.merge_requested && !report.can_merge() {
        return Err(CanopusError::InvalidInput(report.denial_reason()));
    }
    if parsed.deploy_required && !report.can_deploy() {
        return Err(CanopusError::InvalidInput(report.denial_reason()));
    }
    print_json(&report)
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DeliveryGateReport {
    discord_approved: bool,
    github_ready: bool,
    pr_gate_enabled: bool,
    merge_gate_enabled: bool,
    deploy_gate_enabled: bool,
    deploy_configured: bool,
    merge_succeeded: bool,
    status: String,
    denial_reasons: Vec<String>,
}

impl DeliveryGateReport {
    pub(crate) fn from_env(
        discord_approved: bool,
        github_ready: bool,
        merge_succeeded: bool,
    ) -> Self {
        let pr_gate_enabled = env_flag("CANOPUS_ALLOW_GITHUB_PR_MUTATION");
        let merge_gate_enabled = env_flag("CANOPUS_ALLOW_GITHUB_MERGE");
        let deploy_gate_enabled = env_flag("CANOPUS_ALLOW_DEPLOY");
        let deploy_configured = env_non_empty("CANOPUS_DEPLOY_ADAPTER").is_some()
            && env_non_empty("CANOPUS_DEPLOY_ENVIRONMENT").is_some()
            && env_non_empty("CANOPUS_DEPLOY_COMMAND").is_some();
        let mut report = Self {
            discord_approved,
            github_ready,
            pr_gate_enabled,
            merge_gate_enabled,
            deploy_gate_enabled,
            deploy_configured,
            merge_succeeded,
            status: "Ready".to_string(),
            denial_reasons: Vec::new(),
        };
        if !pr_gate_enabled {
            report
                .denial_reasons
                .push("CANOPUS_ALLOW_GITHUB_PR_MUTATION=1 required".to_string());
        }
        if !discord_approved {
            report
                .denial_reasons
                .push("Discord approval missing".to_string());
        }
        if !github_ready {
            report
                .denial_reasons
                .push("GitHub checks/reviews/branch protection not ready".to_string());
        }
        if !merge_gate_enabled {
            report
                .denial_reasons
                .push("CANOPUS_ALLOW_GITHUB_MERGE=1 required".to_string());
        }
        if !deploy_gate_enabled {
            report
                .denial_reasons
                .push("CANOPUS_ALLOW_DEPLOY=1 required".to_string());
        }
        if !deploy_configured {
            report
                .denial_reasons
                .push("explicit deploy adapter/environment/command required".to_string());
        }
        if !merge_succeeded {
            report
                .denial_reasons
                .push("merge must succeed before deploy".to_string());
        }
        report.status = if report.denial_reasons.is_empty() {
            "Allowed"
        } else {
            "Denied"
        }
        .to_string();
        report
    }

    pub(crate) fn can_create_pr(&self) -> bool {
        self.pr_gate_enabled
    }
    pub(crate) fn can_merge(&self) -> bool {
        self.pr_gate_enabled
            && self.discord_approved
            && self.github_ready
            && self.merge_gate_enabled
    }
    pub(crate) fn can_deploy(&self) -> bool {
        self.can_merge()
            && self.merge_succeeded
            && self.deploy_gate_enabled
            && self.deploy_configured
    }
    pub(crate) fn denial_reason(&self) -> String {
        self.denial_reasons.join("; ")
    }
}

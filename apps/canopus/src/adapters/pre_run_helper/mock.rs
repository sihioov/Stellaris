use crate::core::{CanopusResult, HelperOutput, HelperRequest, HelperSelection};
use crate::ports::PreRunHelperBackend;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct MockPreRunHelperBackend;

impl PreRunHelperBackend for MockPreRunHelperBackend {
    fn identity(&self) -> String {
        "mock-pre-run-helper".to_string()
    }

    fn run(
        &self,
        _repo: &Path,
        request: &HelperRequest,
        selection: &HelperSelection,
    ) -> CanopusResult<HelperOutput> {
        Ok(HelperOutput {
            summary: format!(
                "mock helper `{}` prepared context for {}",
                selection.name,
                request.role.as_str()
            ),
            content: format!(
                "Mock pre-run helper context\nrole={}\nstage={}\nprior_artifact_count={}\n",
                request.role.as_str(),
                request.stage_name,
                request.prior_artifact_count
            ),
            truncated: false,
            read_only_check: "passed: mock backend does not access repository files".to_string(),
        })
    }
}

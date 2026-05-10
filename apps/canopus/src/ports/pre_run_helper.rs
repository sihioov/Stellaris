use crate::core::{CanopusResult, HelperOutput, HelperRequest, HelperSelection};
use std::path::Path;

/// Read-only helper backend invoked by Canopus before a role runtime.
///
/// Implementations must not mutate repository files, git state, external
/// services, approvals, PRs, or deployment surfaces. Command-backed adapters
/// must enforce this with allowlisted argv and mutation checks; do not reuse
/// mutating [`ToolGateway`](crate::ports::ToolGateway) operations here.
pub trait PreRunHelperBackend {
    fn identity(&self) -> String;
    fn run(
        &self,
        repo: &Path,
        request: &HelperRequest,
        selection: &HelperSelection,
    ) -> CanopusResult<HelperOutput>;
}

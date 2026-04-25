use crate::core::{Artifact, CanopusResult};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLocation {
    pub path: PathBuf,
}

pub trait ArtifactStore {
    fn save(&self, artifact: &Artifact) -> CanopusResult<ArtifactLocation>;
}

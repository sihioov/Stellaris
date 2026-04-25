use crate::core::{Artifact, CanopusResult};
use crate::ports::{ArtifactLocation, ArtifactStore};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LocalFileArtifactStore {
    root: PathBuf,
}

impl LocalFileArtifactStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl ArtifactStore for LocalFileArtifactStore {
    fn save(&self, artifact: &Artifact) -> CanopusResult<ArtifactLocation> {
        let task_dir = self.root.join(&artifact.task_id);
        fs::create_dir_all(&task_dir)?;
        let path = task_dir.join(artifact.kind.file_name());
        fs::write(&path, &artifact.content)?;
        Ok(ArtifactLocation { path })
    }
}

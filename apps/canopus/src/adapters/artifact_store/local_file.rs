use crate::core::{Artifact, CanopusError, CanopusResult};
use crate::ports::{ArtifactLocation, ArtifactStore};
use std::fs;
use std::path::{Component, PathBuf};

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
        let id_path = std::path::Path::new(&artifact.task_id);
        if id_path.components().count() != 1
            || id_path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(CanopusError::InvalidInput(format!(
                "task_id must be a single path component, got: {}",
                artifact.task_id
            )));
        }
        let task_dir = self.root.join(&artifact.task_id);
        fs::create_dir_all(&task_dir)?;
        let path = task_dir.join(artifact.kind.file_name());
        fs::write(&path, &artifact.content)?;
        Ok(ArtifactLocation { path })
    }
}

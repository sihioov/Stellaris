use canopus::adapters::artifact_store::LocalFileArtifactStore;
use canopus::core::{Artifact, ArtifactKind};
use canopus::ports::ArtifactStore;
use std::fs;

fn test_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}

#[test]
fn saves_artifact_under_task_directory() {
    let root = test_root("artifact-store");
    let store = LocalFileArtifactStore::new(root.clone());
    let artifact = Artifact {
        task_id: "CANOPUS-1".to_string(),
        kind: ArtifactKind::Plan,
        content: "# Plan\n\nRun checks.\n".to_string(),
    };

    let location = store.save(&artifact).unwrap();

    assert_eq!(location.path, root.join("CANOPUS-1").join("plan.md"));
    assert_eq!(fs::read_to_string(location.path).unwrap(), artifact.content);
    let _ = fs::remove_dir_all(root);
}

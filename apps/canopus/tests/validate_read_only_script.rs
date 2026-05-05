#[test]
fn validate_read_only_helper_uses_submit_and_preserves_mutation_gates() {
    let script = include_str!("../../../scripts/validate-read-only.ps1");

    assert!(script.contains("\"submit\""));
    assert!(!script.contains("\"project-register\""));
    assert!(!script.contains("\"work-intake\""));
    assert!(script.contains("CANOPUS_ENABLE_GITHUB = \"1\""));
    assert!(script.contains("CANOPUS_GITHUB_PROJECT_MODE = \"validate-read-only\""));
    assert!(script.contains("CANOPUS_ENABLE_LIVE_MUTATIONS = \"0\""));
    assert!(script.contains("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION = \"0\""));
    assert!(script.contains("Restore-ValidateReadOnlyEnv"));
    assert!(script.contains("$global:LASTEXITCODE = 0"));
    assert!(script.contains("Skipping validate-read-only probe"));
}

use canopus::core::{
    deterministic_agenda_id_for_github_issue, deterministic_agenda_id_for_github_project, Agenda,
    AgendaSource,
};

#[test]
fn new_with_id_defaults_source_to_cli() {
    let agenda = Agenda::new_with_id("CANOPUS-1", "ship the loop").unwrap();
    assert_eq!(agenda.id, "CANOPUS-1");
    assert_eq!(agenda.request, "ship the loop");
    assert_eq!(agenda.source, AgendaSource::Cli);
    assert_eq!(agenda.source.kind(), "cli");
}

#[test]
fn new_with_source_records_typed_variant() {
    let agenda = Agenda::new_with_source(
        "explicit-id",
        "request",
        AgendaSource::GitHubIssue {
            owner: "Acme".to_string(),
            repo: "Demo".to_string(),
            number: 7,
        },
    )
    .unwrap();
    assert_eq!(agenda.source.kind(), "github_issue");
    match agenda.source {
        AgendaSource::GitHubIssue {
            owner,
            repo,
            number,
        } => {
            assert_eq!(owner, "Acme");
            assert_eq!(repo, "Demo");
            assert_eq!(number, 7);
        }
        other => panic!("expected GitHubIssue source, got {other:?}"),
    }
}

#[test]
fn from_github_issue_is_deterministic_per_identity() {
    let a = Agenda::from_github_issue("Acme", "Demo", 42, "first request").unwrap();
    let b = Agenda::from_github_issue("Acme", "Demo", 42, "different request body").unwrap();
    assert_eq!(
        a.id, b.id,
        "same (owner, repo, number) must produce the same agenda id regardless of request body"
    );
    assert_eq!(a.id, "gh-acme-demo-42");
    assert_eq!(a.source.kind(), "github_issue");
    assert_eq!(b.source, a.source);
}

#[test]
fn from_github_issue_changes_id_when_identity_changes() {
    let owner_change = Agenda::from_github_issue("acme", "demo", 1, "r").unwrap();
    let repo_change = Agenda::from_github_issue("acme", "other", 1, "r").unwrap();
    let number_change = Agenda::from_github_issue("acme", "demo", 2, "r").unwrap();
    assert_ne!(owner_change.id, repo_change.id);
    assert_ne!(owner_change.id, number_change.id);
    assert_ne!(repo_change.id, number_change.id);
}

#[test]
fn cli_source_id_differs_from_github_source_id_for_same_input() {
    let cli = Agenda::new_with_id("acme-demo-42", "r").unwrap();
    let gh = Agenda::from_github_issue("acme", "demo", 42, "r").unwrap();
    assert_eq!(gh.id, "gh-acme-demo-42");
    assert_ne!(cli.id, gh.id);
    assert_ne!(cli.source, gh.source);
}

#[test]
fn deterministic_helper_sanitises_like_run_identity() {
    let id = deterministic_agenda_id_for_github_issue("Acme/Org", "Demo Repo", 9001).unwrap();
    // dashes/spaces/slashes collapse to single dashes; lowercase; trailing dash trimmed
    assert_eq!(id, "gh-acme-org-demo-repo-9001");
}

#[test]
fn from_github_project_is_deterministic_per_item() {
    let a = Agenda::from_github_project(
        "https://github.com/orgs/acme/projects/1",
        "PVTI_lAHO",
        "request",
    )
    .unwrap();
    let b = Agenda::from_github_project(
        "https://github.com/orgs/acme/projects/1",
        "PVTI_lAHO",
        "different body",
    )
    .unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(a.source.kind(), "github_project");
    match &a.source {
        AgendaSource::GitHubProject {
            project_url,
            item_id,
        } => {
            assert_eq!(project_url, "https://github.com/orgs/acme/projects/1");
            assert_eq!(item_id, "PVTI_lAHO");
        }
        other => panic!("expected GitHubProject source, got {other:?}"),
    }
}

#[test]
fn deterministic_project_id_uses_ghp_prefix() {
    let id = deterministic_agenda_id_for_github_project(
        "https://github.com/orgs/acme/projects/1",
        "PVTI_lAHO",
    )
    .unwrap();
    assert!(
        id.starts_with("ghp-"),
        "project ids must be distinguishable from issue ids; got {id}"
    );
}

#[test]
fn empty_request_is_rejected_for_every_constructor() {
    assert!(Agenda::new_with_id("id", "   ").is_err());
    assert!(Agenda::new_with_source("id", "", AgendaSource::Cli).is_err());
    assert!(Agenda::from_github_issue("a", "b", 1, " \t ").is_err());
    assert!(Agenda::from_github_project("u", "i", "").is_err());
}

use canopus::adapters::agent_runtime::CodexAgentRuntime;
use canopus::core::{Agenda, AgentRole, AgentTask, Artifact, ArtifactKind};
use canopus::ports::{AgentContext, AgentRuntime};
use std::fs;

fn test_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn fake_codex(repo: &std::path::Path, fail: bool) -> std::path::PathBuf {
    let script = repo.join(if fail {
        "fake-codex-fail.py"
    } else {
        "fake-codex.py"
    });
    let failure = if fail { "sys.exit(7)" } else { "sys.exit(0)" };
    fs::write(
        &script,
        format!(
            r#"import json
import os
import pathlib
import sys

args = sys.argv[1:]
repo = "."
last_message = None
for index, arg in enumerate(args):
    if arg == "--cd":
        repo = args[index + 1]
    if arg == "--output-last-message":
        last_message = args[index + 1]

prompt = sys.stdin.read()
pathlib.Path(repo, "codex-args.json").write_text(json.dumps(args), encoding="utf-8")
pathlib.Path(repo, "codex-prompt.txt").write_text(prompt, encoding="utf-8")
if last_message:
    pathlib.Path(last_message).write_text("fake final for " + os.environ.get("CANOPUS_ROLE", "unknown"), encoding="utf-8")
if os.environ.get("CANOPUS_ROLE") == "coder":
    pathlib.Path(repo, "codex-real-output.txt").write_text(prompt, encoding="utf-8")
print("fake stdout")
print("fake stderr", file=sys.stderr)
{failure}
"#
        ),
    )
    .unwrap();
    script
}

#[tokio::test]
async fn codex_runtime_invokes_codex_exec_and_captures_message_log() {
    let repo = test_repo("codex-runtime");
    let script = fake_codex(&repo, false);
    let agenda = Agenda::new_with_id("CANOPUS-1", "replace mock runtime").unwrap();
    let task = AgentTask::for_agenda("TASK-CODEX", &agenda, AgentRole::Planner);
    let runtime =
        CodexAgentRuntime::new(vec!["python3".to_string(), script.display().to_string()]).unwrap();
    let prior_artifacts = vec![Artifact {
        task_id: "TASK-PRIOR".to_string(),
        kind: ArtifactKind::RuntimeLog,
        content: "prior runtime evidence".to_string(),
    }];

    let result = runtime
        .run(
            &task,
            &AgentContext {
                repo_path: repo.clone(),
            },
            &prior_artifacts,
        )
        .await
        .unwrap();

    assert_eq!(result.task_id, "TASK-CODEX");
    assert_eq!(result.summary, "codex planner completed");
    assert_eq!(result.artifacts[0].kind, ArtifactKind::Plan);
    assert_eq!(result.artifacts[0].content, "fake final for planner");
    assert_eq!(result.message_log.len(), 2);
    assert!(result.message_log[0]
        .content
        .contains("replace mock runtime"));
    assert!(result.message_log[0]
        .content
        .contains("prior runtime evidence"));
    assert_eq!(result.message_log[1].content, "fake final for planner");

    let args: Vec<String> =
        serde_json::from_str(&fs::read_to_string(repo.join("codex-args.json")).unwrap()).unwrap();
    assert!(args
        .windows(3)
        .any(|triple| triple == ["--ask-for-approval", "never", "exec"]));
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--sandbox", "read-only"]));
    assert!(args
        .windows(2)
        .any(|pair| pair[0] == "--output-last-message"));
    assert_eq!(args.last().unwrap(), "-");

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn codex_prompt_labels_helper_context_separately_from_prior_artifacts() {
    let repo = test_repo("codex-runtime-helper-context");
    let script = fake_codex(&repo, false);
    let agenda = Agenda::new_with_id("CANOPUS-1", "review helper context").unwrap();
    let task = AgentTask::for_agenda("TASK-HELPER-CONTEXT", &agenda, AgentRole::Reviewer);
    let runtime =
        CodexAgentRuntime::new(vec!["python3".to_string(), script.display().to_string()]).unwrap();
    let prior_artifacts = vec![
        Artifact {
            task_id: "TASK-HELPER".to_string(),
            kind: ArtifactKind::HelperProvenance,
            content: "helper: repo-explore\nstatus: ok\nHelper output".to_string(),
        },
        Artifact {
            task_id: "TASK-PLAN".to_string(),
            kind: ArtifactKind::Plan,
            content: "ordinary plan".to_string(),
        },
    ];

    let result = runtime
        .run(
            &task,
            &AgentContext {
                repo_path: repo.clone(),
            },
            &prior_artifacts,
        )
        .await
        .unwrap();

    let prompt = &result.message_log[0].content;
    assert!(prompt.contains("Pre-run helper context (Canopus-selected, read-only):"));
    assert!(prompt.contains("helper: repo-explore"));
    assert!(prompt.contains("Prior artifacts:"));
    assert!(prompt.contains("ordinary plan"));
    assert!(prompt.find("Pre-run helper context") < prompt.find("Prior artifacts"));

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn coder_role_can_create_repo_changes_through_real_runtime_path() {
    let repo = test_repo("codex-runtime-coder");
    let script = fake_codex(&repo, false);
    let agenda = Agenda::new_with_id("CANOPUS-1", "write a repo file").unwrap();
    let task = AgentTask::for_agenda("TASK-CODE", &agenda, AgentRole::Coder);
    let runtime =
        CodexAgentRuntime::new(vec!["python3".to_string(), script.display().to_string()]).unwrap();

    let result = runtime
        .run(
            &task,
            &AgentContext {
                repo_path: repo.clone(),
            },
            &[],
        )
        .await
        .unwrap();

    assert!(repo.join("codex-real-output.txt").exists());
    let args: Vec<String> =
        serde_json::from_str(&fs::read_to_string(repo.join("codex-args.json")).unwrap()).unwrap();
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--sandbox", "workspace-write"]));
    assert_eq!(result.summary, "codex coder completed");
    assert_eq!(result.artifacts[0].kind, ArtifactKind::RuntimeLog);
    assert_eq!(result.artifacts[0].content, "fake final for coder");

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn codex_runtime_reports_failed_exec_as_runtime_error() {
    let repo = test_repo("codex-runtime-fail");
    let script = fake_codex(&repo, true);
    let agenda = Agenda::new_with_id("CANOPUS-1", "fail runtime").unwrap();
    let task = AgentTask::for_agenda("TASK-FAIL", &agenda, AgentRole::Reviewer);
    let runtime =
        CodexAgentRuntime::new(vec!["python3".to_string(), script.display().to_string()]).unwrap();

    let err = runtime
        .run(
            &task,
            &AgentContext {
                repo_path: repo.clone(),
            },
            &[],
        )
        .await
        .unwrap_err();

    let message = err.to_string();
    assert!(message.contains("# Codex runtime"));
    assert!(message.contains("status: 7"));
    assert!(message.contains("fake stderr"));

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn analyst_role_runs_read_only_to_preserve_clean_worktree_for_later_stages() {
    let repo = test_repo("codex-runtime-analyst");
    let script = fake_codex(&repo, false);
    let agenda = Agenda::new_with_id("CANOPUS-1", "analyze before planning").unwrap();
    let task = AgentTask::for_agenda(
        "TASK-ANALYST",
        &agenda,
        AgentRole::Custom("analyst".to_string()),
    );
    let runtime =
        CodexAgentRuntime::new(vec!["python3".to_string(), script.display().to_string()]).unwrap();

    let result = runtime
        .run(
            &task,
            &AgentContext {
                repo_path: repo.clone(),
            },
            &[],
        )
        .await
        .unwrap();

    let args: Vec<String> =
        serde_json::from_str(&fs::read_to_string(repo.join("codex-args.json")).unwrap()).unwrap();
    assert!(args
        .windows(2)
        .any(|pair| pair == ["--sandbox", "read-only"]));
    assert!(result.message_log[0].content.contains("Do not edit files"));

    let _ = fs::remove_dir_all(repo);
}

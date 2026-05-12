#![allow(clippy::await_holding_lock)]

use canopus::cli;
use std::fs;
use std::process::Command;
use std::sync::{LazyLock, Mutex, MutexGuard};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

const GUARDED_ENV_VARS: &[&str] = &[
    "CANOPUS_BACKEND_REGISTRY_CONFIG",
    "CANOPUS_PRE_RUN_HELPERS",
    "CANOPUS_AGENT_RUNTIME",
    "CANOPUS_AGENT_COMMAND",
    "SAFE_HINT",
    "GITHUB_TOKEN",
    "CANOPUS_ENABLE_LIVE_MUTATIONS",
];

struct EnvGuard {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn new() -> Self {
        let guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = GUARDED_ENV_VARS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for key in GUARDED_ENV_VARS {
            std::env::remove_var(key);
        }
        Self {
            _guard: guard,
            saved,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.iter().rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn git_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "canopus@example.invalid"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Canopus Test"])
        .current_dir(&root)
        .output()
        .unwrap();
    fs::write(root.join("README.md"), "# fixture\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&root)
        .output()
        .unwrap();

    root
}

fn current_branch(repo: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn runtime_log(state: &std::path::Path, agenda: &str, stage: &str) -> String {
    fs::read_to_string(
        state
            .join("artifacts")
            .join(format!("{agenda}-TASK-0-{stage}"))
            .join("runtime-log.md"),
    )
    .unwrap()
}

fn backend_selection(state: &std::path::Path, agenda: &str) -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(
            state
                .join("artifacts")
                .join(format!("{agenda}-backend-selection"))
                .join("runtime-log.md"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

fn write_executable(path: &std::path::Path, content: impl AsRef<str>) {
    fs::write(path, content.as_ref()).unwrap();
    make_executable(path);
}

#[tokio::test]
async fn planner_backend_can_swap_by_config_and_directive_without_branching() {
    let _env = EnvGuard::new();
    std::env::set_var("SAFE_HINT", "visible");
    std::env::set_var("GITHUB_TOKEN", "must-not-leak");
    std::env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "1");

    let root = std::env::temp_dir().join(format!(
        "canopus-plugin-runtime-fixtures-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script_a = root.join("backend-a.sh");
    let script_b = root.join("backend-b.sh");
    for (script, label) in [(&script_a, "sample_a"), (&script_b, "sample_b")] {
        write_executable(
            script,
            format!(
                "#!/bin/sh\nprintf 'backend=%s capability=%s safe=%s token=%s live=%s label={label}\\n' \"$CANOPUS_BACKEND\" \"$CANOPUS_CAPABILITY\" \"$SAFE_HINT\" \"$GITHUB_TOKEN\" \"$CANOPUS_ENABLE_LIVE_MUTATIONS\"\n"
            ),
        );
    }
    let config = root.join("registry.json");
    fs::write(
        &config,
        serde_json::json!({
            "backends": {
                "sample_a": {
                    "kind": "command",
                    "argv": [script_a.display().to_string()],
                    "env_allowlist": ["SAFE_HINT", "GITHUB_TOKEN", "CANOPUS_ENABLE_LIVE_MUTATIONS"]
                },
                "sample_b": {
                    "kind": "command",
                    "argv": [script_b.display().to_string()],
                    "env_allowlist": ["SAFE_HINT", "GITHUB_TOKEN", "CANOPUS_ENABLE_LIVE_MUTATIONS"]
                }
            },
            "capability_defaults": {
                "plan": "sample_a",
                "implement": "sample_a",
                "review": "sample_a"
            },
            "capability_override_allowlists": {
                "plan": ["sample_a", "sample_b"],
                "implement": ["sample_a"],
                "review": ["sample_a"]
            }
        })
        .to_string(),
    )
    .unwrap();
    std::env::set_var("CANOPUS_BACKEND_REGISTRY_CONFIG", &config);

    let repo_default = git_repo("plugin-runtime-default");
    let branch_default = current_branch(&repo_default);
    let state_default = repo_default.join(".canopus");
    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo_default.display().to_string(),
        "--state".to_string(),
        state_default.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-DEFAULT-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.planner".to_string(),
        "--role-mode".to_string(),
        "planner".to_string(),
        "make a plan".to_string(),
    ])
    .await
    .unwrap();

    let repo_directive = git_repo("plugin-runtime-directive");
    let branch_directive = current_branch(&repo_directive);
    let state_directive = repo_directive.join(".canopus");
    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo_directive.display().to_string(),
        "--state".to_string(),
        state_directive.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-DIRECTIVE-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.planner".to_string(),
        "--role-mode".to_string(),
        "planner".to_string(),
        "make a plan backend=sample_b".to_string(),
    ])
    .await
    .unwrap();

    assert_eq!(current_branch(&repo_default), branch_default);
    assert_eq!(current_branch(&repo_directive), branch_directive);

    let default_log = runtime_log(&state_default, "plugin-default-1", "plan");
    assert!(default_log.contains("backend=sample_a"));
    assert!(default_log.contains("label=sample_a"));
    assert!(default_log.contains("safe=visible"));
    assert!(default_log.contains("token= live="));

    let directive_log = runtime_log(&state_directive, "plugin-directive-1", "plan");
    assert!(directive_log.contains("backend=sample_b"));
    assert!(directive_log.contains("label=sample_b"));

    let selection = backend_selection(&state_directive, "plugin-directive-1");
    assert_eq!(selection["capability"], "plan");
    assert_eq!(selection["backend_name"], "sample_b");
    assert_eq!(selection["source"], "directive");
    assert_eq!(selection["default_or_override_source"], "directive");
    assert_eq!(selection["preparation"], "read-only");
    assert_eq!(selection["read_only"], true);

    let records: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(state_directive.join("runs/plugin-directive-1.json")).unwrap(),
    )
    .unwrap();
    assert!(records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "backend-selection" && record["status"] == "ok"));
    assert!(!records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "code"));

    let _ = fs::remove_dir_all(repo_default);
    let _ = fs::remove_dir_all(repo_directive);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn free_text_backend_hint_does_not_override_default() {
    let _env = EnvGuard::new();
    let root =
        std::env::temp_dir().join(format!("canopus-plugin-free-text-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script = root.join("backend.sh");
    write_executable(
        &script,
        "#!/bin/sh\nprintf 'backend=%s capability=%s\\n' \"$CANOPUS_BACKEND\" \"$CANOPUS_CAPABILITY\"\n",
    );
    let config = root.join("registry.json");
    fs::write(
        &config,
        serde_json::json!({
            "backends": {
                "sample_a": {"kind": "command", "argv": [script.display().to_string()]},
                "sample_b": {"kind": "command", "argv": [script.display().to_string()]}
            },
            "capability_defaults": {
                "plan": "sample_a",
                "implement": "sample_a",
                "review": "sample_a"
            },
            "capability_override_allowlists": {
                "plan": ["sample_a", "sample_b"],
                "implement": ["sample_a"],
                "review": ["sample_a"]
            }
        })
        .to_string(),
    )
    .unwrap();
    std::env::set_var("CANOPUS_BACKEND_REGISTRY_CONFIG", &config);

    let repo = git_repo("plugin-runtime-free-text");
    let state = repo.join(".canopus");
    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-FREETEXT-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.planner".to_string(),
        "--role-mode".to_string(),
        "planner".to_string(),
        "please use sample_b but not as a directive".to_string(),
    ])
    .await
    .unwrap();

    let log = runtime_log(&state, "plugin-freetext-1", "plan");
    assert!(log.contains("backend=sample_a"));
    let selection = backend_selection(&state, "plugin-freetext-1");
    assert_eq!(selection["source"], "default");
    assert_eq!(selection["default_or_override_source"], "default");

    let _ = fs::remove_dir_all(repo);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn unallowed_directive_records_failure_before_branch_or_command_execution() {
    let _env = EnvGuard::new();
    let root =
        std::env::temp_dir().join(format!("canopus-plugin-unallowed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script = root.join("backend-b.sh");
    write_executable(
        &script,
        "#!/bin/sh\nprintf executed > SHOULD_NOT_RUN.txt\nprintf executed\n",
    );
    let config = root.join("registry.json");
    fs::write(
        &config,
        serde_json::json!({
            "backends": {
                "sample_a": {"kind": "mock"},
                "sample_b": {"kind": "command", "argv": [script.display().to_string()]}
            },
            "capability_defaults": {
                "plan": "sample_a",
                "implement": "sample_a",
                "review": "sample_a"
            },
            "capability_override_allowlists": {
                "plan": ["sample_a"],
                "implement": ["sample_a"],
                "review": ["sample_a"]
            }
        })
        .to_string(),
    )
    .unwrap();
    std::env::set_var("CANOPUS_BACKEND_REGISTRY_CONFIG", &config);

    let repo = git_repo("plugin-runtime-unallowed");
    let branch = current_branch(&repo);
    let state = repo.join(".canopus");
    let err = cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-UNALLOWED-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.planner".to_string(),
        "--role-mode".to_string(),
        "planner".to_string(),
        "backend=sample_b".to_string(),
    ])
    .await
    .unwrap_err();

    assert!(err.to_string().contains("not allowed"));
    assert_eq!(current_branch(&repo), branch);
    assert!(!repo.join("SHOULD_NOT_RUN.txt").exists());
    let selection = backend_selection(&state, "plugin-unallowed-1");
    assert_eq!(selection["status"], "failed");
    assert!(selection["reason"]
        .as_str()
        .unwrap()
        .contains("not allowed"));
    assert_eq!(selection["capability"], "plan");
    assert_eq!(selection["requested_backend"], "sample_b");
    assert_eq!(selection["directive_source"], "directive");
    assert_eq!(selection["read_only"], true);
    let records: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(state.join("runs/plugin-unallowed-1.json")).unwrap(),
    )
    .unwrap();
    assert!(records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "backend-selection" && record["status"] == "failed"));

    let _ = fs::remove_dir_all(repo);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn reviewer_capability_uses_read_only_review_pipeline_without_branching() {
    let _env = EnvGuard::new();
    let root = std::env::temp_dir().join(format!("canopus-plugin-reviewer-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script = root.join("reviewer.sh");
    write_executable(
        &script,
        "#!/bin/sh\nprintf 'backend=%s capability=%s role=%s\\n' \"$CANOPUS_BACKEND\" \"$CANOPUS_CAPABILITY\" \"$CANOPUS_ROLE\"\n",
    );
    let config = root.join("registry.json");
    fs::write(
        &config,
        serde_json::json!({
            "backends": {
                "review_backend": {"kind": "command", "argv": [script.display().to_string()]}
            },
            "capability_defaults": {
                "plan": "review_backend",
                "implement": "review_backend",
                "review": "review_backend"
            },
            "capability_override_allowlists": {
                "plan": ["review_backend"],
                "implement": ["review_backend"],
                "review": ["review_backend"]
            }
        })
        .to_string(),
    )
    .unwrap();
    std::env::set_var("CANOPUS_BACKEND_REGISTRY_CONFIG", &config);

    let repo = git_repo("plugin-runtime-reviewer");
    let branch = current_branch(&repo);
    let state = repo.join(".canopus");
    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-REVIEW-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.reviewer".to_string(),
        "--role-mode".to_string(),
        "reviewer".to_string(),
        "review this".to_string(),
    ])
    .await
    .unwrap();

    assert_eq!(current_branch(&repo), branch);
    let log = runtime_log(&state, "plugin-review-1", "review");
    assert!(log.contains("capability=review"));
    assert!(log.contains("role=reviewer"));
    let selection = backend_selection(&state, "plugin-review-1");
    assert_eq!(selection["capability"], "review");
    assert_eq!(selection["preparation"], "read-only");
    assert_eq!(selection["read_only"], true);
    let records: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(state.join("runs/plugin-review-1.json")).unwrap())
            .unwrap();
    assert!(records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "review"));
    assert!(!records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "code"));

    let _ = fs::remove_dir_all(repo);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn command_backend_timeout_is_enforced() {
    let _env = EnvGuard::new();
    let config_root =
        std::env::temp_dir().join(format!("canopus-plugin-timeout-{}", std::process::id()));
    let _ = fs::remove_dir_all(&config_root);
    fs::create_dir_all(&config_root).unwrap();
    let config = config_root.join("registry.json");
    fs::write(
        &config,
        serde_json::json!({
            "backends": {
                "slow": {
                    "kind": "command",
                    "argv": ["/bin/sh", "-c", "sleep 1"],
                    "timeout_seconds": 0
                }
            },
            "capability_defaults": {
                "plan": "slow",
                "implement": "slow",
                "review": "slow"
            },
            "capability_override_allowlists": {
                "plan": ["slow"],
                "implement": ["slow"],
                "review": ["slow"]
            }
        })
        .to_string(),
    )
    .unwrap();
    std::env::set_var("CANOPUS_BACKEND_REGISTRY_CONFIG", &config);

    let repo = git_repo("plugin-runtime-timeout");
    let state = repo.join(".canopus");
    let err = cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-TIMEOUT-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.planner".to_string(),
        "--role-mode".to_string(),
        "planner".to_string(),
        "timeout proof".to_string(),
    ])
    .await
    .unwrap_err();

    assert!(err.to_string().contains("timed out"));

    let _ = fs::remove_dir_all(repo);
    let _ = fs::remove_dir_all(config_root);
}

#[tokio::test]
async fn conflicting_role_mode_and_task_type_fail_before_branch_creation() {
    let _env = EnvGuard::new();
    let repo = git_repo("plugin-runtime-conflict");
    let branch = current_branch(&repo);
    let state = repo.join(".canopus");

    let err = cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-CONFLICT-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.agent".to_string(),
        "--role-mode".to_string(),
        "planner".to_string(),
        "conflicting request".to_string(),
    ])
    .await
    .unwrap_err();

    assert!(err.to_string().contains("conflicts"));
    assert_eq!(current_branch(&repo), branch);
    let selection = backend_selection(&state, "plugin-conflict-1");
    assert_eq!(selection["status"], "failed");
    assert!(selection["reason"].as_str().unwrap().contains("conflicts"));
    let records: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(state.join("runs/plugin-conflict-1.json")).unwrap(),
    )
    .unwrap();
    assert!(records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "backend-selection" && record["status"] == "failed"));

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn read_only_capability_rejects_backend_repo_mutation() {
    let _env = EnvGuard::new();
    let root = std::env::temp_dir().join(format!(
        "canopus-plugin-mutating-backend-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script = root.join("mutating.sh");
    write_executable(
        &script,
        "#!/bin/sh\nprintf mutation > SHOULD_NOT_EXIST.txt\nprintf mutated\n",
    );
    let config = root.join("registry.json");
    fs::write(
        &config,
        serde_json::json!({
            "backends": {
                "mutating": {
                    "kind": "command",
                    "argv": [script.display().to_string()]
                }
            },
            "capability_defaults": {
                "plan": "mutating",
                "implement": "mutating",
                "review": "mutating"
            },
            "capability_override_allowlists": {
                "plan": ["mutating"],
                "implement": ["mutating"],
                "review": ["mutating"]
            }
        })
        .to_string(),
    )
    .unwrap();
    std::env::set_var("CANOPUS_BACKEND_REGISTRY_CONFIG", &config);

    let repo = git_repo("plugin-runtime-read-only-mutation");
    let branch = current_branch(&repo);
    let state = repo.join(".canopus");
    let err = cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "PLUGIN-MUTATION-1".to_string(),
        "--task-type".to_string(),
        "custom:canopus.planner".to_string(),
        "--role-mode".to_string(),
        "planner".to_string(),
        "plan should be read only".to_string(),
    ])
    .await
    .unwrap_err();

    assert!(err.to_string().contains("read-only backend"));
    assert_eq!(current_branch(&repo), branch);
    assert!(!repo.join("SHOULD_NOT_EXIST.txt").exists());
    let records: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(state.join("runs/plugin-mutation-1.json")).unwrap(),
    )
    .unwrap();
    assert!(records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "plan" && record["status"] == "failed"));

    let _ = fs::remove_dir_all(repo);
    let _ = fs::remove_dir_all(root);
}

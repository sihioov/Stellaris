# Canopus Local Patch MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first Canopus CLI workflow that turns a request into a local branch, simulated agent patch, checks, and saved artifacts without pushing or creating PRs.

**Architecture:** Add `apps/canopus` as a new Rust workspace crate. Keep Canopus domain logic in `core`, external contracts in `ports`, and concrete local/Stellaris implementations in `adapters`. The first working loop uses a deterministic mock agent runtime so the orchestration, branch, diff, and artifact flow is testable before binding to a real AI CLI.

**Tech Stack:** Rust 2021, standard library, existing `dysonsphere` crate, existing `tokio` runtime, local git command execution.

---

## File Structure

- Modify `Cargo.toml`: add `apps/canopus` to the workspace.
- Modify `dysonsphere/src/message.rs`: add a generic `TaskType::Custom(String)` variant so Canopus can map tasks without putting AI-specific names into Dysonsphere.
- Create `dysonsphere/tests/task_type.rs`: lock JSON compatibility for existing `NewsA` and new custom task types.
- Create `apps/canopus/Cargo.toml`: Canopus crate manifest.
- Create `apps/canopus/src/lib.rs`: crate module exports.
- Create `apps/canopus/src/main.rs`: CLI entrypoint.
- Create `apps/canopus/src/core/mod.rs`, `error.rs`, `types.rs`, `workflow.rs`: pure Canopus domain types and state transitions.
- Create `apps/canopus/src/ports/mod.rs`, `artifact_store.rs`, `task_backend.rs`, `agent_runtime.rs`, `tool_gateway.rs`: backend-agnostic port traits.
- Create `apps/canopus/src/adapters/mod.rs`: adapter namespace.
- Create `apps/canopus/src/adapters/artifact_store/mod.rs`, `local_file.rs`: local artifact persistence.
- Create `apps/canopus/src/adapters/task_backend/mod.rs`, `stellaris.rs`: maps Canopus agent tasks to Dysonsphere task messages.
- Create `apps/canopus/src/adapters/agent_runtime/mod.rs`, `mock.rs`: deterministic runtime that simulates planner/coder/reviewer work.
- Create `apps/canopus/src/adapters/tool_gateway/mod.rs`, `local.rs`: local git/check/diff gateway.
- Create `apps/canopus/src/cli/mod.rs`: argument parsing and command orchestration.
- Create `apps/canopus/tests/core_workflow.rs`: state transition tests.
- Create `apps/canopus/tests/local_file_artifact_store.rs`: artifact persistence tests.
- Create `apps/canopus/tests/stellaris_task_backend.rs`: Stellaris mapping tests.
- Create `apps/canopus/tests/mock_agent_runtime.rs`: mock runtime behavior tests.
- Create `apps/canopus/tests/local_tool_gateway.rs`: git gateway tests in a temp repository.
- Create `apps/canopus/tests/cli_submit.rs`: end-to-end CLI handler test using mock adapters.

## Task 1: Workspace And Generic Task Type

**Files:**
- Modify: `dysonsphere/src/message.rs`
- Create: `dysonsphere/tests/task_type.rs`

- [ ] **Step 1: Write the failing Dysonsphere task type test**

Create `dysonsphere/tests/task_type.rs`:

```rust
use dysonsphere::message::TaskType;

#[test]
fn news_a_serialization_stays_compatible() {
    let json = serde_json::to_string(&TaskType::NewsA).unwrap();
    assert_eq!(json, "\"NewsA\"");
}

#[test]
fn custom_task_type_can_represent_application_workloads() {
    let task_type = TaskType::Custom("canopus.agent".to_string());
    let json = serde_json::to_string(&task_type).unwrap();
    let decoded: TaskType = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, task_type);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
cargo test -p dysonsphere --test task_type
```

Expected: FAIL because `TaskType::Custom` does not exist.

- [ ] **Step 3: Add the generic task type variant**

Modify `dysonsphere/src/message.rs`:

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum TaskType {
    NewsA,
    Custom(String),
}
```

- [ ] **Step 4: Run the Dysonsphere test**

Run:

```powershell
cargo test -p dysonsphere --test task_type
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add dysonsphere/src/message.rs dysonsphere/tests/task_type.rs
git commit -m "[dysonsphere] feat: allow generic task types (Refs #10)"
```

## Task 2: Canopus Crate Scaffold

**Files:**
- Modify: `Cargo.toml`
- Create: `apps/canopus/Cargo.toml`
- Create: `apps/canopus/src/lib.rs`
- Create: `apps/canopus/src/main.rs`

- [ ] **Step 1: Create the crate manifest**

Create `apps/canopus/Cargo.toml`:

```toml
[package]
name = "canopus"
version = "0.1.0"
edition = "2021"

[dependencies]
dysonsphere = { path = "../../dysonsphere" }
tokio = { version = "1.44.2", features = ["rt-multi-thread"] }
```

- [ ] **Step 2: Add Canopus to the workspace**

Modify root `Cargo.toml`:

```toml
[workspace]
members = [
    "ton618",
    "dysonsphere",
    "laniakea",
    "apps/canopus"
]
```

- [ ] **Step 3: Create the library entrypoint**

Create `apps/canopus/src/lib.rs`:

```rust
pub mod adapters;
pub mod cli;
pub mod core;
pub mod ports;
```

- [ ] **Step 4: Create the binary entrypoint**

Create `apps/canopus/src/main.rs`:

```rust
fn main() {
    if let Err(err) = canopus::cli::run(std::env::args().collect()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 5: Add temporary empty module files so the crate compiles**

Create:

```text
apps/canopus/src/adapters/mod.rs
apps/canopus/src/cli/mod.rs
apps/canopus/src/core/mod.rs
apps/canopus/src/ports/mod.rs
```

Use this content in `apps/canopus/src/cli/mod.rs`:

```rust
use crate::core::error::CanopusResult;

pub fn run(_args: Vec<String>) -> CanopusResult<()> {
    Ok(())
}
```

Use this content in `apps/canopus/src/core/mod.rs`:

```rust
pub mod error;
```

Create `apps/canopus/src/core/error.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanopusError {
    InvalidInput(String),
    InvalidTransition(String),
    Io(String),
    Backend(String),
    Tool(String),
    Runtime(String),
}

impl fmt::Display for CanopusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanopusError::InvalidInput(message) => write!(f, "invalid input: {message}"),
            CanopusError::InvalidTransition(message) => write!(f, "invalid transition: {message}"),
            CanopusError::Io(message) => write!(f, "io error: {message}"),
            CanopusError::Backend(message) => write!(f, "backend error: {message}"),
            CanopusError::Tool(message) => write!(f, "tool error: {message}"),
            CanopusError::Runtime(message) => write!(f, "runtime error: {message}"),
        }
    }
}

impl std::error::Error for CanopusError {}

impl From<std::io::Error> for CanopusError {
    fn from(value: std::io::Error) -> Self {
        CanopusError::Io(value.to_string())
    }
}

pub type CanopusResult<T> = Result<T, CanopusError>;
```

Leave `apps/canopus/src/adapters/mod.rs` and `apps/canopus/src/ports/mod.rs` empty in this task. They become populated when the port and adapter tasks are implemented.

- [ ] **Step 6: Run the crate check**

Run:

```powershell
cargo check -p canopus
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add Cargo.toml apps/canopus
git commit -m "[infra] feat: add Canopus workspace crate (Refs #10)"
```

## Task 3: Core Domain And State Machine

**Files:**
- Modify: `apps/canopus/src/core/mod.rs`
- Create: `apps/canopus/src/core/types.rs`
- Create: `apps/canopus/src/core/workflow.rs`
- Create: `apps/canopus/tests/core_workflow.rs`

- [ ] **Step 1: Write the failing workflow tests**

Create `apps/canopus/tests/core_workflow.rs`:

```rust
use canopus::core::{
    AgentRole, AgentTask, Agenda, ArtifactKind, WorkflowState,
};

#[test]
fn agenda_rejects_empty_request() {
    let result = Agenda::new_with_id("CANOPUS-1", "   ");
    assert!(result.is_err());
}

#[test]
fn agenda_creates_planner_task() {
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-1", &agenda, AgentRole::Planner);

    assert_eq!(task.id, "TASK-1");
    assert_eq!(task.agenda_id, "CANOPUS-1");
    assert_eq!(task.role, AgentRole::Planner);
    assert!(task.prompt.contains("add tests"));
}

#[test]
fn workflow_allows_local_patch_path() {
    let state = WorkflowState::Created
        .transition_to(WorkflowState::Planned)
        .unwrap()
        .transition_to(WorkflowState::Executing)
        .unwrap()
        .transition_to(WorkflowState::Checking)
        .unwrap()
        .transition_to(WorkflowState::Reviewed)
        .unwrap()
        .transition_to(WorkflowState::Completed)
        .unwrap();

    assert_eq!(state, WorkflowState::Completed);
}

#[test]
fn workflow_rejects_skipping_plan() {
    let err = WorkflowState::Created
        .transition_to(WorkflowState::Executing)
        .unwrap_err();

    assert!(err.to_string().contains("Created -> Executing"));
}

#[test]
fn artifact_kind_has_stable_file_names() {
    assert_eq!(ArtifactKind::Plan.file_name(), "plan.md");
    assert_eq!(ArtifactKind::Diff.file_name(), "diff.md");
    assert_eq!(ArtifactKind::TestResult.file_name(), "test-result.md");
    assert_eq!(ArtifactKind::Review.file_name(), "review.md");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```powershell
cargo test -p canopus --test core_workflow
```

Expected: FAIL because the core types do not exist.

- [ ] **Step 3: Implement core module exports**

Modify `apps/canopus/src/core/mod.rs`:

```rust
pub mod error;
pub mod types;
pub mod workflow;

pub use error::{CanopusError, CanopusResult};
pub use types::{AgentRole, AgentTask, Agenda, Artifact, ArtifactKind, AgentRunResult};
pub use workflow::WorkflowState;
```

- [ ] **Step 4: Implement core types**

Create `apps/canopus/src/core/types.rs`:

```rust
use crate::core::error::{CanopusError, CanopusResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agenda {
    pub id: String,
    pub request: String,
    pub source: String,
}

impl Agenda {
    pub fn new_with_id(id: impl Into<String>, request: impl Into<String>) -> CanopusResult<Self> {
        let request = request.into();
        if request.trim().is_empty() {
            return Err(CanopusError::InvalidInput("request must not be empty".to_string()));
        }

        Ok(Self {
            id: id.into(),
            request: request.trim().to_string(),
            source: "cli".to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRole {
    Planner,
    Coder,
    Reviewer,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Planner => "planner",
            AgentRole::Coder => "coder",
            AgentRole::Reviewer => "reviewer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTask {
    pub id: String,
    pub agenda_id: String,
    pub role: AgentRole,
    pub prompt: String,
}

impl AgentTask {
    pub fn for_agenda(id: impl Into<String>, agenda: &Agenda, role: AgentRole) -> Self {
        Self {
            id: id.into(),
            agenda_id: agenda.id.clone(),
            role,
            prompt: format!("Agenda {}: {}", agenda.id, agenda.request),
        }
    }

    pub fn to_backend_payload(&self) -> String {
        format!(
            "agenda_id={}\nrole={}\nprompt={}\n",
            self.agenda_id,
            self.role.as_str(),
            self.prompt
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactKind {
    Plan,
    Diff,
    TestResult,
    Review,
    RuntimeLog,
}

impl ArtifactKind {
    pub fn file_name(&self) -> &'static str {
        match self {
            ArtifactKind::Plan => "plan.md",
            ArtifactKind::Diff => "diff.md",
            ArtifactKind::TestResult => "test-result.md",
            ArtifactKind::Review => "review.md",
            ArtifactKind::RuntimeLog => "runtime-log.md",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub task_id: String,
    pub kind: ArtifactKind,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunResult {
    pub task_id: String,
    pub summary: String,
    pub artifacts: Vec<Artifact>,
}
```

- [ ] **Step 5: Implement workflow transitions**

Create `apps/canopus/src/core/workflow.rs`:

```rust
use crate::core::error::{CanopusError, CanopusResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowState {
    Created,
    Planned,
    Executing,
    Checking,
    Reviewed,
    Completed,
    Failed,
}

impl WorkflowState {
    pub fn transition_to(self, next: WorkflowState) -> CanopusResult<WorkflowState> {
        let allowed = matches!(
            (self, next),
            (WorkflowState::Created, WorkflowState::Planned)
                | (WorkflowState::Planned, WorkflowState::Executing)
                | (WorkflowState::Executing, WorkflowState::Checking)
                | (WorkflowState::Checking, WorkflowState::Reviewed)
                | (WorkflowState::Reviewed, WorkflowState::Completed)
                | (_, WorkflowState::Failed)
        );

        if allowed {
            Ok(next)
        } else {
            Err(CanopusError::InvalidTransition(format!("{self:?} -> {next:?}")))
        }
    }
}
```

- [ ] **Step 6: Run the workflow tests**

Run:

```powershell
cargo test -p canopus --test core_workflow
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add apps/canopus/src/core apps/canopus/tests/core_workflow.rs
git commit -m "[infra] feat: define Canopus core workflow (Refs #10)"
```

## Task 4: Port Traits

**Files:**
- Modify: `apps/canopus/src/ports/mod.rs`
- Create: `apps/canopus/src/ports/artifact_store.rs`
- Create: `apps/canopus/src/ports/task_backend.rs`
- Create: `apps/canopus/src/ports/agent_runtime.rs`
- Create: `apps/canopus/src/ports/tool_gateway.rs`

- [ ] **Step 1: Add the port module exports**

Modify `apps/canopus/src/ports/mod.rs`:

```rust
pub mod agent_runtime;
pub mod artifact_store;
pub mod task_backend;
pub mod tool_gateway;

pub use agent_runtime::{AgentContext, AgentRuntime};
pub use artifact_store::{ArtifactLocation, ArtifactStore};
pub use task_backend::{SubmittedTask, TaskBackend};
pub use tool_gateway::{CommandOutput, ToolGateway};
```

- [ ] **Step 2: Add the artifact store port**

Create `apps/canopus/src/ports/artifact_store.rs`:

```rust
use crate::core::{Artifact, CanopusResult};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLocation {
    pub path: PathBuf,
}

pub trait ArtifactStore {
    fn save(&self, artifact: &Artifact) -> CanopusResult<ArtifactLocation>;
}
```

- [ ] **Step 3: Add the task backend port**

Create `apps/canopus/src/ports/task_backend.rs`:

```rust
use crate::core::{AgentTask, CanopusResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedTask {
    pub backend_id: String,
}

pub trait TaskBackend {
    fn submit(&self, task: &AgentTask) -> CanopusResult<SubmittedTask>;
}
```

- [ ] **Step 4: Add the agent runtime port**

Create `apps/canopus/src/ports/agent_runtime.rs`:

```rust
use crate::core::{AgentRunResult, AgentTask, CanopusResult};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContext {
    pub repo_path: PathBuf,
}

pub trait AgentRuntime {
    fn run(&self, task: &AgentTask, context: &AgentContext) -> CanopusResult<AgentRunResult>;
}
```

- [ ] **Step 5: Add the tool gateway port**

Create `apps/canopus/src/ports/tool_gateway.rs`:

```rust
use crate::core::CanopusResult;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait ToolGateway {
    fn ensure_clean_worktree(&self, repo: &Path) -> CanopusResult<()>;
    fn create_branch(&self, repo: &Path, branch: &str) -> CanopusResult<CommandOutput>;
    fn run_check(&self, repo: &Path, command: &[&str]) -> CanopusResult<CommandOutput>;
    fn changed_files(&self, repo: &Path) -> CanopusResult<CommandOutput>;
}
```

- [ ] **Step 6: Run check**

Run:

```powershell
cargo check -p canopus
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add apps/canopus/src/ports
git commit -m "[infra] feat: add Canopus port contracts (Refs #10)"
```

## Task 5: Local File Artifact Store Adapter

**Files:**
- Modify: `apps/canopus/src/adapters/mod.rs`
- Create: `apps/canopus/src/adapters/artifact_store/mod.rs`
- Create: `apps/canopus/src/adapters/artifact_store/local_file.rs`
- Create: `apps/canopus/tests/local_file_artifact_store.rs`

- [ ] **Step 1: Write the failing artifact store test**

Create `apps/canopus/tests/local_file_artifact_store.rs`:

```rust
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
cargo test -p canopus --test local_file_artifact_store
```

Expected: FAIL because `LocalFileArtifactStore` does not exist.

- [ ] **Step 3: Add adapter module exports**

Modify `apps/canopus/src/adapters/mod.rs`:

```rust
pub mod agent_runtime;
pub mod artifact_store;
pub mod task_backend;
pub mod tool_gateway;
```

Create `apps/canopus/src/adapters/artifact_store/mod.rs`:

```rust
pub mod local_file;

pub use local_file::LocalFileArtifactStore;
```

Create empty module files for later adapters:

```text
apps/canopus/src/adapters/agent_runtime/mod.rs
apps/canopus/src/adapters/task_backend/mod.rs
apps/canopus/src/adapters/tool_gateway/mod.rs
```

- [ ] **Step 4: Implement the local file store**

Create `apps/canopus/src/adapters/artifact_store/local_file.rs`:

```rust
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
```

- [ ] **Step 5: Run the artifact store test**

Run:

```powershell
cargo test -p canopus --test local_file_artifact_store
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add apps/canopus/src/adapters apps/canopus/tests/local_file_artifact_store.rs
git commit -m "[infra] feat: persist Canopus artifacts locally (Refs #10)"
```

## Task 6: Stellaris Task Backend Adapter

**Files:**
- Modify: `apps/canopus/src/adapters/task_backend/mod.rs`
- Create: `apps/canopus/src/adapters/task_backend/stellaris.rs`
- Create: `apps/canopus/tests/stellaris_task_backend.rs`

- [ ] **Step 1: Write the failing backend adapter test**

Create `apps/canopus/tests/stellaris_task_backend.rs`:

```rust
use canopus::adapters::task_backend::StellarisTaskBackend;
use canopus::core::{AgentRole, AgentTask, Agenda};
use canopus::ports::TaskBackend;
use dysonsphere::db::{FileTaskTable, TaskTable};
use dysonsphere::message::TaskType;
use std::fs;

fn test_file(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("canopus-{name}-{}.json", std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn submits_agent_task_as_stellaris_task_message() {
    let path = test_file("stellaris-backend");
    let backend = StellarisTaskBackend::new(path.clone()).unwrap();
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-1", &agenda, AgentRole::Coder);

    let submitted = backend.submit(&task).unwrap();

    assert_eq!(submitted.backend_id, "TASK-1");

    let table = FileTaskTable::new(path.clone());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let stored = runtime.block_on(table.fetch("TASK-1")).unwrap().unwrap();

    assert_eq!(stored.task_id, "TASK-1");
    assert_eq!(stored.task_type, TaskType::Custom("canopus.agent".to_string()));
    assert!(stored.payload.contains("role=coder"));
    assert!(stored.payload.contains("add tests"));
    let _ = fs::remove_file(path);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
cargo test -p canopus --test stellaris_task_backend
```

Expected: FAIL because `StellarisTaskBackend` does not exist.

- [ ] **Step 3: Export the adapter**

Modify `apps/canopus/src/adapters/task_backend/mod.rs`:

```rust
pub mod stellaris;

pub use stellaris::StellarisTaskBackend;
```

- [ ] **Step 4: Implement the adapter**

Create `apps/canopus/src/adapters/task_backend/stellaris.rs`:

```rust
use crate::core::{AgentTask, CanopusError, CanopusResult};
use crate::ports::{SubmittedTask, TaskBackend};
use dysonsphere::db::{FileTaskTable, TaskTable};
use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use std::path::PathBuf;
use tokio::runtime::Runtime;

pub struct StellarisTaskBackend {
    table: FileTaskTable,
    runtime: Runtime,
}

impl StellarisTaskBackend {
    pub fn new(path: PathBuf) -> CanopusResult<Self> {
        let runtime = Runtime::new().map_err(|err| CanopusError::Backend(err.to_string()))?;
        Ok(Self {
            table: FileTaskTable::new(path),
            runtime,
        })
    }
}

impl TaskBackend for StellarisTaskBackend {
    fn submit(&self, task: &AgentTask) -> CanopusResult<SubmittedTask> {
        let message = TaskMessage {
            task_id: task.id.clone(),
            task_type: TaskType::Custom("canopus.agent".to_string()),
            payload: task.to_backend_payload(),
            meta: TaskMeta::default(),
        };

        self.runtime
            .block_on(self.table.create(message))
            .map_err(|err| CanopusError::Backend(err.to_string()))?;

        Ok(SubmittedTask {
            backend_id: task.id.clone(),
        })
    }
}
```

- [ ] **Step 5: Run the backend adapter test**

Run:

```powershell
cargo test -p canopus --test stellaris_task_backend
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add apps/canopus/src/adapters/task_backend apps/canopus/tests/stellaris_task_backend.rs
git commit -m "[infra] feat: map Canopus tasks onto Stellaris backend (Refs #10)"
```

## Task 7: Mock Agent Runtime Adapter

**Files:**
- Modify: `apps/canopus/src/adapters/agent_runtime/mod.rs`
- Create: `apps/canopus/src/adapters/agent_runtime/mock.rs`
- Create: `apps/canopus/tests/mock_agent_runtime.rs`

- [ ] **Step 1: Write the failing mock runtime tests**

Create `apps/canopus/tests/mock_agent_runtime.rs`:

```rust
use canopus::adapters::agent_runtime::MockAgentRuntime;
use canopus::core::{AgentRole, AgentTask, Agenda, ArtifactKind};
use canopus::ports::{AgentContext, AgentRuntime};
use std::fs;

fn test_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn planner_returns_plan_artifact() {
    let repo = test_repo("mock-planner");
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-PLAN", &agenda, AgentRole::Planner);
    let runtime = MockAgentRuntime;

    let result = runtime.run(&task, &AgentContext { repo_path: repo.clone() }).unwrap();

    assert_eq!(result.task_id, "TASK-PLAN");
    assert_eq!(result.artifacts[0].kind, ArtifactKind::Plan);
    assert!(result.artifacts[0].content.contains("Mock plan"));
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn coder_runtime_creates_a_repo_file_for_diff_testing() {
    let repo = test_repo("mock-coder");
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-CODE", &agenda, AgentRole::Coder);
    let runtime = MockAgentRuntime;

    let result = runtime.run(&task, &AgentContext { repo_path: repo.clone() }).unwrap();

    assert!(repo.join("canopus-mock-output.txt").exists());
    assert_eq!(result.artifacts[0].kind, ArtifactKind::RuntimeLog);
    let _ = fs::remove_dir_all(repo);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```powershell
cargo test -p canopus --test mock_agent_runtime
```

Expected: FAIL because `MockAgentRuntime` does not exist.

- [ ] **Step 3: Export the adapter**

Modify `apps/canopus/src/adapters/agent_runtime/mod.rs`:

```rust
pub mod mock;

pub use mock::MockAgentRuntime;
```

- [ ] **Step 4: Implement the mock runtime**

Create `apps/canopus/src/adapters/agent_runtime/mock.rs`:

```rust
use crate::core::{AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind, CanopusResult};
use crate::ports::{AgentContext, AgentRuntime};
use std::fs;

#[derive(Debug, Clone, Copy)]
pub struct MockAgentRuntime;

impl AgentRuntime for MockAgentRuntime {
    fn run(&self, task: &AgentTask, context: &AgentContext) -> CanopusResult<AgentRunResult> {
        match task.role {
            AgentRole::Planner => Ok(AgentRunResult {
                task_id: task.id.clone(),
                summary: "mock planner completed".to_string(),
                artifacts: vec![Artifact {
                    task_id: task.id.clone(),
                    kind: ArtifactKind::Plan,
                    content: format!("# Mock plan\n\n{}\n", task.prompt),
                }],
            }),
            AgentRole::Coder => {
                let output = context.repo_path.join("canopus-mock-output.txt");
                fs::write(&output, format!("{}\n", task.prompt))?;
                Ok(AgentRunResult {
                    task_id: task.id.clone(),
                    summary: "mock coder wrote canopus-mock-output.txt".to_string(),
                    artifacts: vec![Artifact {
                        task_id: task.id.clone(),
                        kind: ArtifactKind::RuntimeLog,
                        content: format!("Wrote {}\n", output.display()),
                    }],
                })
            }
            AgentRole::Reviewer => Ok(AgentRunResult {
                task_id: task.id.clone(),
                summary: "mock reviewer completed".to_string(),
                artifacts: vec![Artifact {
                    task_id: task.id.clone(),
                    kind: ArtifactKind::Review,
                    content: "# Mock review\n\nNo issues found in mock runtime.\n".to_string(),
                }],
            }),
        }
    }
}
```

- [ ] **Step 5: Run the mock runtime tests**

Run:

```powershell
cargo test -p canopus --test mock_agent_runtime
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add apps/canopus/src/adapters/agent_runtime apps/canopus/tests/mock_agent_runtime.rs
git commit -m "[infra] feat: add deterministic Canopus agent runtime (Refs #10)"
```

## Task 8: Local Tool Gateway Adapter

**Files:**
- Modify: `apps/canopus/src/adapters/tool_gateway/mod.rs`
- Create: `apps/canopus/src/adapters/tool_gateway/local.rs`
- Create: `apps/canopus/tests/local_tool_gateway.rs`

- [ ] **Step 1: Write the failing local tool gateway tests**

Create `apps/canopus/tests/local_tool_gateway.rs`:

```rust
use canopus::adapters::tool_gateway::LocalToolGateway;
use canopus::ports::ToolGateway;
use std::fs;
use std::process::Command;

fn git_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    Command::new("git").arg("init").current_dir(&root).output().unwrap();
    Command::new("git").args(["config", "user.email", "canopus@example.invalid"]).current_dir(&root).output().unwrap();
    Command::new("git").args(["config", "user.name", "Canopus Test"]).current_dir(&root).output().unwrap();
    fs::write(root.join("README.md"), "# fixture\n").unwrap();
    Command::new("git").args(["add", "README.md"]).current_dir(&root).output().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    root
}

#[test]
fn creates_branch_and_reports_diff_names() {
    let repo = git_repo("local-tool");
    let gateway = LocalToolGateway;

    gateway.ensure_clean_worktree(&repo).unwrap();
    gateway.create_branch(&repo, "canopus/test").unwrap();
    fs::write(repo.join("canopus-mock-output.txt"), "changed\n").unwrap();
    let diff = gateway.changed_files(&repo).unwrap();

    assert_eq!(diff.status, 0);
    assert!(diff.stdout.contains("canopus-mock-output.txt"));
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn rejects_disallowed_check_command() {
    let repo = git_repo("local-tool-deny");
    let gateway = LocalToolGateway;

    let err = gateway.run_check(&repo, &["powershell", "-Command", "Write-Output nope"]).unwrap_err();

    assert!(err.to_string().contains("command is not allowlisted"));
    let _ = fs::remove_dir_all(repo);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```powershell
cargo test -p canopus --test local_tool_gateway
```

Expected: FAIL because `LocalToolGateway` does not exist.

- [ ] **Step 3: Export the adapter**

Modify `apps/canopus/src/adapters/tool_gateway/mod.rs`:

```rust
pub mod local;

pub use local::LocalToolGateway;
```

- [ ] **Step 4: Implement the adapter**

Create `apps/canopus/src/adapters/tool_gateway/local.rs`:

```rust
use crate::core::{CanopusError, CanopusResult};
use crate::ports::{CommandOutput, ToolGateway};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy)]
pub struct LocalToolGateway;

impl LocalToolGateway {
    fn run_command(&self, repo: &Path, command: &[&str]) -> CanopusResult<CommandOutput> {
        if command.is_empty() {
            return Err(CanopusError::Tool("command must not be empty".to_string()));
        }

        if !matches!(command[0], "git" | "cargo") {
            return Err(CanopusError::Tool(format!(
                "command is not allowlisted: {}",
                command[0]
            )));
        }

        let output = Command::new(command[0])
            .args(&command[1..])
            .current_dir(repo)
            .output()?;

        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

impl ToolGateway for LocalToolGateway {
    fn ensure_clean_worktree(&self, repo: &Path) -> CanopusResult<()> {
        let output = self.run_command(repo, &["git", "status", "--porcelain"])?;
        if output.status != 0 {
            return Err(CanopusError::Tool(output.stderr));
        }
        if !output.stdout.trim().is_empty() {
            return Err(CanopusError::Tool("worktree is not clean".to_string()));
        }
        Ok(())
    }

    fn create_branch(&self, repo: &Path, branch: &str) -> CanopusResult<CommandOutput> {
        let output = self.run_command(repo, &["git", "checkout", "-b", branch])?;
        if output.status == 0 {
            Ok(output)
        } else {
            Err(CanopusError::Tool(output.stderr))
        }
    }

    fn run_check(&self, repo: &Path, command: &[&str]) -> CanopusResult<CommandOutput> {
        self.run_command(repo, command)
    }

    fn changed_files(&self, repo: &Path) -> CanopusResult<CommandOutput> {
        let mut output = self.run_command(repo, &["git", "status", "--porcelain"])?;
        output.stdout = output
            .stdout
            .lines()
            .filter(|line| {
                let path = line.get(3..).unwrap_or("").trim();
                !(path == ".canopus" || path.starts_with(".canopus/") || path.starts_with(".canopus\\"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !output.stdout.is_empty() {
            output.stdout.push('\n');
        }
        Ok(output)
    }
}
```

- [ ] **Step 5: Run the local tool gateway tests**

Run:

```powershell
cargo test -p canopus --test local_tool_gateway
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add apps/canopus/src/adapters/tool_gateway apps/canopus/tests/local_tool_gateway.rs
git commit -m "[infra] feat: add local Canopus tool gateway (Refs #10)"
```

## Task 9: CLI Submit/Status/Artifacts Flow

**Files:**
- Modify: `apps/canopus/src/cli/mod.rs`
- Create: `apps/canopus/tests/cli_submit.rs`

- [ ] **Step 1: Write the failing CLI flow test**

Create `apps/canopus/tests/cli_submit.rs`:

```rust
use canopus::cli;
use std::fs;
use std::process::Command;

fn git_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    Command::new("git").arg("init").current_dir(&root).output().unwrap();
    Command::new("git").args(["config", "user.email", "canopus@example.invalid"]).current_dir(&root).output().unwrap();
    Command::new("git").args(["config", "user.name", "Canopus Test"]).current_dir(&root).output().unwrap();
    fs::write(root.join("README.md"), "# fixture\n").unwrap();
    Command::new("git").args(["add", "README.md"]).current_dir(&root).output().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(&root).output().unwrap();

    root
}

#[test]
fn submit_creates_branch_patch_backend_task_and_artifacts() {
    let repo = git_repo("cli-submit");
    let state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "add test coverage".to_string(),
    ]).unwrap();

    assert!(repo.join("canopus-mock-output.txt").exists());
    assert!(state.join("artifacts").join("TASK-1-plan").join("plan.md").exists());
    assert!(state.join("artifacts").join("TASK-2-code").join("runtime-log.md").exists());
    assert!(state.join("artifacts").join("TASK-3-review").join("review.md").exists());
    assert!(state.join("tasks.json").exists());

    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "canopus/CANOPUS-1");

    let _ = fs::remove_dir_all(repo);
}
```

- [ ] **Step 2: Run the CLI test to verify it fails**

Run:

```powershell
cargo test -p canopus --test cli_submit
```

Expected: FAIL because CLI submit is not implemented.

- [ ] **Step 3: Implement CLI parsing and orchestration**

Replace `apps/canopus/src/cli/mod.rs`:

```rust
use crate::adapters::agent_runtime::MockAgentRuntime;
use crate::adapters::artifact_store::LocalFileArtifactStore;
use crate::adapters::task_backend::StellarisTaskBackend;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::core::{AgentRole, AgentTask, Agenda, Artifact, ArtifactKind, CanopusError, CanopusResult};
use crate::ports::{AgentContext, AgentRuntime, ArtifactStore, TaskBackend, ToolGateway};
use std::path::PathBuf;

pub fn run(args: Vec<String>) -> CanopusResult<()> {
    if args.len() < 2 {
        return Err(CanopusError::InvalidInput(usage()));
    }

    match args[1].as_str() {
        "submit" => submit(&args[2..]),
        "status" => status(&args[2..]),
        "artifacts" => artifacts(&args[2..]),
        _ => Err(CanopusError::InvalidInput(usage())),
    }
}

fn submit(args: &[String]) -> CanopusResult<()> {
    let parsed = SubmitArgs::parse(args)?;
    let agenda = Agenda::new_with_id("CANOPUS-1", parsed.request)?;
    let branch = format!("canopus/{}", agenda.id);
    let artifact_store = LocalFileArtifactStore::new(parsed.state.join("artifacts"));
    let backend = StellarisTaskBackend::new(parsed.state.join("tasks.json"))?;
    let runtime = MockAgentRuntime;
    let tools = LocalToolGateway;

    tools.ensure_clean_worktree(&parsed.repo)?;
    tools.create_branch(&parsed.repo, &branch)?;

    let plan_task = AgentTask::for_agenda("TASK-1-plan", &agenda, AgentRole::Planner);
    backend.submit(&plan_task)?;
    let plan_result = runtime.run(&plan_task, &AgentContext { repo_path: parsed.repo.clone() })?;
    for artifact in &plan_result.artifacts {
        artifact_store.save(artifact)?;
    }

    let code_task = AgentTask::for_agenda("TASK-2-code", &agenda, AgentRole::Coder);
    backend.submit(&code_task)?;
    let code_result = runtime.run(&code_task, &AgentContext { repo_path: parsed.repo.clone() })?;
    for artifact in &code_result.artifacts {
        artifact_store.save(artifact)?;
    }

    let diff = tools.changed_files(&parsed.repo)?;
    artifact_store.save(&Artifact {
        task_id: code_task.id.clone(),
        kind: ArtifactKind::Diff,
        content: format!("# Diff\n\n```text\n{}```\n", diff.stdout),
    })?;

    let check = tools.run_check(&parsed.repo, &["git", "diff", "--check"])?;
    artifact_store.save(&Artifact {
        task_id: code_task.id.clone(),
        kind: ArtifactKind::TestResult,
        content: format!(
            "# Check\n\nstatus: {}\n\n## stdout\n```text\n{}```\n\n## stderr\n```text\n{}```\n",
            check.status, check.stdout, check.stderr
        ),
    })?;

    let review_task = AgentTask::for_agenda("TASK-3-review", &agenda, AgentRole::Reviewer);
    backend.submit(&review_task)?;
    let review_result = runtime.run(&review_task, &AgentContext { repo_path: parsed.repo })?;
    for artifact in &review_result.artifacts {
        artifact_store.save(artifact)?;
    }

    println!("Canopus task {} completed local patch flow on branch {branch}", agenda.id);
    Ok(())
}

fn status(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput("usage: canopus status <task-id>".to_string()));
    }
    println!("{}: local status is file-backed in MVP", args[0]);
    Ok(())
}

fn artifacts(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput("usage: canopus artifacts <task-id>".to_string()));
    }
    println!("artifacts for {} are under .canopus/artifacts", args[0]);
    Ok(())
}

struct SubmitArgs {
    repo: PathBuf,
    state: PathBuf,
    request: String,
}

impl SubmitArgs {
    fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = PathBuf::from(".");
        let mut state = PathBuf::from(".canopus");
        let mut request_parts = Vec::new();
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    let value = args.get(index).ok_or_else(|| {
                        CanopusError::InvalidInput("--repo requires a path".to_string())
                    })?;
                    repo = PathBuf::from(value);
                }
                "--state" => {
                    index += 1;
                    let value = args.get(index).ok_or_else(|| {
                        CanopusError::InvalidInput("--state requires a path".to_string())
                    })?;
                    state = PathBuf::from(value);
                }
                value => request_parts.push(value.to_string()),
            }
            index += 1;
        }

        let request = request_parts.join(" ");
        if request.trim().is_empty() {
            return Err(CanopusError::InvalidInput("submit requires a request".to_string()));
        }

        Ok(Self { repo, state, request })
    }
}

fn usage() -> String {
    "usage: canopus submit [--repo <path>] [--state <path>] <request>".to_string()
}
```

- [ ] **Step 4: Run the CLI test**

Run:

```powershell
cargo test -p canopus --test cli_submit
```

Expected: PASS.

- [ ] **Step 5: Run all Canopus tests**

Run:

```powershell
cargo test -p canopus
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add apps/canopus/src/cli apps/canopus/tests/cli_submit.rs
git commit -m "[infra] feat: add Canopus local patch CLI flow (Refs #10)"
```

## Task 10: Workspace Verification And Documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/stellaris-deck.md`

- [ ] **Step 1: Update README module list**

Add this subsection near the existing component overview in `README.md`:

```markdown
### Canopus (AI development orchestration layer)

Canopus is an application layer built on top of Stellaris. It keeps AI development orchestration outside the core engine by using ports and adapters for task backends, agent runtimes, tool gateways, artifact storage, and intake surfaces.

The first Canopus milestone is a Local Patch MVP: a CLI request creates a local branch, simulates bounded agent work, runs local checks, and stores plan, diff, test, and review artifacts without pushing or creating a PR.
```

- [ ] **Step 2: Update the deck structure**

Add `apps/canopus` to the structure in `docs/stellaris-deck.md`:

```markdown
├── apps/
│   └── canopus/                 ← portable AI development orchestration layer [MVP]
```

- [ ] **Step 3: Run workspace tests**

Run:

```powershell
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 4: Run workspace check**

Run:

```powershell
cargo check --workspace
```

Expected: PASS.

- [ ] **Step 5: Confirm no remote operations were added**

Run:

```powershell
rg -n "push|pull request|merge|deploy|gh pr|git push" apps/canopus
```

Expected: output may include documentation strings explaining exclusions, but no code path that executes `git push`, PR creation, merge, or deployment commands.

- [ ] **Step 6: Commit**

```powershell
git add README.md docs/stellaris-deck.md
git commit -m "[docs] docs: document Canopus MVP entrypoint (Refs #10)"
```

## Final Verification

- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo check --workspace`.
- [ ] Run `cargo run -p canopus -- submit --repo <clean-fixture-repo> --state <clean-fixture-repo>/.canopus "add test coverage"` against a disposable clean git repository.
- [ ] Verify the disposable repository is on branch `canopus/CANOPUS-1`.
- [ ] Verify the disposable repository contains `canopus-mock-output.txt`.
- [ ] Verify `.canopus/tasks.json` exists in the disposable repository.
- [ ] Verify `.canopus/artifacts/TASK-1-plan/plan.md` exists.
- [ ] Verify `.canopus/artifacts/TASK-2-code/diff.md` exists.
- [ ] Verify `.canopus/artifacts/TASK-2-code/test-result.md` exists.
- [ ] Verify `.canopus/artifacts/TASK-3-review/review.md` exists.
- [ ] Run `rg -n "git push|gh pr|merge|deploy" apps/canopus/src` and confirm there are no matches.

## Self-Review

Spec coverage:

- Portable Canopus layer: covered by `core`, `ports`, and `adapters` split.
- Stellaris backend adapter: covered by Task 6.
- Local patch MVP: covered by Tasks 7 through 9.
- Local artifacts: covered by Task 5 and CLI submit flow.
- Safety exclusions for push, PR, merge, and deploy: covered by Task 10 and final verification.

Placeholder scan:

- No placeholder markers are intentionally left in this plan.

Type consistency:

- `Agenda`, `AgentTask`, `AgentRole`, `Artifact`, `ArtifactKind`, `AgentRunResult`, `WorkflowState`, `TaskBackend`, `AgentRuntime`, `ToolGateway`, and `ArtifactStore` are defined before use by adapters or CLI code.

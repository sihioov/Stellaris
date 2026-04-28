# Stellaris / Canopus v1: 단일 흐름 Discord-Driven AI 작업 파이프라인

## 0. 문서 위치

- **이 문서 (v1)**: `docs/stellaris-canopus-architecture-v1.md`
  현재 진행 중인 구현의 정합 모델이자, v2로 가기 위한 디딤돌 설계.
- **v2 최종본**: `docs/stellaris-canopus-architecture.md`
  Discord-native, GitHub Project-aware, multi-agent orchestration kernel.

v1은 v2의 **부분집합 + 데이터/포트 호환 레이어**다. v1에서 만드는 모든 데이터 구조, port/adapter, workflow 상태는 v2로 무손실 확장 가능해야 한다.

---

## 1. v1의 한 줄 정의

```text
Canopus v1은 Discord에서 받은 단일 요청을
고정된 파이프라인 (Analyst → Planner → Coder → Check → Reviewer)으로
순차 실행하고, GitHub Issue/PR로 산출물을 떨구는
single-agent-per-stage AI 작업 자동화 시스템이다.
```

핵심 차이 (v1 vs v2):

```text
v1:  Stage-by-stage pipeline, 단일 agent role per stage, artifact 전달
v2:  Multi-agent orchestration, Agent Message Bus, turn-based conversation
```

v1은 **agent 간 대화가 없다**. Stage 사이의 정보 전달은 **artifact 파일 + 상태 전이**로 끝낸다.

---

## 2. v1 범위

## 2.1 In Scope

```text
- Discord 명령 → 작업 큐 등록
- 채널 이름 기반 pipeline 분기 (#planning / #development / #review)
- TON618 스케줄러가 작업 큐 폴링
- Laniakea 워커가 Canopus 바이너리 실행
- Canopus CLI가 고정 stage pipeline 수행
- 각 stage에서 단일 agent 호출 (LLM 또는 mock)
- Artifact 파일 저장 (.canopus/artifacts/)
- Discord webhook으로 stage 진행 알림
- GitHub Q&A Issue (Analyst stage)
- Discord !approve / !reject로 작업 승인
- 승인된 작업만 git push + gh pr create
- Hubble의 자율 코드 스캐너 → 후보 task 등록
- Tool Gateway 기본 policy (allowlist 기반)
- File-based task storage (tasks-{category_id}.json)
- 다중 프로젝트 지원 (Discord category = git repo)
```

## 2.2 Out of Scope (v2로 미룸)

```text
- Agent Message Bus
- Agent 간 turn-based 대화 / debate
- Conversation / round / max_rounds 제어
- Multi-agent council review
- GitHub Project board 양방향 동기화
- 7-type Approval (PLAN/SCOPE/RISKY_TOOL/...) — v1은 단일 PendingReview gate
- Decision Memory
- Postgres / Redis / NATS
- Webhook 기반 실시간 GitHub 동기화
- Agent별 권한 분리
```

## 2.3 Non-Goals (v1에서도 절대 안 함)

```text
- Agent가 main branch에 직접 push
- Approval 없이 PR merge / deploy
- Discord 메시지를 source of truth로 쓰기
- Hubble이 사람 확인 없이 모든 발견을 즉시 작업으로 변환 (현재 안티패턴 → §11에서 보완)
```

---

## 3. v1 최상위 아키텍처

```text
┌──────────────────────────────────────────────────────────┐
│                       Discord                            │
│  category = project, channel = pipeline mode             │
│  - !new-project / !register                              │
│  - !run <요청>                                            │
│  - !approve / !reject [task_id]                          │
│  - !status                                                │
└────────────────────────┬─────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────┐
│                Discord Bot (Python)                       │
│  - command parsing                                        │
│  - tasks-{category_id}.json 작성                          │
│  - channel name → task_type.Custom("canopus.X") 매핑      │
│  - PendingReview → Processed/Failed 상태 전환             │
└────────────────────────┬─────────────────────────────────┘
                         │ writes
                         ▼
┌──────────────────────────────────────────────────────────┐
│   Task Queue (file): tasks-{category_id}.json            │
│   schema = dysonsphere::TaskMessage                      │
│   states = Pending → Dispatched → PendingReview          │
│            → Processed → Failed (terminal)               │
└────────────────────────┬─────────────────────────────────┘
                         │ polled by
                         ▼
┌──────────────────────────────────────────────────────────┐
│              TON618 (Rust 스케줄러)                       │
│  - FileTaskDataSource로 10초 폴링                         │
│  - Pending → Dispatched 전이                              │
│  - dispatcher → Laniakea 채널/파일                        │
└────────────────────────┬─────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────┐
│              Laniakea (Rust AI 워커)                      │
│  - Dispatched task 수신                                   │
│  - task_type에 따라 handler 선택                          │
│  - canopus 바이너리 spawn (subprocess)                    │
│  - stdout/stderr 로깅                                     │
│  - 종료 시 PendingReview로 상태 전환                       │
└────────────────────────┬─────────────────────────────────┘
                         │ exec
                         ▼
┌──────────────────────────────────────────────────────────┐
│               Canopus CLI (Rust)                          │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │           Pipeline Orchestrator                      │ │
│  │  - WorkflowState 상태머신                            │ │
│  │  - stage 순차 실행                                    │ │
│  │  - stage 간 artifact 전달                             │ │
│  │  - Pipeline = DevMode | PlanOnly |                   │ │
│  │               ReviewOnly | Maintenance               │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │         Agent Runtime (port)                         │ │
│  │  - 단일 agent 호출, agent 간 대화 없음                │ │
│  │  - adapter: MockAgentRuntime (현재)                  │ │
│  │  - adapter: LLMAgentRuntime (v1 목표)                │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │         Tool Gateway (port)                          │ │
│  │  - allowlist 기반 git/gh/cargo 실행                   │ │
│  │  - block: force-push, main 직접 수정, secret 접근     │ │
│  │  - adapter: LocalToolGateway                         │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │         Artifact Store (port)                        │ │
│  │  - kind: Plan / Diff / TestResult / Review / QA      │ │
│  │  - adapter: LocalFileArtifactStore                   │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │         GitHub Adapter                               │ │
│  │  - Issue 생성/comment 폴링/close                      │ │
│  │  - PR 생성 (gh pr create via Tool Gateway)            │ │
│  │  - Project board는 v1에서는 read-only/없음            │ │
│  └─────────────────────────────────────────────────────┘ │
└────────────────────────┬─────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────┐
│                   GitHub                                  │
│  - Q&A Issue (Analyst → 사람 답변)                        │
│  - Branch / Commit / PR                                   │
│  - Branch protection이 최종 merge 안전장치                 │
└──────────────────────────────────────────────────────────┘

           ┌──────────────────────────────────────┐
           │   Hubble (Rust 자율 코드 스캐너)        │
           │  - cargo clippy 기반 발견              │
           │  - workspace 등록 → 등록된 repo 감시   │
           │  - finding → "agenda 후보" task 등록   │
           │  - 단, status=PendingProposal로 표시   │
           │    (사람이 !approve 해야 Pending로 승격) │ ← v1 보완 포인트
           └──────────────────────────────────────┘
```

---

## 4. 컴포넌트 책임

## 4.1 Discord Bot (`apps/discord-bot/bot.py`)

```text
- Discord category 단위로 프로젝트를 격리
  └ projects.json에 (category_id → repo_path) 매핑 저장
- 채널 이름이 곧 pipeline mode
  └ #planning   → canopus.planner   (Plan만)
  └ #development → canopus.agent     (Plan+Code+Review 풀)
  └ #review      → canopus.reviewer  (리뷰만)
  └ #general     → 명령 안 받음
- !run 시 tasks-{category_id}.json에 TaskMessage 추가
  └ task_type = TaskType::Custom("canopus.X")
  └ payload = {"request": "<원문>", "repo_path": "..."}
- !approve / !reject 는 PendingReview 상태 task만 다룸
  └ approve → Processed (Canopus watch가 후속 처리)
  └ reject  → Failed     (terminal)
- ALLOWED_USER_IDS 환경변수로 작동 권한 제한
```

v1 변경 없음. 다만 §11 Hubble과 연계하여 `PendingProposal` 상태 처리만 추가.

## 4.2 Task Queue (`tasks-{category_id}.json`)

dysonsphere의 `TaskMessage` 스키마를 그대로 사용한다.

```rust
TaskMessage {
    task_id: String,            // "discord-<uuid12>" 또는 "hubble-<uuid12>"
    task_type: TaskType,        // Custom("canopus.planner") | Bug | Security | TestCoverage | UXImprovement
    payload: String,            // JSON {"request", "repo_path", ...}
    meta: TaskMeta {
        status: TaskStatus,     // Pending | Dispatched | PendingReview | Processed | Failed
        created_at,
        updated_at,
    },
}
```

v1 보완:

```text
- payload schema를 typed struct로 굳힌다 (CanopusPayload)
- TaskStatus에 PendingProposal 추가 (Hubble 자동 발견 → 사람 승인 대기)
- file lock은 fs2로 이미 적용됨 (커밋 ae989ac)
- 카테고리별 파일 분리는 그대로 유지 (project isolation)
```

`PendingProposal` 추가는 **v2 호환 보강**이다. v2의 `agenda_proposal` 데이터 모델과 1:1 매핑된다.

## 4.3 TON618 (`ton618/`)

```text
- FileTaskDataSource로 tasks-*.json polling
- Pending → Dispatched 상태만 책임
- 우선순위 큐 / Schedule 엔진은 그대로 활용
- v1에서는 다중 tasks-*.json을 동시에 watch (multi-project)
```

v1 보완:

```text
- 다중 task 파일 polling 지원 (현재 단일 파일 가능성, 확인 필요)
- PendingProposal 상태는 dispatch 대상에서 제외
- 실패 재시도 횟수 제한 (max_retries)
```

## 4.4 Laniakea (`laniakea/`)

```text
- Dispatched task 수신
- handler를 task_type으로 분기
  └ canopus.planner / canopus.agent / canopus.reviewer
  └ Bug / Security / TestCoverage / UXImprovement (Hubble 발 task)
- canopus 바이너리를 subprocess로 실행
- 환경변수 주입: CANOPUS_REPO, CANOPUS_STATE
- 종료 코드 처리 → Dispatched → PendingReview 또는 Failed
```

v1 보완:

```text
- canopus 실행 시 stdout/stderr를 .canopus/logs/{task_id}.log로 보존
- timeout 적용 (예: 30분 hard cap)
- panic/crash 시 Failed 상태 + 사유 기록
```

## 4.5 Canopus CLI (`apps/canopus/`)

v1 핵심. `canopus submit`이 stage pipeline을 돌린다.

### 4.5.1 Pipeline 정의

```rust
enum Pipeline {
    DevMode,        // Analyst → Planner → Coder → Check → Reviewer
    PlanOnly,       // Analyst → Planner
    ReviewOnly,     // Reviewer
    Maintenance,    // Bug/Security/TestCoverage/UX → 좁은 stage set
}
```

채널 이름 → task_type → Pipeline 매핑:

```text
canopus.planner   → PlanOnly
canopus.agent     → DevMode
canopus.reviewer  → ReviewOnly
Bug/Security/...  → Maintenance
```

### 4.5.2 WorkflowState (현재 7-state 유지)

```text
Created → Planned → Executing → Checking → Reviewed → Completed
                                                  └→ Failed (any)
```

v1에서는 이 상태머신을 그대로 유지한다. **v2의 21-state 머신은 v1 위에 superset으로 얹는다.**

v1 → v2 매핑:

```text
v1.Created   → v2.WORKFLOW_CREATED
v1.Planned   → v2.PLANNING + PLAN_REVIEW
v1.Executing → v2.AGENT_RUNNING + TOOL_EXECUTING
v1.Checking  → v2.TESTING
v1.Reviewed  → v2.AGENT_REVIEW
v1.Completed → v2.COMPLETED
```

### 4.5.3 Stage 실행 모델

각 stage는 다음을 수행:

```text
1. AgentTask 생성 → backend.submit
2. AgentRuntime.run(task, context, prior_artifacts) 호출
3. result.artifacts → ArtifactStore.save
4. Discord webhook 알림
5. WorkflowState.transition_to(다음 상태)
6. 다음 stage로 prior_artifacts 누적 전달
```

**Agent 간 대화 없음**. 다음 agent는 이전 agent의 artifact를 input으로 받을 뿐이다.

### 4.5.4 Q&A Issue Sub-flow (Analyst stage)

현재 구현 (`apps/canopus/src/cli/mod.rs:57~95`)을 v1 표준으로 굳힌다.

```text
1. Analyst agent가 질문 목록 artifact 생성
2. GitHub Q&A Issue 생성 ("[Canopus Q&A] <request>")
3. Discord에 "Issue #N에 답변해 주세요" 알림
4. 30초 간격으로 새 comment 폴링
5. 새 답변 감지 시 Planner stage로 진행
6. post_approval 시 Issue close
```

v1 보완:

```text
- 폴링 timeout 추가 (기본 24시간 → 그 이후 Failed)
- 답변자가 ALLOWED_USER_IDS 또는 repo collaborator인지 확인
- 답변 내용을 다음 stage의 prior_artifact에 포함
```

## 4.6 AgentRuntime (port + adapters)

```rust
trait AgentRuntime {
    async fn run(
        &self,
        task: &AgentTask,
        ctx: &AgentContext,
        prior: &[Artifact],
    ) -> Result<AgentRunResult>;
}
```

v1 adapter 2종:

```text
MockAgentRuntime    → 현재 (placeholder artifact)
LLMAgentRuntime     → v1 목표 (실제 Claude/OpenAI 호출)
```

v1 LLMAgentRuntime 요구사항:

```text
- Role별 system prompt
  ├ Planner   : "다음 요청을 작은 단위로 분해하고 구현 계획을 세워라"
  ├ Coder     : "Plan을 따라 unified diff를 생성하라"
  ├ Reviewer  : "Diff와 check 결과를 검토하고 위험을 보고하라"
  └ Analyst   : "요청에서 모호한 부분을 질문 목록으로 만들어라"
- API key는 환경변수 (ANTHROPIC_API_KEY 등)
- Token / cost 로깅
- Retry on rate limit
- Timeout (예: stage당 5분)
- 출력 형식 강제: artifact 1개 이상 생성하지 못하면 Failed
```

`AgentRuntime` trait은 v2에서도 그대로 쓴다. v2는 같은 trait을 conversation context와 함께 호출하는 별도 adapter를 추가할 뿐이다.

## 4.7 ToolGateway (port + adapter)

현재 `LocalToolGateway`는 사실상 `git`/`gh`/임의 shell을 그냥 실행한다. v1에서는 **간이 policy**를 추가한다.

```rust
enum ToolPolicy {
    Allow,                  // 즉시 실행
    Deny(Reason),           // 차단
    RequireApproval,        // v2로 미룸 — v1은 Allow|Deny만
}
```

v1 allowlist:

```text
Allow:
  git status / diff / log / branch / checkout / add / commit
  git push -u origin <feature-branch>           ← branch != main/master
  gh pr create
  cargo build / check / clippy / test / fmt --check

Deny:
  git push --force / --force-with-lease
  git push origin main / master / develop
  git reset --hard
  git clean -fdx
  rm -rf
  any command containing secrets/.env/credentials
  curl/wget to non-allowlisted hosts
```

위반 시 `CanopusError::PolicyViolation` 반환 + Discord 알림 + Failed 상태.

이 policy 모듈이 v2의 Tool Gateway 정책 엔진의 v1 형태다.

## 4.8 ArtifactStore

현재 `LocalFileArtifactStore`로 충분. v1에서는 다음만 정리:

```text
경로: <state>/artifacts/<task_id>/<kind>-<seq>.md
kind: Plan | Diff | TestResult | Review | QA | Log
```

v2의 object storage 단계에서도 같은 namespace를 유지한다.

## 4.9 Hubble (자율 코드 스캐너)

§11에서 자세히 기술. v1에서는 **자동 task 등록 → 사람 승인** 흐름으로 보완한다.

---

## 5. 데이터 모델 (v1 → v2 호환)

v1에서 유지·신규로 만드는 데이터 구조:

```text
[유지]
TaskMessage          (dysonsphere::message)
TaskStatus           (dysonsphere::status)
TaskType             (dysonsphere::message)
Agenda               (canopus::core)
AgentTask            (canopus::core)
Artifact / ArtifactKind
WorkflowState
Pipeline

[v1 신규]
CanopusPayload       (TaskMessage.payload의 typed struct)
ToolPolicy / PolicyDecision
RunOutcome           (success/failure/timeout/cancelled)
StageRecord          (stage 시작/종료/소요시간/artifact 목록)

[v2 forward-compat — v1에서는 읽기/쓰기만 정의, 사용은 v2]
ApprovalRequest stub (단일 PendingReview gate를 ApprovalRequest로 view)
ProjectMapping stub  (category_id ↔ github_owner/repo 매핑)
AgendaProposal       (Hubble의 PendingProposal task → 이걸로 view 가능)
```

`StageRecord`만 새 테이블/파일로 추가하고, 나머지는 typed wrapper만 만들면 충분하다.

권장 저장 경로:

```text
.canopus/
  tasks-{category_id}.json     # 큐 (Discord Bot이 작성, TON618이 읽음)
  artifacts/<task_id>/...       # stage 산출물
  logs/<task_id>.log            # Laniakea 캡처 stdout/stderr
  runs/<task_id>.json           # StageRecord 리스트 (audit 단초)
  projects.json                 # ProjectMapping (Discord Bot에서 옮겨오기)
```

`runs/<task_id>.json`이 v2의 `audit_events`/`workflows` 테이블의 v1 형태다.

---

## 6. Discord 명령 set (v1 확정)

```text
[프로젝트]
!new-project <name> <repo_path>   디렉토리+git init+카테고리/채널+등록
!register <repo_path>             기존 카테고리에 repo 연결

[작업 실행]
!run <요청>                        현재 채널의 mode로 task 등록 (Pending)

[승인]
!approve [task_id]                PendingReview → Processed
!reject  [task_id]                PendingReview → Failed

[v1 신규]
!propose-approve [task_id]        PendingProposal → Pending (Hubble 발견 승격)
!propose-reject  [task_id]        PendingProposal → Failed
!cancel [task_id]                 진행 중 task 취소 (Failed로 강제)

[조회]
!status                            현재 프로젝트 task 목록
!show [task_id]                    artifact 요약 + GitHub Issue/PR 링크
!help                              도움말
```

`!cancel`과 `!propose-*`만 신규. 나머지는 현재 구현 그대로.

---

## 7. Workflow 전이 규칙 (v1 확정)

TaskStatus 측면:

```text
Pending           ← Discord Bot 또는 사용자가 PendingProposal에서 승격
Dispatched        ← TON618이 Pending에서 picked
PendingReview     ← Laniakea/Canopus가 stage 다 끝낸 직후
Processed         ← !approve 후, watch가 PR까지 만들고 종료
Failed            ← 어느 단계든 비정상 종료
PendingProposal   ← Hubble이 자동 등록 (사람 승격 대기) — 신규
```

WorkflowState 측면 (Canopus 내부, single run 안에서):

```text
Created → Planned → Executing → Checking → Reviewed → Completed
   │         │          │           │          │
   └─────────┴──────────┴───────────┴──────────┴──── Failed (terminal)
```

상태 전이는 **오직 Pipeline orchestrator에서만** 일어난다 (v2 §18 원칙 7과 정합).

---

## 8. Approval 모델 (v1 단순화)

v2의 7-type Approval은 **v1에서 두 종류로 압축**한다.

```text
v1 ApprovalGate (단일 Discord 명령으로 처리):

  ReviewGate
    - trigger: WorkflowState=Reviewed (Canopus 종료 직후)
    - status: PendingReview
    - actor : Discord !approve / !reject (ALLOWED_USER_IDS)
    - effect: !approve → Processed → watch가 PR 생성
              !reject  → Failed (terminal)

  ProposalGate (신규)
    - trigger: Hubble이 finding → task 등록 시
    - status: PendingProposal
    - actor : Discord !propose-approve / !propose-reject
    - effect: !propose-approve → Pending (정상 큐로 진입)
              !propose-reject  → Failed
```

merge 자체의 안전장치는 **GitHub branch protection**에 위임한다. v1은 PR을 만들어 두는 데까지만 책임진다.

이 두 gate는 v2에서 `approval_requests` 테이블의 row 2종(`PR_REVIEW_APPROVAL`, `PROPOSAL_APPROVAL`)으로 자연스럽게 확장된다.

---

## 9. GitHub 연동 범위 (v1)

```text
사용함:
- Issues API: 생성 / comment 조회 / close
  └ Q&A Issue, 향후 finding-issue
- PRs   API: 생성 (gh CLI 경유)
- gh CLI    : Tool Gateway를 통해 호출

사용 안 함 (v2로 미룸):
- GitHub Projects v2 (board, fields, items)
- Webhooks
- Checks API 직접 호출
- Status field 동기화
```

GitHub Project가 없는 대신, **v1의 source of truth는 `tasks-{category_id}.json`**이다. v2 전환 시 이 파일을 GitHub Project board로 마이그레이션하는 별도 작업이 필요하다 (§13).

---

## 10. Hubble의 v1 동작 (자율 review loop의 축소판)

현재 Hubble은 발견 즉시 task를 Pending으로 등록한다. 이는 v2 §18 원칙 10 위반.

v1 보완:

```text
[변경 전]
clippy 발견 → tasks.json에 status=Pending task 추가 → 즉시 파이프라인 진입

[변경 후]
clippy 발견
  → finding 정규화 (severity, category, file:line, suggested_fix)
  → "Discovery Brief" artifact 생성 (.canopus/findings/<id>.md)
  → tasks.json에 status=PendingProposal로 추가
  → Discord #general에 알림 ("새 후보 N개 발견 — !propose-approve <id>")

사용자가 !propose-approve <id>
  → PendingProposal → Pending
  → 이후 일반 파이프라인과 동일

사용자가 !propose-reject <id>
  → PendingProposal → Failed (이유 기록)
  → 같은 finding이 다음 스캔에서 재등록되지 않도록 dedup hash 저장
```

dedup hash 저장 위치:

```text
.canopus/hubble/seen.json
{
  "<finding_hash>": { "first_seen": "...", "last_status": "rejected", "task_id": "..." }
}
```

이 흐름이 v2의 `finding → agenda_proposal → user approval → workflow` 사이클의 v1 형태다.

---

## 11. 안전장치 / 안티패턴 방지

v1에서 절대 어기지 말 것:

```text
1. Pipeline orchestrator 외부에서 WorkflowState 전이 금지
2. AgentRuntime 또는 Hubble이 git/gh를 직접 실행 금지 — 반드시 ToolGateway 경유
3. ToolGateway가 Deny를 반환했는데 Caller가 우회 금지
4. Discord !approve 없이 git push / gh pr create 금지
5. Hubble이 PendingProposal 단계 건너뛰고 Pending으로 직접 등록 금지
6. Canopus가 main/master/develop 브랜치에 직접 commit 금지
7. tasks-*.json 외 source of truth 추가 금지 (DB 분기는 v2 작업)
8. AgentRuntime adapter가 LLM 호출 시 secret을 prompt에 그대로 노출 금지
9. .env / credentials 경로의 파일을 artifact로 저장 금지
10. Failed task를 자동 재시도 무한루프 금지 (max_retries=2)
```

이 10개 항목이 v1의 **준수 체크리스트**다. 각 항목은 v2 §18 핵심 원칙과 정합한다.

---

## 12. v1 기능 매트릭스 (현재 상태)

| 영역 | 항목 | 현재 | v1 목표 | v2 |
|---|---|---|---|---|
| Discord | 명령 셋 | ✅ 6개 | ✅ +!cancel/!propose-* | ✅ +slash command |
| Discord | 알림 webhook | ✅ | ✅ | ✅ + transcript projection |
| Queue | tasks-{cat}.json | ✅ | ✅ +PendingProposal | ✅ DB 마이그레이션 |
| Queue | file lock (fs2) | ✅ | ✅ | — |
| TON618 | 폴링 | ✅ | ✅ multi-file | ✅ event bus |
| Laniakea | subprocess 실행 | ✅ | ✅ +timeout/log 캡처 | ✅ pool |
| Canopus | WorkflowState 7단계 | ✅ | ✅ 그대로 | 21단계로 확장 |
| Canopus | Pipeline 분기 | ✅ DevMode/Plan/Review | ✅ +Maintenance | ✅ 동적 routing |
| Canopus | Q&A Issue 흐름 | ✅ | ✅ +timeout | ✅ |
| AgentRuntime | Mock | ✅ | (deprecate) | — |
| AgentRuntime | LLM (Claude) | 🔴 | ✅ **v1 핵심 작업** | ✅ +conversation |
| ToolGateway | shell exec | ✅ | ✅ +allowlist policy | ✅ +risk class |
| ArtifactStore | 로컬 파일 | ✅ | ✅ | ✅ +object storage |
| GitHub | Issue 생성/폴링 | ✅ | ✅ +collaborator 검증 | ✅ webhook |
| GitHub | PR 생성 | ✅ (gh CLI) | ✅ | ✅ octocrab API |
| GitHub | Project board | 🔴 | 🔴 (의도적 미포함) | ✅ |
| Hubble | clippy 스캔 | ✅ | ✅ | ✅ +다양한 분석기 |
| Hubble | 즉시 Pending 등록 | ⚠️ 안티패턴 | 🔴 PendingProposal 변경 | ✅ |
| Approval | !approve/!reject | ✅ | ✅ +!propose-* | ✅ 7-type |
| Approval | type 분류 | 🔴 | 🔴 (의도적 미포함) | ✅ |
| Audit | runs/<task_id>.json | 🔴 | ✅ **v1 신규** | ✅ DB |
| Conversation | Agent 간 대화 | 🔴 | 🔴 (의도적 미포함) | ✅ Message Bus |

## 12.1 v1 완성에 남은 작업 (우선순위)

```text
[P0 — 실제 가치 제공]
1. LLMAgentRuntime adapter 구현 (Claude API)
   - Role별 system prompt
   - Token/cost 로깅
   - Stage timeout
   - Mock과의 swap을 환경변수로 (CANOPUS_AGENT_RUNTIME=llm|mock)

[P0 — 안전성]
2. ToolGateway allowlist policy 도입
   - PolicyViolation 에러 + Discord 알림
   - main/master push 차단
   - secret 경로 차단

3. Hubble 자동 등록 → PendingProposal 변경
   - !propose-approve / !propose-reject 명령 추가
   - dedup hash 저장
   - Discord #general 알림

[P1 — 관측성]
4. StageRecord (runs/<task_id>.json) 기록
   - 각 stage 시작/종료/소요시간/artifact 목록
   - audit log의 v1 형태

5. Laniakea stdout/stderr → .canopus/logs/<task_id>.log

6. !show <task_id> 명령 — artifact 요약 + 링크

[P2 — 운영성]
7. Stage timeout / max_retries
8. !cancel 명령
9. Q&A Issue 답변자 권한 검증
10. ProjectMapping을 .canopus/projects.json으로 단일화
```

P0 3개를 끝내면 "v1 완성"으로 간주할 수 있다.

---

## 13. v1 → v2 마이그레이션 전략

v1 작업물을 가능한 한 깨지 않고 v2로 가는 경로:

## 13.1 데이터 마이그레이션

```text
tasks-{cat}.json
  → workflows table        (1 task = 1 workflow row)
  → tasks table            (각 stage가 1 task row)
  → artifacts table        (메타만, content는 object storage)

runs/<task_id>.json
  → audit_events table     (StageRecord → 여러 audit row)

PendingProposal task
  → agenda_proposals table

PendingReview gate
  → approval_requests row (type=PR_REVIEW_APPROVAL)

projects.json
  → github_project_mappings table
```

마이그레이션 스크립트는 v2 작업의 **첫 번째 작업**으로 두면 된다. 데이터 손실 없이 single-run 단위로 변환 가능.

## 13.2 코드 진화 경로

```text
[v1 → v2 step 1] AgentMessage 추가
  - port: AgentMessageBus trait 신설
  - adapter: InProcessMessageBus (먼저), DBMessageBus (나중)
  - Pipeline orchestrator를 그대로 두되,
    각 stage가 끝날 때 AgentMessage(type=task_result)를 bus에 발행
  - 처음에는 bus가 transcript 출력 용도로만 쓰임 (passive)

[v1 → v2 step 2] Conversation 모델
  - 각 stage 안에서 max_rounds=1로 동작 (현 v1과 동일)
  - 단, AgentTask가 Conversation에 묶이고 round=1로 기록됨
  - Discord transcript projection 추가 (#canopus-agent-room)

[v1 → v2 step 3] Multi-agent debate 활성화
  - Reviewer stage를 max_rounds>1로 확장
  - Architect/Security agent 추가 시 conversation에 합류
  - Orchestrator가 turn 결정

[v1 → v2 step 4] GitHub Project board 양방향
  - github_project_mappings 채우기
  - Status field projection
  - "Ready for Agent" item watcher

[v1 → v2 step 5] Approval 7-type 분리
  - PendingReview를 PR_REVIEW_APPROVAL로 rename
  - PLAN_APPROVAL, RISKY_TOOL_APPROVAL 등 추가
  - ToolGateway가 RequireApproval 정책 사용 시작

[v1 → v2 step 6] Postgres 도입
  - SQLite/file → Postgres 마이그레이션
  - Redis/NATS 큐 도입
```

각 step은 독립적으로 deploy 가능하도록 설계한다. v1 사용자가 갑자기 모든 게 바뀐 v2를 받지 않게.

## 13.3 v2 진입 조건

다음 조건을 만족할 때 v2 step 1을 시작한다:

```text
✅ v1 P0 3개(LLM, Tool policy, Hubble proposal) 완료
✅ 실제 사용자가 v1으로 1주 이상 안정 운영
✅ runs/ audit log가 충분히 쌓여 패턴 분석 가능
✅ "agent끼리 대화시키고 싶다"는 명시적 요구가 발생
```

마지막 조건이 중요하다. **실제 필요가 발생하기 전에 Message Bus를 먼저 짓지 않는다.**

---

## 14. v1 핵심 설계 원칙 (요약)

```text
1. Discord는 v1에서도 control plane이다.
2. tasks-{category_id}.json이 v1의 source of truth다.
3. Pipeline은 고정 stage 순서로 돈다 — 분기는 channel/task_type으로만.
4. Agent 간 대화는 v1에 없다 — artifact 전달로 충분하다.
5. Stage 진행은 Pipeline orchestrator만 한다.
6. Tool 실행은 모두 ToolGateway를 거친다.
7. !approve 없이 PR이 만들어지지 않는다.
8. 자율 발견(Hubble)은 PendingProposal로 등록되고, 사람이 승격한다.
9. 모든 stage는 runs/<task_id>.json에 기록된다.
10. v1의 모든 데이터는 v2로 무손실 마이그레이션 가능해야 한다.
```

---

## 15. 한 줄 요약

```text
v1 = Discord 명령 → 고정 pipeline → artifact + PR.
v2 = Discord 명령 → multi-agent 대화 → GitHub Project + PR.

v1은 v2의 stage들을 single-agent로 압축한 형태이며,
v2는 v1의 stage들을 conversation으로 펼친 형태다.

따라서 v1을 잘 만드는 것이 곧 v2의 토대를 만드는 것이다.
```

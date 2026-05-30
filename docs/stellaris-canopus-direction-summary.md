# Stellaris / Canopus Direction

이 문서는 Stellaris/Canopus의 기준 방향을 정리한다. 목적은 새 시스템을 갈아엎는 것이 아니라, 이미 있는 Europa, Canopus, Dysonsphere, TON618, Laniakea, GitHub 연동 조각을 하나의 end-to-end 개발 작업 관제 흐름으로 닫는 것이다.

V1은 먼저 local-first / dry-run / human approval 경계를 지키면서 재현 가능한 작업 loop를 완성한다. live GitHub mutation, PR 생성, CI backflow는 같은 방향 안에 두되 명시적인 gate 뒤에서 단계적으로 연다.

## 1. 핵심 방향

Stellaris/Canopus는 Hermes 같은 범용 agent shell이 아니라, **Discord에서 개발 작업을 발주하고 Canopus가 Codex, Oh My Codex, Superpowers 같은 coding runtime을 실행해 GitHub PR/CI/review까지 연결하는 human-in-the-loop 개발 작업 관제 시스템**으로 간다.

목표 흐름은 다음과 같다.

```text
Discord에서 자연어로 작업 지시
→ Stellaris/Europa가 작업을 받음
→ 내부 router가 intent 분류
→ Canopus가 적절한 runner 실행
→ 분석 작업이면 Discord에 바로 답변
→ 코드 수정이면 branch/worktree와 변경 artifact 준비
→ live gate가 열리면 PR 생성 및 GitHub Actions 실행
→ GitHub 상태가 다시 Stellaris로 들어옴
→ Discord thread에 진행상황 업데이트
→ 사람이 리뷰하고 merge
```

목표는 AI 챗봇이 아니라 **개발 작업 orchestration system**이다.

---

## 2. 역할 분리

```text
Stellaris / Europa
= control plane + Discord UI

Canopus
= execution engine + policy owner + runner orchestrator

Codex / Oh My Codex / Superpowers
= coding intelligence / workflow runtime

GitHub
= branch, PR, CI, review, merge gate

Discord
= operator interface
```

Canopus는 단순 실행 worker가 아니다. Canopus는 job lifecycle, workspace/branch/worktree 준비, checkpoint/artifact 저장, 안전 정책, 승인 gate, runner 호출, 결과 수집을 소유한다. Europa는 Discord surface이고, mutation 권한이나 GitHub policy 판단을 소유하지 않는다.

Hermes는 messaging, memory, assistant shell에는 좋지만, 이 프로젝트의 중심 책임인 작업 큐, job 상태 관리, branch/worktree 관리, PR 생성, CI 추적, GitHub webhook, Discord 진행상황 알림, human approval에는 Stellaris/Canopus 구조가 더 적합하다.

Hermes는 나중에 보조 assistant layer로 붙일 수는 있지만, core orchestrator가 되면 안 된다.

---

## 3. 사용자 UX

사용자가 매번 read-only인지 code-change인지 고를 필요는 없다.

사용자는 Discord에서 자연어로 말하면 된다.

```text
로그인 기능 추가해줘
로그인 로직 어디 있는지 분석해줘
PR #42 리뷰해줘
CI 실패 고쳐줘
```

Stellaris가 내부적으로 intent를 분류한다.

```text
로그인 기능 추가해줘
→ CodeChange

로그인 로직 어디 있는지 분석해줘
→ ReadOnlyAnalysis

PR #42 리뷰해줘
→ PrReview

CI 실패 고쳐줘
→ CiRepair

로그인 쪽 봐줘
→ 애매하므로 ReadOnlyAnalysis 먼저
```

read-only / write 구분은 사용자 UX가 아니라 **내부 실행 정책**이다.

---

## 4. Discord task thread

Task를 요청하면 해당 task 전용 Discord thread를 만드는 구조가 좋다.

```text
#stellaris-tasks
  ├─ [job_123] 로그인 기능 추가
  ├─ [job_124] 인증 구조 분석
  ├─ [job_125] PR #42 리뷰
  └─ [job_126] CI 실패 수정
```

thread 하나가 곧 job session이다.

```text
discord_thread_id
→ job_id
→ Canopus workspace
→ branch
→ PR
→ CI state
```

thread 안에서 사용자가 다음처럼 말하면:

```text
테스트도 더 추가해
```

새 작업이 아니라 기존 `job_123`의 follow-up으로 처리한다.

이 구조의 장점:

```text
- 작업별 context가 섞이지 않음
- 진행상황 로그가 자연스럽게 남음
- 사용자가 follow-up 하기 쉬움
- GitHub webhook 알림을 보낼 위치가 명확함
- 완료된 작업은 thread archive 가능
```

---

## 5. 세션 유지 방식

세션의 source of truth는 Codex transcript가 아니다.

진짜 세션은 다음 조합이다.

```text
Stellaris job record
+ Discord thread
+ Canopus workspace
+ checkpoint artifacts
+ git branch
+ PR
+ CI status
```

Codex session, Oh My Codex state, Superpowers context는 보조적인 runner session이다.

권장 구조:

```text
Discord thread
→ Stellaris job session
→ Canopus workspace/session
→ Codex or OMX runner session
→ Git branch / PR
```

job DB에는 최소 다음 정보를 저장한다.

```text
job_id
repo
instruction
intent
status
discord_thread_id
workspace_path
branch
runner_backend
runner_session_id
pr_number
pr_url
last_checkpoint
```

Codex resume이 가능하면 사용한다.

```text
codex exec resume <session_id>
```

하지만 resume이 실패해도 이어갈 수 있도록 Canopus는 checkpoint를 저장해야 한다.

```text
request.md
plan.md
checkpoint.md
result.json
test.log
diff summary
```

즉, Codex session은 있으면 좋지만 없어도 checkpoint, git diff, job state로 복구 가능해야 한다.

---

## 6. Canopus와 plugin/runtime의 관계

Canopus가 Codex ecosystem을 막으면 안 된다.

```text
Canopus는 Codex를 대체하지 않는다.
Canopus는 Codex / Superpowers / Oh My Codex를 실행할 수 있는 runner다.
```

좋은 구조:

```text
Canopus Runner
  ├─ codex
  ├─ codex_with_plugins
  ├─ omx
  ├─ future_claude_code
  └─ future_custom_runner
```

역할 분리:

```text
Canopus
= job lifecycle, workspace, branch, PR, CI, Discord status, safety policy

Codex
= coding intelligence

Superpowers
= planning / TDD / debugging / review workflow skills

Oh My Codex
= optional Codex workflow backend

GitHub
= review and verification gate
```

피해야 할 설계:

```text
Canopus가 자체 planning/TDD/worktree/agent-loop를 모두 강제
→ Superpowers/OMX와 충돌
```

지향해야 할 설계:

```text
Canopus가 workspace와 policy를 준비
→ 선택된 runner가 작업
→ Canopus가 결과/diff/test/PR/CI/Discord를 관리
```

---

## 7. GitHub의 역할

코드 변경은 무조건 branch/PR을 통해야 한다.

허용:

```text
- branch 생성
- commit 생성
- draft PR 생성
- GitHub Actions 실행
- PR/CI 상태 Discord 알림
```

금지:

```text
- main/master 직접 push
- force push
- 자동 merge
- 자동 deploy
- secret 출력
- .env commit
```

live-gated 기본 흐름:

```text
CodeChange job
→ branch/worktree 생성
→ Canopus runner 실행
→ local check
→ commit
→ push
→ draft PR
→ GitHub Actions
→ webhook으로 상태 수신
→ Discord thread 업데이트
→ human review
```

GitHub Actions는 검증 시스템이고, Stellaris는 작업 상태 control plane이다.

---

## 8. V1 실행 순서

현재 저장소는 완전히 빈 프로젝트가 아니다.

이미 있는 전제:

```text
- apps/canopus: AI development execution layer
- apps/europa: Discord control surface
- Canopus v1 방향성
- local-first / dry-run / approval 철학
- ToolGateway / safety gate 개념
- CI / smoke test 기반
```

앞으로 해야 할 일은 새로 갈아엎는 것이 아니라, 기존 구조를 연결해서 end-to-end loop를 닫는 것이다. 구현은 두 단계로 나눈다.

### 8.1 먼저 닫을 V1 local loop

V1의 첫 성공 기준은 live GitHub mutation이 아니라, local dry-run 환경에서 Discord 요청이 job으로 기록되고 Canopus runner/checkpoint/finalize 경로까지 재현되는 것이다.

우선순위:

```text
1. canonical direction 문서 확정
2. unified instruction router
3. job record / lifecycle 확장
4. task당 Discord thread 생성 및 job 매핑
5. checkpoint artifacts 저장
6. local dry-run E2E 검증
```

### 8.2 Unified instruction router

자연어 instruction을 내부 intent로 분류한다.

```text
ReadOnlyAnalysis
CodeChange
PrReview
CiRepair
NeedsClarification
```

초기에는 rule-based로 충분하다.

### 8.3 Job lifecycle 확장

PR/CI까지 확장 가능한 상태가 필요하다. 단, 초기 V1 local loop에서는 push/pr/ci 상태를 실제 live mutation 없이 dry-run artifact 또는 pending 상태로 표현할 수 있어야 한다.

```text
created
classified
running
diff_ready
local_commit_created
pushed
pr_created
ci_running
ci_passed
ci_failed
awaiting_human_review
done
failed
cancelled
```

### 8.4 Task당 Discord thread

작업 생성 시 thread를 만들고, 이후 모든 follow-up과 상태 업데이트를 해당 thread에 모은다.

```text
task request
→ job created
→ thread created
→ discord_thread_id 저장
→ 이후 메시지는 같은 job context
```

### 8.5 Checkpoint artifacts

runner session이 끊겨도 job을 이어갈 수 있도록 Canopus workspace에 최소 artifacts를 남긴다.

```text
request.md
plan.md
checkpoint.md
result.json
test.log
diff-summary.md
```

Codex/OMX/Superpowers의 session id는 있으면 사용하지만, source of truth는 job record와 checkpoint artifacts다.

### 8.6 Live-gated GitHub PR/CI backflow

PR 생성 후 GitHub Actions 상태가 다시 Discord로 돌아와야 한다.

```text
GitHub webhook
→ PR/check/workflow event
→ job_id 매핑
→ status 업데이트
→ Discord thread 알림
```

이 단계는 live mutation gate 뒤에 둔다. 기본값은 dry-run/read-only이고, branch push, draft PR 생성, Project mutation은 명시적인 승인과 환경 flag가 있을 때만 허용한다.

### 8.7 Runner backend 추상화

Canopus가 특정 runner만 강제하지 않도록 한다.

```text
codex
codex_with_plugins
omx
future_claude_code
future_custom_runner
```

초기에는 interface와 한 개의 안정적인 backend부터 닫는다. 다른 runner는 enum/확장 포인트로 남기고, 실제 다중 backend 운영은 local loop가 안정화된 뒤에 연다.

---

## 9. 최종 구조

```text
Discord parent channel
  ↓
Task request
  ↓
Stellaris/Europa
  - job 생성
  - intent 분류
  - Discord thread 생성
  ↓
Canopus
  - workspace 준비
  - branch/worktree 관리
  - runner backend 실행
  - checkpoint 저장
  - diff/test/result 수집
  ↓
Codex / Superpowers / OMX
  - 실제 분석/구현 수행
  ↓
Canopus
  - commit
  - PR 생성 (live gate 필요)
  ↓
GitHub Actions
  - CI 실행
  ↓
GitHub webhook
  ↓
Stellaris
  - job status 업데이트
  ↓
Discord thread
  - 진행상황/결과 알림
```

---

## 10. 한 줄 요약

Stellaris/Canopus는 Discord를 조종석으로, Canopus를 실행 엔진으로, Codex/OMX/Superpowers를 선택 가능한 coding runtime으로, GitHub를 검증 게이트로 사용하는 **human-in-the-loop AI development orchestration system**으로 간다.

지금 집중할 것:

```text
canonical direction 문서
→ intent router
→ Discord task thread
→ job session/checkpoint
→ local dry-run E2E
→ live-gated PR/CI lifecycle
→ GitHub webhook backflow
→ runner backend extension point
```

지금 하지 않을 것:

```text
Hermes core 도입
multi-agent debate
dashboard
자동 merge/deploy
대규모 rewrite
```

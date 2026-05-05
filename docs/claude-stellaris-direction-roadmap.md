# Codex Stellaris 방향성과 작업 순서

## 현재 시스템 방향성

Stellaris의 방향은 **분산 Task 처리 코어 위에 AI 개발 자동화 앱(Canopus)을 얹고, 장기적으로 GitHub agenda + Discord meeting room + 역할 에이전트 협업 플랫폼으로 확장**하는 것이다.

현재 구조는 크게 세 층으로 본다.

1. **Stellaris Core**
   - `TaskMessage`, `TaskStatus`, `TaskTable` 같은 공통 계약을 제공한다.
   - Hubble/Kepler가 후보 작업을 발견하고, TON618이 작업을 dispatch하며, Laniakea가 worker로 실행한다.
   - Discovery 결과는 곧바로 실행하지 않고 `PendingProposal`로 등록한 뒤 사용자 승인 후 `Pending`으로 승격한다.

2. **Canopus v1**
   - Stellaris 위에서 동작하는 AI 개발 자동화 workload다.
   - 목표 흐름은 `intake → plan → code/work → check → review → human approval → optional PR/follow-up`이다.
   - 기본은 dry-run/local artifact 중심이며, GitHub push/PR/Project mutation은 명시적 gate 없이는 실행하지 않는다.

3. **Stellaris v2 Agent Collaboration**
   - 장기 목표는 단순 명령 실행이 아니라 GitHub Issue/Project를 공식 agenda ledger로 두는 협업 시스템이다.
   - Discord는 사람이 볼 수 있는 meeting room 역할을 한다.
   - Planner, Developer, Designer, Reviewer, QA, Security, Writer 같은 역할 에이전트가 회의에 참여한다.
   - Proposal과 Decision을 만들고, 사용자 confirmation 이후에만 ActionItem과 executable Task로 전환한다.

## 현재 진행 상태 판단

- **Core file-backed MVP**는 일부 구현되어 있다.
  - `dysonsphere`, `ton618`, `laniakea`, `hubble`, `kepler`가 workspace에 존재한다.
  - 파일 기반 task table, pending dispatch, dispatched worker 처리 흐름이 있다.
  - 다만 production-grade DB, RabbitMQ durable status, Redis result/cache, 통합 테스트는 아직 후순위다.

- **Canopus v1**은 꽤 진행되어 있다.
  - CLI pipeline, workflow state, artifact 저장, GitHub Project v2 dry-run/read/live gate, Discord metadata 전달 구조가 존재한다.
  - 하지만 실제 live Discord delivery, GitHub push/PR/merge/deploy, credential rotation, production deployment는 아직 자동 검증 밖이다.

- **v2 multi-agent collaboration**은 주로 설계 문서 단계다.
  - Agenda, Room, Meeting, Proposal, Decision, ActionItem 개념은 정의되어 있다.
  - 하지만 이 도메인 모델이 core 구현으로 본격 반영되지는 않았다.

## 우선 작업 순서

### 0. 빌드 복구 / 중간 리팩토링 정리

최우선 작업이다.

현재 `StellarisError::DefaultError`는 enum에서 사라졌지만, `dysonsphere/src/db/task_table_file.rs`에는 아직 사용처가 남아 있다. 이 상태는 컴파일 에러로 이어질 가능성이 크다.

해야 할 일:

1. `DefaultError` 잔존 사용처 제거
2. `DuplicateTask`, `TaskNotFound` 등 구체 에러로 교체
3. `cargo check` / `cargo test`가 도는 상태까지 복구

### 1. Core task 상태 전이 계약 확정

Stellaris Core의 상태 머신을 먼저 고정해야 한다.

목표 흐름:

```text
PendingProposal
→ Pending
→ Dispatched
→ PendingReview
→ Processed / Failed
```

작업:

1. `TaskStatus::can_transition_to` 규칙 완성
2. `TaskMessage::transition_to()` 추가
   - status 변경
   - `updated_at` 갱신
   - 잘못된 전이 차단
3. `TaskTable::transition()` 추가
4. 기존 `update_status_if_current()` 호출을 `transition()`으로 교체
   - `ton618/src/main.rs`
   - `laniakea/src/worker.rs`
5. `task.meta.status = ...` 직접 대입 제거

이 단계가 끝나야 scheduler, worker, Discord, Canopus가 같은 상태 규칙을 공유할 수 있다.

### 2. CI / 검증 체계 고정

기본 검증 명령:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 -m py_compile apps/europa/europa.py
python3 -m unittest apps/europa/test_bot_config.py
```

이후 추가 검증:

```bash
cargo build --workspace --all-features
cargo build -p ton618 --no-default-features --features file-dispatch
```

목표는 local validation과 CI가 같은 기준으로 동작하게 만드는 것이다.

### 3. TON618 / Laniakea 서비스 구조 정리

Core 상태 전이가 안정된 뒤에는 `main.rs`에 있는 loop를 service 객체로 분리한다.

TON618 목표:

```rust
DispatchService::run_once()
DispatchService::run()
```

Laniakea 목표:

```rust
Worker::process_one()
Worker::run_file_loop()
```

이렇게 분리하면 scheduler/worker 동작을 단위 테스트로 검증할 수 있다.

### 4. Canopus v1 경로 안정화

새 기능 추가보다 먼저 전체 흐름이 끝까지 안전하게 도는지 확인한다.

검증할 흐름:

```text
Discord !run
→ task Pending 생성
→ TON618 Dispatched
→ Laniakea가 Canopus 호출
→ Canopus submit pipeline
→ artifacts 생성
→ PendingReview
→ Discord !approve
→ Processed
→ finalize/dry-run PR or GitHub Project artifact
```

확인할 것:

- Discord payload의 GitHub metadata가 Canopus까지 전달되는지
- `dry-run-offline`이 HTTP 없이 끝나는지
- live mutation gate 없이는 push/PR/Project mutation이 불가능한지

### 5. GitHub Issue / Project v2 MVP 마무리

Canopus GitHub Project v2 작업은 다음 순서로 마무리한다.

1. dry-run artifact 품질 확인
2. validate-read-only 모드 검증
3. mutate-live gate 조건 검증
4. 실제 live mutation은 별도 승인된 환경에서만 smoke test

원칙:

```text
기본값은 dry-run-offline
GitHub Project는 아직 source of truth가 아니라 sync target
live mutation은 명시적 env gate 필요
```

### 6. 저장소 / 메시징 backend 확장

이 단계는 Core 상태 계약이 확정된 뒤에 진행한다.

후순위:

1. SQLite `TaskTable`
2. Postgres
3. Redis result/cache
4. RabbitMQ durable dispatch/status 연동

지금 SQLite부터 들어가면 아직 흔들리는 상태 전이 규칙을 새 backend에도 복제하게 되므로 순서가 이르다.

### 7. v2 협업 시스템 구현

마지막 큰 단계다.

목표 흐름:

```text
GitHub Issue / Project agenda
→ Discord room / meeting
→ role agents discussion
→ proposal
→ user-confirmed decision
→ action items
→ executable tasks
```

구현 순서:

1. `Agenda`, `Room`, `Meeting`, `Proposal`, `Decision`, `ActionItem` 도메인 모델
2. GitHub Issue/Project를 agenda ledger로 쓰는 adapter
3. Discord meeting thread/room 생성
4. Role Agent message 기록
5. Decision confirmation gate
6. ActionItem → TaskMessage 변환
7. 기존 TON618/Laniakea/Canopus 실행 경로에 연결

## 추천 최종 순서

```text
빌드 복구
→ Core 상태 전이/에러 정리
→ CI 강화
→ TON618/Laniakea service화
→ Canopus v1 E2E dry-run 검증
→ GitHub Project v2 MVP 마무리
→ SQLite/RabbitMQ 등 backend 확장
→ v2 Agenda/Room/Agent 협업 모델 구현
```

## 현재 바로 다음 작업

지금 바로 해야 할 다음 작업은 **빌드 복구 + Core 상태 전이 리팩토링 마무리**다.

이 작업이 끝나야 Canopus, GitHub Project, v2 협업 모델을 안정적으로 올릴 수 있다.

# Stellaris System Direction

## 한 줄 정의

```text
Stellaris는 범용 분산 작업 처리 플랫폼이고,
Canopus는 Stellaris 위에서 동작하는 AI 개발 자동화 앱이다.
```

이 문서는 Hubble, TON618, Laniakea, Dysonsphere, Canopus, Kepler의 책임 경계를 정리하는 기준 문서다.  
세부 구현 문서가 이 기준과 충돌하면, 우선 이 문서의 계층 경계를 따른다.

---

## 1. Stellaris Core

Stellaris Core의 목표는 특정 업무 도메인에 묶이지 않는 분산 작업 처리다.

```text
Producer
→ Task Contract / Queue
→ Scheduler / Dispatcher
→ Worker
→ Result / Status
```

현재 컴포넌트 기준 역할은 다음과 같다.

| Component | Layer | Responsibility |
|---|---|---|
| Hubble | Producer / collector | 외부 입력, 데이터, 신호를 수집해 작업 후보나 원천 데이터를 만든다. |
| Dysonsphere | Shared contract | TaskMessage, TaskStatus, storage/queue abstraction 등 공통 계약을 제공한다. |
| TON618 | Scheduler / dispatcher | Pending task를 골라 worker에게 배분하고 상태 전이를 관리한다. |
| Laniakea | Worker / executor | 배정된 task를 실행하고 결과 상태를 기록한다. |

Stellaris Core는 Canopus 전용 workflow, agent role, artifact schema, git/PR policy에 직접 종속되지 않는다.

---

## 2. Canopus App

Canopus는 Stellaris 위에서 실행되는 첫 번째 고급 workload/app이다.

```text
Canopus = AI 개발 작업 자동화 앱
```

Canopus의 책임은 다음과 같다.

- 개발 요청을 AI agent workflow로 변환한다.
- planning / coding / checking / review stage를 실행한다.
- git, shell, test, GitHub 동작을 ToolGateway policy 뒤에 둔다.
- plan, diff, test result, review 같은 artifact를 저장한다.
- 위험하거나 최종적인 전이는 human approval gate 뒤에 둔다.
- Discord/GitHub/CLI 같은 intake와 notification adapter를 붙인다.

Canopus는 Stellaris Core를 대체하지 않는다. Stellaris의 task contract와 worker 실행 인프라를 사용하는 app 계층이다.

---

## 3. 기본 실행 흐름

사용자 요청 기반 AI 개발 작업은 다음 흐름을 따른다.

```text
Discord / CLI / GitHub에서 개발 요청 입력
→ TaskMessage 등록
→ TON618이 Pending task dispatch
→ Laniakea가 task type에 맞는 handler 실행
→ Canopus CLI/app이 AI workflow 수행
→ artifact / status / audit 기록
→ 사람 승인 또는 반려
→ 승인된 경우 PR 생성, 후속 처리, 완료 상태 전이
```

핵심 경계는 다음과 같다.

```text
Stellaris = 실행 인프라와 작업 상태 전이
Canopus = AI 개발 작업의 의미론과 정책
```

---

## 4. Discovery Sources

자동 발견 컴포넌트는 실행자가 아니라 producer 계층이다.

```text
Discovery Source
→ PendingProposal
→ Human Approval
→ Pending
→ TON618 dispatch
→ Laniakea execution
```

| Source | Intended scope |
|---|---|
| Hubble | 외부 세계 신호: RSS, web, SNS, issue tracker, webhook, news 등 |
| Kepler | 내부 코드베이스 신호: clippy, test failure, coverage gap, code smell, security finding 등 |

중요 원칙:

```text
Discovery source는 발견을 바로 실행 가능한 task로 만들지 않는다.
항상 PendingProposal로 등록하고, 사람이 승격해야 Pending이 된다.
```

Kepler는 코드 분석 목적상 Canopus와 강하게 결합될 수 있으므로, 장기적으로는 `apps/canopus`의 discovery adapter 또는 optional producer로 둘 수 있다. 다만 TaskMessage와 PendingProposal 계약은 Dysonsphere/Stellaris 경계를 따라야 한다.

---

## 5. 책임 경계

### Stellaris Core가 가져야 하는 것

- task queue / task storage abstraction
- TaskMessage / TaskStatus / TaskType 계약
- scheduler / dispatcher
- worker execution model
- 안전한 상태 전이, 예: compare-and-set 기반 transition
- 분산 실행, retry, timeout, observability의 일반 메커니즘

### Canopus App이 가져야 하는 것

- AI 개발 workflow state
- agent role / stage / artifact model
- tool policy
- git branch / PR / check orchestration
- approval semantics
- Discord/GitHub/CLI adapter
- AI runtime adapter

### Discovery Source가 가져야 하는 것

- candidate finding 생성
- dedup ledger
- PendingProposal 등록
- 발견 알림

---

## 6. 피해야 할 안티패턴

- Stellaris Core가 Canopus 전용 stage나 artifact schema를 알아야 하는 구조
- Canopus가 TON618/Laniakea의 scheduler/worker 역할을 대체하는 구조
- Hubble/Kepler가 승인 없이 Pending task를 직접 만들어 실행시키는 구조
- Discord 메시지나 seen ledger가 task source of truth가 되는 구조
- worker가 취소/반려/완료된 상태를 무조건 덮어쓰는 구조
- AI agent가 main/master/develop에 직접 push하거나 approval 없이 PR/merge/deploy하는 구조

---

## 7. 문서 정리 원칙

- `stellaris_summary.md`는 Stellaris Core 관점의 요약을 유지한다.
- `stellaris-canopus-architecture-v1.md`는 Canopus v1 app/workload 설계로 해석한다.
- `stellaris-canopus-architecture.md`의 “Canopus kernel” 표현은 장기 제품 비전을 설명할 때만 사용하고, Stellaris Core를 대체한다는 의미로 쓰지 않는다.
- Canopus 관련 구현은 가능하면 `apps/canopus` 아래 app 계층으로 유지한다.

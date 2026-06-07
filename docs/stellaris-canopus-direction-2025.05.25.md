# Stellaris / Canopus 방향성

이 문서는 `Stellaris` 저장소의 앞으로의 큰 방향을 정리한 문서다.  
목표는 새 구조로 갈아엎는 것이 아니라, 이미 있는 `Europa`, `Canopus`, `Dysonsphere` 구조를 살려서 Discord 기반 AI 개발 작업 흐름을 완성하는 것이다.

## 1. 목표

Stellaris / Canopus의 목표는 단순한 Discord 챗봇이 아니다.

최종적으로는 Discord에서 자연어로 개발 작업을 지시하면, 시스템이 작업을 분류하고, 필요한 경우 AI coding worker를 실행해서 코드 수정, PR 생성, CI 추적, Discord 상태 알림까지 이어지게 하는 것이다.

```text
Discord 지시
→ Stellaris / Europa가 작업 접수
→ 작업 의도 분류
→ Canopus가 실행
→ 분석 작업이면 Discord에 답변
→ 코드 수정이면 branch / PR 생성
→ GitHub Actions 실행
→ 결과를 Discord에 알림
→ 사람이 최종 리뷰 / merge
```

## 2. 역할 분리

```text
Europa
= Discord UI / control surface
= 사용자의 명령을 받고 상태를 보여준다.

Canopus
= AI development execution engine
= repo 준비, coding agent 실행, diff/test/commit/PR 생성을 담당한다.

Stellaris
= 전체 control plane
= 작업 상태, 정책, 흐름, GitHub/Discord 연동을 관리한다.

GitHub
= branch, PR, CI, review의 source of truth
= 실제 코드 변경 검증은 GitHub에서 이루어진다.

Discord
= 사람이 작업을 지시하고 진행상황을 보는 operator interface
```

중요한 원칙은 **Europa가 실행 정책을 직접 소유하지 않고, Canopus가 Discord UI를 직접 소유하지 않는 것**이다.

## 3. 사용자 경험 방향

사용자는 read-only 작업인지 code-change 작업인지 매번 직접 고를 필요가 없다.

예를 들어 사용자는 그냥 이렇게 말하면 된다.

```text
로그인 기능 추가해줘
로그인 로직 어디 있는지 분석해줘
PR #42 리뷰해줘
CI 실패 고쳐줘
```

시스템 내부에서 이를 대략 다음 intent로 분류한다.

```text
ReadOnlyAnalysis
CodeChange
PrReview
CiRepair
NeedsClarification
```

예시:

```text
"로그인 기능 추가해줘" → CodeChange
"로그인 로직 어디 있어?" → ReadOnlyAnalysis
"PR #42 리뷰해줘" → PrReview
"CI 실패 고쳐줘" → CiRepair
```

초기에는 LLM classifier가 아니라 간단한 rule-based router로 충분하다.

## 4. 핵심 방향

앞으로의 핵심은 새로운 agent framework를 붙이는 것이 아니라, 이미 있는 Europa / Canopus 구조로 다음 루프를 닫는 것이다.

```text
Discord
→ intent router
→ Canopus workflow
→ branch / PR
→ GitHub Actions
→ GitHub status backflow
→ Discord progress update
```

이 루프가 완성되면 Discord가 단순 명령창이 아니라 개발 작업 관제 UI가 된다.

## 5. 다음 우선순위

### 1. 현재 구조 파악

먼저 실제 repo 기준으로 현재 구현 상태를 확인한다.

```text
apps/europa
apps/canopus
dysonsphere
docs
.github/workflows
AGENTS.md
```

확인할 것:

```text
- 현재 Discord command 흐름
- 현재 Canopus 실행 흐름
- 현재 job/task 상태 모델
- 현재 GitHub/PR 관련 코드
- 현재 CI workflow
```

### 2. Intent router 정리

자연어 instruction을 내부 실행 모드로 분류하는 작은 router를 만든다.

초기 목표:

```text
ReadOnlyAnalysis
CodeChange
PrReview
CiRepair
NeedsClarification
```

기존 `!ask`, `!analyze`, `!brainstorm`, `!run` 같은 명령은 당장 제거하지 않는다.  
대신 이 명령들 아래에서 재사용 가능한 분류 계층을 만든다.

### 3. Job lifecycle 확장

작업 상태가 PR/CI 흐름까지 표현할 수 있어야 한다.

필요한 상태 개념:

```text
created
running
diff_ready
pr_created
ci_running
ci_passed
ci_failed
awaiting_review
done
failed
cancelled
```

상태 이름은 기존 구현과 맞춰도 된다. 중요한 것은 작업이 지금 어디까지 진행됐는지 Discord에서 알 수 있어야 한다는 점이다.

### 4. Code-change delivery path 표준화

코드 수정 작업은 항상 안전한 흐름을 따른다.

```text
branch 생성
→ AI coding worker 실행
→ local check 실행
→ commit 생성
→ PR 생성
→ CI 실행
→ Discord 알림
→ 사람이 review / merge
```

AI가 main/master에 직접 push하거나 merge하면 안 된다.

### 5. GitHub backflow 추가

PR을 만든 뒤 GitHub Actions 결과가 다시 Stellaris로 돌아와야 한다.

목표:

```text
PR 생성
→ CI running
→ CI passed / failed
→ job 상태 업데이트
→ Discord 알림
```

이를 위해 GitHub webhook 또는 check/workflow 상태를 job과 매핑하는 흐름이 필요하다.

### 6. Discord progress UX 개선

작업 하나마다 Discord에서 진행상황이 보여야 한다.

예시:

```text
Job created: job_123
Intent: CodeChange
Status: running

Branch created: agent/job_123-login

PR created: #42
CI: running

CI passed
Status: awaiting review
```

가능하면 job 하나당 Discord thread 하나를 사용하는 방향이 좋다.

## 6. 안전 원칙

이 프로젝트는 AI가 repository를 다루므로 안전장치를 유지해야 한다.

금지:

```text
- main/master 직접 push
- force push
- 자동 merge
- 자동 production deploy
- secret 출력
- .env 파일 commit
- 승인 없는 destructive command 실행
```

기본 방향:

```text
- local-first
- dry-run 우선
- 위험 작업은 명시적 gate 필요
- PR 이후 human review 필수
```

## 7. 지금 하지 않을 것

현재 우선순위가 아닌 것:

```text
- Hermes Agent를 core orchestrator로 도입
- multi-agent debate 구조
- autonomous merge
- autonomous deploy
- 복잡한 web dashboard
- 전체 architecture rewrite
- 새 agent framework로 교체
```

지금은 새로운 프레임워크를 붙이는 단계가 아니라, 기존 Europa / Canopus 구조를 이용해 end-to-end 개발 작업 루프를 완성하는 단계다.

## 8. 요약

Stellaris / Canopus의 방향은 다음과 같다.

```text
Discord에서 자연어로 개발 작업을 지시하면,
Stellaris가 작업을 관리하고,
Canopus가 안전한 AI coding workflow를 실행하며,
GitHub PR/CI 결과가 다시 Discord로 돌아오는
human-in-the-loop AI development orchestration system.
```

가까운 다음 목표는 다음 순서다.

```text
현재 구조 파악
→ intent router
→ job lifecycle
→ PR delivery
→ GitHub Actions backflow
→ Discord progress notification
```

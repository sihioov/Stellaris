# AI 개발 오케스트레이션 파이프라인 요약

## 1. 한 줄 요약

디스코드, 웹 UI, CLI, GitHub/GitLab Issue 등에서 작업 아젠다를 입력하면, 내부 **Orchestrator AI**가 작업을 해석하고 여러 **Worker AI**에게 분배하여 코드 수정, 테스트, 리뷰, PR 생성을 자동으로 수행한다. 최종 merge와 배포는 사용자가 승인한다.

---

## 2. 전체 구조

```text
[Discord / Web UI / CLI / Issue]
        ↓
[Agenda Intake Layer]
        ↓
[Orchestrator AI]
        ↓
[Planner AI]
        ↓
[Context / RAG / Repo Index]
        ↓
[Worker AI Agents]
   ├─ Coder Agent
   ├─ Test Agent
   ├─ Reviewer Agent
   ├─ Security Agent
   ├─ Docs Agent
   └─ DevOps Agent
        ↓
[Tool Gateway]
   ├─ git
   ├─ shell sandbox
   ├─ test runner
   ├─ linter
   ├─ package manager
   └─ GitHub/GitLab API
        ↓
[Branch 생성 → 코드 수정 → 테스트 → PR 생성]
        ↓
[AI Review Summary]
        ↓
[Human Review / Approval]
        ↓
[Merge / Reject / Rework]
```

---

## 3. 핵심 플로우

### 3.1 아젠다 입력

사용자는 디스코드나 웹 UI에서 작업을 입력한다.

예시:

```text
로그인 API에 refresh token rotation 기능 추가해줘.
테스트 작성하고 PR까지만 만들어줘.
내가 리뷰하기 전에는 merge하지 마.
```

---

### 3.2 Agenda Intake Layer

입력된 자연어 요청을 내부 작업 단위로 변환한다.

역할:

- 요청 파싱
- 프로젝트/레포 식별
- 작업 유형 분류
- 우선순위 설정
- 위험도 판단
- 승인 필요 여부 결정
- 작업 티켓 생성

예시 내부 데이터:

```json
{
  "task_id": "TASK-2026-0424-001",
  "source": "discord",
  "project": "backend-api",
  "repo": "company/backend-api",
  "task_type": "feature",
  "title": "Add refresh token rotation",
  "approval_required": true,
  "risk_level": "medium"
}
```

---

### 3.3 Orchestrator AI

전체 작업을 관리하는 중앙 제어 AI다.

역할:

- 작업 의도 해석
- 필요한 Agent 선택
- 작업 순서 결정
- Context 수집 요청
- Worker Agent에게 작업 분배
- 결과 검증
- 실패 시 재시도 또는 중단
- PR 생성 지시
- 사용자에게 상태 보고

---

### 3.4 Planner AI

코드를 수정하기 전에 실행 계획을 만든다.

예시:

```text
1. 인증 관련 코드 구조 분석
2. refresh token 발급 위치 확인
3. token 저장소 확인
4. rotation 정책 설계
5. 코드 수정
6. 테스트 추가
7. lint/test 실행
8. PR 생성
9. 변경 요약 작성
```

---

### 3.5 Context / RAG / Repo Index

Worker AI가 작업하기 전에 필요한 정보를 검색한다.

수집 대상:

- 레포 파일 구조
- 코드 심볼 인덱스
- README
- API 문서
- ADR
- 기존 테스트
- Git history
- 기존 PR/Issue
- 코딩 컨벤션
- dependency 정보

---

## 4. Worker AI Agents

### Coder Agent

코드 수정 담당.

- 코드 읽기
- 변경 지점 찾기
- patch 작성
- 파일 수정
- 타입 에러 수정

### Test Agent

테스트 담당.

- 기존 테스트 확인
- unit/integration test 작성
- 테스트 실행
- 실패 원인 분석

### Reviewer Agent

AI 코드 리뷰 담당.

- 요구사항 충족 여부 확인
- 불필요한 변경 감지
- edge case 확인
- regression risk 확인
- PR 리뷰 요약 작성

### Security Agent

보안 검토 담당.

- 인증/인가 취약점 확인
- secret 노출 확인
- injection 위험 확인
- dependency vulnerability 확인
- 권한 상승 가능성 확인

### Docs Agent

문서화 담당.

- README 수정
- API 문서 업데이트
- changelog 작성
- PR description 작성

### DevOps Agent

인프라/CI 담당.

- CI 실패 분석
- Dockerfile 확인
- 배포 설정 확인
- 환경변수 문서화

---

## 5. Tool Gateway

AI Agent가 직접 시스템 권한을 쓰지 않고, Tool Gateway를 통해 제한된 작업만 수행한다.

허용 도구 예시:

- `git status`
- `git diff`
- `git checkout -b`
- `git commit`
- `git push`
- 파일 읽기/쓰기
- 테스트 실행
- linter 실행
- PR 생성

정책 체크 예시:

- 허용된 레포인가?
- protected branch에 직접 push하려는가?
- secret 파일을 수정하려는가?
- destructive command인가?
- 사용자 승인이 필요한 작업인가?

---

## 6. Git / PR 자동화

작업 시작 시 새 브랜치를 만든다.

```text
main
  ↓
ai/TASK-2026-0424-001-refresh-token-rotation
```

자동 수행 순서:

```text
1. branch 생성
2. 코드 수정
3. 테스트 추가
4. lint 실행
5. test 실행
6. git diff 확인
7. commit 생성
8. remote push
9. PR 생성
10. AI review summary 작성
```

PR 본문 예시:

```markdown
## Summary
- Added refresh token rotation
- Added token reuse detection
- Added unit and integration tests

## Validation
- npm test
- npm run lint

## Risk
- Medium: authentication flow changed

## Review Notes
- Please verify refresh token invalidation behavior
- Please review token storage policy
```

---

## 7. Human-in-the-loop 승인 구조

AI가 작업을 수행하되, 최종 결정권은 사용자에게 둔다.

권장 승인 단계:

```text
1. 아젠다 접수
2. 작업 계획 승인
3. 코드 수정
4. PR 생성
5. 사용자 리뷰
6. merge 승인
7. 배포 승인
```

필수 원칙:

- AI가 main/master에 직접 push 금지
- AI가 자동 merge 금지
- AI가 production deploy 금지
- destructive command는 사용자 승인 필수
- secret/env 파일 수정은 사용자 승인 필수

---

## 8. 상태 머신

```text
CREATED
  ↓
PLANNING
  ↓
PLAN_REVIEW_REQUIRED
  ↓
APPROVED
  ↓
EXECUTING
  ↓
TESTING
  ↓
AI_REVIEWING
  ↓
PR_CREATED
  ↓
HUMAN_REVIEW_REQUIRED
  ↓
MERGED / REWORK_REQUESTED / REJECTED
```

재작업이 필요한 경우:

```text
HUMAN_REVIEW_REQUIRED
  ↓
REWORK_REQUESTED
  ↓
PLANNING or EXECUTING
  ↓
PR_UPDATED
```

---

## 9. 저장해야 할 데이터

### tasks

- task_id
- title
- source
- repo
- status
- risk_level
- created_by
- created_at
- branch
- pr_url

### agent_runs

- task_id
- agent_name
- input
- output
- tool_calls
- status

### tool_logs

- task_id
- tool_name
- command
- result
- exit_code

### approvals

- task_id
- approval_type
- approved_by
- approved_at
- decision

### artifacts

- plan
- diff_summary
- test_result
- review_summary

---

## 10. MVP 범위

처음에는 작게 시작한다.

```text
1. Discord에서 작업 입력
2. Orchestrator가 작업 티켓 생성
3. Repo context 검색
4. Planner가 작업 계획 생성
5. Coder Agent가 새 branch에서 코드 수정
6. Test Agent가 테스트 실행
7. Reviewer Agent가 diff 리뷰
8. GitHub/GitLab PR 생성
9. Discord로 PR 요약 보고
10. 사용자가 직접 리뷰/merge
```

초기 Agent 구성:

```text
- Orchestrator
- Coder Agent
- Reviewer Agent
```

추후 확장:

```text
- Test Agent
- Security Agent
- Docs Agent
- DevOps Agent
```

---

## 11. 핵심 원칙

```text
AI는 작업을 수행한다.
하지만 권한은 제한한다.
변경은 PR로만 만든다.
최종 merge와 배포는 사람이 승인한다.
```

이 시스템의 목표는 완전 무통제 자동개발이 아니라, **AI가 개발 업무를 수행하고 사용자가 리뷰/승인하는 안전한 개발 자동화 파이프라인**이다.

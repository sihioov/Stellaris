# Stellaris / Canopus: Discord 기반 자율 Multi-Agent 개발 오케스트레이션 아키텍처

## 1. 문서 목적

이 문서는 `Stellaris / Canopus`가 지향하는 **Discord 기반 자율 AI Agent 개발 관리 시스템**의 아키텍처를 정리한다.

본 시스템은 단순한 AI 코딩 도구나 이슈 보드가 아니다. 사용자는 Discord를 통해 명령을 내리고, 승인하거나 반려하며, 여러 AI Agent는 GitHub Project와 GitHub Repository를 지속적으로 감시하면서 작업을 수행하고, 필요한 경우 사용자에게 의사결정을 요청한다.

핵심 목표는 다음과 같다.

```text
Discord에서 사용자 명령 입력
→ Canopus가 workflow 생성
→ GitHub Project / Issue / PR 상태 감시
→ Agent들이 역할별로 검토·대화·작업
→ 필요 시 Discord로 승인 요청
→ 사용자가 Discord에서 승인/반려
→ Agent가 GitHub branch / commit / PR 작업 수행
→ 리뷰 통과 후 merge
→ GitHub Project 상태 업데이트
→ Discord로 전체 과정 알림
```

---

## 2. 시스템 핵심 포지션

Canopus는 이슈트래커도 아니고, 단순 Discord Bot도 아니다.

Canopus의 정체성은 다음과 같다.

```text
Canopus = Discord-native, GitHub Project-aware, multi-agent orchestration kernel
```

즉:

```text
Discord = 명령 / 승인 / 사용자 개입 / 알림 채널
GitHub Project = 공식 작업 보드 / 작업 원장 / 사용자-facing source of truth
GitHub Repository = 코드 변경 / branch / PR / CI / merge 대상
Canopus = Agent workflow / 대화 / 상태 / 승인 / 실행 중재 엔진
```

---

## 3. 최상위 아키텍처

```text
┌──────────────────────────────────────────────────────────────┐
│                         Discord                              │
│  - 명령 입력                                                  │
│  - 승인 / 반려 / 변경 요청                                    │
│  - Agent 대화 transcript                                      │
│  - 작업 상태 알림                                             │
└─────────────────────────────┬────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                    Discord Control Adapter                    │
│  - Slash command 수신                                         │
│  - Button / Modal interaction 처리                            │
│  - 승인 이벤트 변환                                           │
│  - Discord notification 발행                                  │
└─────────────────────────────┬────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                         Canopus Core                         │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                  Workflow Orchestrator                  │  │
│  │  - workflow 생성 / 상태 전이                            │  │
│  │  - agent 배정                                           │  │
│  │  - approval gate 제어                                   │  │
│  │  - loop / retry / timeout 제어                          │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                    Agent Message Bus                    │  │
│  │  - agent 간 structured message 전달                     │  │
│  │  - turn-based conversation 제어                         │  │
│  │  - conversation transcript 저장                         │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                     Agent Runtime                       │  │
│  │  - Planner / Architect / Coder / QA / Reviewer          │  │
│  │  - Security / Docs / DevOps Agent                       │  │
│  │  - 자율 review loop                                     │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                    Tool Gateway                         │  │
│  │  - git / shell / test runner / GitHub API 실행 중재      │  │
│  │  - policy check                                         │  │
│  │  - 위험 작업 승인 요구                                  │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────┬────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                    GitHub Integration Layer                   │
│  - GitHub Project 감시                                       │
│  - GitHub Issue 생성 / 수정                                  │
│  - Project item field update                                 │
│  - Branch / Commit / Push / PR 생성                          │
│  - CI status 확인                                            │
│  - Merge 수행                                                │
└─────────────────────────────┬────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                       GitHub Platform                         │
│  - GitHub Project                                             │
│  - GitHub Issues                                              │
│  - GitHub Pull Requests                                       │
│  - GitHub Actions / Checks                                    │
│  - Branch Protection                                          │
└──────────────────────────────────────────────────────────────┘
```

---

## 4. 역할 분리

## 4.1 Discord

Discord는 단순 notification 채널이 아니라, 사용자의 주요 control plane이다.

Discord의 역할:

```text
- 사용자의 자연어 명령 수신
- slash command 기반 명령 수신
- 승인 / 반려 / 변경 요청 처리
- 작업 진행 상황 알림
- Agent 간 대화 transcript 출력
- GitHub Project / PR 상태 알림
- 긴급 중단 / 재개 명령 처리
```

예시 명령:

```text
/canopus start "결제 실패 retry 로직 개선해"
/canopus status WF-2026-001
/canopus approve WF-2026-001
/canopus reject WF-2026-001 reason:"테스트 부족"
/canopus pause WF-2026-001
/canopus resume WF-2026-001
/canopus review github-project-item:123
```

---

## 4.2 GitHub Project

GitHub Project는 공식 작업 보드이자 사용자-facing source of truth다.

GitHub Project의 역할:

```text
- 공식 작업 상태 관리
- Issue / PR / Project item 연결
- 우선순위 / 담당 영역 / 상태 필드 관리
- 사용자와 agent가 함께 보는 작업 원장
- Canopus Agent가 지속적으로 감시하는 대상
```

GitHub Project 상태 예시:

```text
Backlog
Ready for Agent
Agent Reviewing
Agent Working
Waiting Human Approval
Changes Requested
Ready to Merge
Done
Blocked
Failed
```

---

## 4.3 GitHub Repository

GitHub Repository는 실제 코드 작업 공간이다.

역할:

```text
- branch 생성
- commit / push
- pull request 생성
- CI 실행
- code review
- branch protection 적용
- merge 수행
```

중요 원칙:

```text
- Agent는 main branch에 직접 push하지 않는다.
- Agent는 feature branch 또는 worktree에서만 작업한다.
- merge는 approval gate와 GitHub branch protection을 모두 통과해야 한다.
- production deploy는 별도 승인 gate 뒤에 둔다.
```

---

## 4.4 Canopus

Canopus는 전체 시스템의 내부 두뇌다.

Canopus의 역할:

```text
- Discord 명령을 workflow로 변환
- GitHub Project item을 감시하고 workflow로 연결
- Agent 간 대화 및 turn 관리
- Agent task 할당
- Tool execution 중재
- Approval gate 생성 및 처리
- GitHub 상태 업데이트
- Discord transcript / notification 발행
- 자율 review loop 관리
- 감사 로그 저장
```

---

## 5. Canopus 내부 모듈

## 5.1 Command Intake

사용자 명령과 외부 이벤트를 내부 command로 변환한다.

입력 소스:

```text
- Discord slash command
- Discord button interaction
- Discord modal submission
- GitHub Project item change
- GitHub Issue event
- GitHub PR event
- GitHub check / CI event
- Scheduled review tick
```

내부 command 예시:

```json
{
  "command_id": "cmd_2026_001",
  "source": "discord",
  "actor": "sihioov",
  "intent": "start_workflow",
  "instruction": "결제 실패 retry 로직 개선",
  "github_project_id": "PVT_xxx",
  "requires_approval": true
}
```

---

## 5.2 Workflow Orchestrator

Workflow Orchestrator는 상태머신을 담당한다.

주요 책임:

```text
- workflow 생성
- workflow 상태 전이
- agent task 생성
- agent turn 결정
- approval gate 생성
- tool request 허용 여부 판단
- 실패 / 재시도 / 중단 / 재개 처리
- GitHub Project 상태 projection
```

Canopus 내부 workflow 상태 예시:

```text
OBSERVED
WORKFLOW_CREATED
PLANNING
PLAN_REVIEW
WAITING_PLAN_APPROVAL
AGENT_ASSIGNED
AGENT_RUNNING
TOOL_EXECUTING
TESTING
AGENT_REVIEW
NEEDS_HUMAN_DECISION
WAITING_DISCORD_APPROVAL
APPROVED
CHANGES_REQUESTED
PR_CREATED
WAITING_HUMAN_REVIEW
MERGE_READY
WAITING_MERGE_APPROVAL
MERGED
COMPLETED
FAILED
CANCELLED
```

---

## 5.3 Agent Message Bus

Agent 간 대화는 Discord에서 직접 일어나는 것이 아니라 내부 `Agent Message Bus`에서 일어난다.

Discord는 이 내부 대화의 projection/transcript를 보여주는 역할이다.

기본 메시지 구조:

```json
{
  "message_id": "msg_001",
  "workflow_id": "wf_001",
  "conversation_id": "conv_001",
  "task_id": "task_001",
  "round": 3,
  "sender": "ArchitectAgent",
  "receiver": "BackendAgent",
  "type": "task_request",
  "intent": "implement",
  "content": "retry policy는 idempotency key 기준으로 구현하세요.",
  "requires_response": true,
  "requires_approval": false,
  "created_at": "2026-04-28T10:00:00Z"
}
```

메시지 타입:

```text
user_command
task_assign
proposal
critique
task_request
task_result
tool_request
tool_result
approval_request
approval_decision
status_update
observation
finding
agenda_proposal
risk_assessment
priority_vote
agenda_review
error
final_summary
```

---

## 5.4 Agent Runtime

Agent Runtime은 역할별 agent를 실행한다.

기본 Agent 구성:

```text
Orchestrator Agent
Planner Agent
Architect Agent
Backend Agent
Frontend Agent
QA Agent
Reviewer Agent
Security Agent
Docs Agent
DevOps Agent
```

각 Agent는 다음 공통 인터페이스를 가진다.

```text
input:
- AgentMessage
- Workflow context
- GitHub Project context
- Repository context
- Decision memory
- Tool result

output:
- AgentMessage
- ToolRequest
- AgendaProposal
- ApprovalRequest
- FinalSummary
```

---

## 5.5 Tool Gateway

Tool Gateway는 Agent가 외부 도구를 직접 실행하지 못하게 막는 중재 계층이다.

Agent는 직접 `git push`, `merge`, `shell command`를 실행하지 않고 `tool_request`를 생성한다.

Tool Gateway 책임:

```text
- tool_request 검증
- policy rule 확인
- approval 필요 여부 판단
- 허용된 명령만 실행
- 실행 결과 저장
- 실패 결과를 agent message bus로 반환
```

Tool 분류:

```text
Safe:
- git status
- git diff
- test read-only check
- issue read
- PR read

Controlled:
- branch 생성
- file modify
- unit test 실행
- draft PR 생성

Approval Required:
- dependency 추가
- DB migration
- force push
- production config 변경
- merge
- deploy
- secret 관련 작업
```

---

## 5.6 GitHub Project Watcher

GitHub Project Watcher는 GitHub Project와 Issue/PR 상태를 지속 감시한다.

감시 대상:

```text
- Ready for Agent 상태의 Project item
- Waiting Human Approval 상태의 item
- stale issue
- blocked issue
- failed CI
- newly opened PR
- merged PR
- issue comment
- PR review comment
```

감시 방식:

```text
- GitHub webhook
- 주기적 polling / sync
- Project field snapshot 비교
- PR / check status polling
```

Watcher가 생성하는 내부 이벤트:

```text
github_project_item_observed
github_issue_updated
github_pr_opened
github_pr_reviewed
github_check_failed
github_check_passed
github_project_status_changed
stale_item_detected
```

---

## 5.7 Autonomous Review Loop

각 Agent는 자기 역할에 따라 주기적 또는 이벤트 기반 review loop를 가진다.

목표:

```text
- 사용자가 직접 지시하지 않아도 문제를 발견
- 발견 사항을 agenda proposal로 생성
- 다른 Agent의 검토를 거침
- 사용자 승인 후 정식 workflow로 전환
```

Review trigger:

```text
- scheduled daily review
- GitHub Project item 변경
- PR opened
- PR merged
- CI failed
- issue stale
- dependency update
- user requested review
```

Agent별 review 예시:

```text
QA Agent:
- 테스트 커버리지 부족 탐지
- flaky test 감지
- CI 실패 분석

Security Agent:
- secret leakage 감지
- auth/permission 변경 감시
- dependency risk 확인

Reviewer Agent:
- 최근 merge 코드 품질 검토
- technical debt 후보 탐지

Docs Agent:
- 문서와 실제 코드 불일치 감지
- changelog 후보 작성

Backend Agent:
- API contract 불일치
- DB migration 위험
- retry/idempotency 정책 위반 감지
```

Agenda proposal 예시:

```json
{
  "proposal_id": "ap_2026_001",
  "type": "agenda_proposal",
  "sender": "QAAgent",
  "title": "결제 실패 retry integration test 부족",
  "reason": "최근 retry policy 변경 이후 실패 케이스가 unit test에만 존재합니다.",
  "suggested_tasks": [
    "retry 실패 integration test 추가",
    "timeout edge case 추가"
  ],
  "impact": "high",
  "risk": "medium",
  "confidence": 0.82,
  "requires_user_approval": true
}
```

---

## 6. 주요 Workflow

## 6.1 Discord 명령 기반 작업 시작

```text
1. 사용자가 Discord에서 명령 입력
   /canopus start "결제 실패 retry 로직 개선"

2. Discord Adapter가 interaction 수신

3. Canopus가 command 생성

4. Workflow Orchestrator가 workflow 생성

5. Planner Agent에게 planning task 할당

6. Planner Agent가 작업 분해

7. Architect / Reviewer Agent가 계획 검토

8. 필요하면 Discord로 계획 승인 요청

9. 승인되면 GitHub Issue / Project item 생성 또는 연결

10. Coder Agent에게 구현 task 할당

11. Tool Gateway가 branch/worktree 생성

12. Agent가 코드 수정

13. 테스트 실행

14. Reviewer / QA Agent 검토

15. PR 생성

16. Discord로 사용자 review 요청

17. 사용자가 승인

18. Canopus가 merge 조건 확인

19. merge 수행

20. GitHub Project 상태 Done 업데이트

21. Discord에 완료 알림
```

---

## 6.2 GitHub Project 감시 기반 작업 시작

```text
1. 사용자가 GitHub Project에 Issue 추가

2. 상태를 Ready for Agent로 변경

3. GitHub Project Watcher가 item 감지

4. Canopus가 OBSERVED event 생성

5. Orchestrator가 workflow 생성

6. Agent가 issue context 검토

7. 필요한 경우 Discord에 작업 시작 확인 요청

8. 승인 또는 policy 통과 후 Agent 작업 시작

9. branch / PR / CI / review 진행

10. GitHub Project 상태 업데이트

11. Discord로 상태 알림
```

---

## 6.3 Agent 자율 아젠다 생성

```text
1. Scheduled review tick 발생

2. QA / Security / Reviewer Agent가 GitHub Project와 PR 상태 검토

3. Agent가 observation 생성

4. 의미 있는 observation을 finding으로 승격

5. finding이 기준을 넘으면 agenda_proposal 생성

6. 관련 Agent들이 proposal 검토

7. Orchestrator가 priority score 계산

8. Discord에 새 아젠다 후보 알림

9. 사용자가 승인하면 GitHub Issue / Project item 생성

10. 정식 workflow로 전환
```

---

## 6.4 사용자 승인 기반 Merge

```text
1. Agent 작업 완료

2. PR 생성

3. CI 통과

4. Reviewer / QA / Security Agent 검토 완료

5. Workflow 상태 WAITING_MERGE_APPROVAL

6. Discord에 merge 승인 요청

7. 사용자가 Approve 버튼 클릭

8. Canopus가 approval_decision event 저장

9. GitHub branch protection / check 상태 확인

10. merge 수행

11. Project item Done 업데이트

12. Discord에 merge 완료 알림
```

---

## 7. Approval Architecture

Approval은 Canopus의 핵심 안전장치다.

Approval 타입:

```text
PLAN_APPROVAL
SCOPE_CHANGE_APPROVAL
RISKY_TOOL_APPROVAL
DEPENDENCY_APPROVAL
PR_REVIEW_APPROVAL
MERGE_APPROVAL
DEPLOY_APPROVAL
```

Approval request 예시:

```json
{
  "approval_id": "appr_001",
  "workflow_id": "wf_001",
  "type": "MERGE_APPROVAL",
  "requested_by": "Orchestrator",
  "summary": "PR #78 merge 승인이 필요합니다.",
  "status": "PENDING",
  "allowed_actors": ["sihioov"],
  "created_at": "2026-04-28T10:30:00Z"
}
```

Discord 출력 예시:

```text
[WF-2026-001 / Issue #45] Merge 승인이 필요합니다.

PR: #78
상태:
- CI: passed
- ReviewerAgent: passed
- QAAgent: passed
- SecurityAgent: no critical issue
- GitHub Project: Waiting Human Approval

변경 요약:
- retry policy 추가
- integration test 추가
- error handling 보강

[Approve Merge] [Request Changes] [Reject]
```

Approval 처리 원칙:

```text
- 모든 approval은 DB/event log에 저장한다.
- Discord 버튼 클릭은 곧바로 merge가 아니라 approval_decision event를 생성한다.
- Orchestrator가 승인자의 권한을 확인한다.
- 이미 처리된 approval은 중복 처리하지 않는다.
- 승인 후에도 GitHub branch protection과 CI 상태를 재확인한다.
- merge/deploy는 반드시 policy gate 뒤에서만 실행한다.
```

---

## 8. Agent Conversation Architecture

Agent 간 대화는 자유 채팅이 아니라 제어된 conversation이다.

핵심 원칙:

```text
- 모든 대화에는 workflow_id가 있다.
- 모든 대화에는 conversation_id가 있다.
- 각 메시지는 sender / receiver / type / intent를 가진다.
- Agent는 자신에게 전달된 메시지 또는 할당 task에만 응답한다.
- Orchestrator가 다음 발화자를 결정한다.
- 대화는 max_rounds 제한을 가진다.
- approval 상태에서는 실행성 tool request가 중단된다.
```

Turn 예시:

```text
Round 1: Planner → 작업 계획 제안
Round 2: Architect → 설계 검토
Round 3: Planner → 계획 수정
Round 4: Orchestrator → task 생성
Round 5: BackendAgent → 구현 결과
Round 6: QAAgent → 테스트 결과
Round 7: ReviewerAgent → 리뷰 결과
Round 8: Orchestrator → 사용자 승인 요청
```

Loop 방지 설정:

```json
{
  "max_rounds": 8,
  "max_messages_per_agent": 5,
  "max_tool_requests": 20,
  "max_retries": 2,
  "max_runtime_minutes": 30,
  "human_required_after_failures": 2
}
```

---

## 9. Data Architecture

Canopus는 Discord 메시지나 GitHub Project만을 상태 저장소로 사용하지 않는다.

GitHub Project는 사용자-facing source of truth이고, Canopus DB는 내부 실행 source of truth다.

권장 저장 모델:

```text
workflows
tasks
agent_messages
conversations
tool_requests
tool_results
approval_requests
approval_decisions
agenda_proposals
findings
github_project_mappings
github_issue_mappings
github_pr_mappings
artifacts
decision_memory
audit_events
```

---

## 9.1 workflows

```text
id
source
status
github_project_id
github_project_item_id
github_issue_id
github_pr_id
created_by
created_at
updated_at
completed_at
```

---

## 9.2 tasks

```text
id
workflow_id
agent_role
status
input_summary
output_summary
claimed_by
created_at
started_at
completed_at
```

---

## 9.3 agent_messages

```text
id
workflow_id
conversation_id
task_id
round
sender
receiver
type
intent
content
requires_response
requires_approval
created_at
```

---

## 9.4 approval_requests

```text
id
workflow_id
type
status
summary
allowed_actors
discord_message_id
github_issue_id
github_pr_id
created_at
resolved_at
```

---

## 9.5 github_mappings

```text
id
workflow_id
github_project_id
github_project_item_id
github_issue_number
github_issue_node_id
github_pr_number
github_pr_node_id
status_field_id
last_synced_at
```

---

## 9.6 decision_memory

Decision Memory는 프로젝트에서 반복적으로 적용해야 하는 판단과 규칙을 저장한다.

예시:

```text
이 프로젝트에서는 retry policy 구현 시 idempotency key를 기준으로 한다.
DB migration은 SecurityAgent와 ReviewerAgent 검토 후 진행한다.
payment-service의 timeout 기본값은 5초다.
```

필드:

```text
id
scope
title
decision
reason
source_workflow_id
source_issue_id
applies_to
created_at
updated_at
```

---

## 10. Discord Channel Architecture

권장 Discord 채널 구조:

```text
#canopus-control
- 사용자 명령
- slash command 중심
- 중요한 수동 명령

#canopus-approvals
- 승인 요청
- Approve / Reject / Request Changes 버튼

#canopus-agent-room
- Agent 간 대화 transcript
- planning / review / task discussion

#canopus-github
- GitHub Project / Issue / PR 상태 알림

#canopus-ci
- CI 실패 / 테스트 결과 / 분석 알림

#canopus-audit
- 상태 전이 / tool execution / approval log

#canopus-alerts
- 실패 / timeout / blocked / 긴급 알림
```

Discord 출력 원칙:

```text
- 모든 메시지는 workflow_id를 포함한다.
- GitHub Issue / PR 링크를 포함한다.
- Agent 발화는 역할 이름으로 구분한다.
- 너무 긴 내부 reasoning은 요약해서 출력한다.
- 승인 요청은 별도 채널에 출력한다.
- audit 성격의 이벤트는 별도 채널에 출력한다.
```

---

## 11. GitHub Project Integration

GitHub Project는 Canopus의 공식 외부 작업 보드다.

Project field 예시:

```text
Status
Priority
Area
Agent Owner
Risk
Approval Required
Canopus Workflow ID
Last Agent Update
```

Status mapping 예시:

```text
Canopus PLANNING
→ GitHub Project: Agent Reviewing

Canopus AGENT_RUNNING
→ GitHub Project: Agent Working

Canopus WAITING_DISCORD_APPROVAL
→ GitHub Project: Waiting Human Approval

Canopus CHANGES_REQUESTED
→ GitHub Project: Changes Requested

Canopus MERGE_READY
→ GitHub Project: Ready to Merge

Canopus COMPLETED
→ GitHub Project: Done
```

GitHub Project 감시 원칙:

```text
- Canopus는 Project item 변경을 감지한다.
- Canopus는 Ready for Agent 상태를 작업 후보로 본다.
- Canopus는 Waiting Human Approval 상태를 Discord approval과 연결한다.
- GitHub Project 상태와 Canopus 내부 상태가 다를 경우 reconciliation을 수행한다.
```

---

## 12. Security Architecture

보안 원칙:

```text
- Agent는 main branch에 직접 push하지 않는다.
- Agent는 merge/deploy를 직접 수행하지 않는다.
- 모든 위험 작업은 Tool Gateway를 거친다.
- 모든 approval은 사용자 identity와 함께 저장한다.
- Discord 명령 권한은 role/user allowlist로 제한한다.
- GitHub token은 최소 권한으로 분리한다.
- Agent별 tool 권한을 분리한다.
- Secret 접근은 기본적으로 금지한다.
- Shell command는 allowlist 또는 approval 기반으로 제한한다.
- 모든 tool execution은 audit log에 저장한다.
```

권한 계층:

```text
User:
- 명령
- 승인 / 반려
- 긴급 중단

Orchestrator:
- workflow 상태 전이
- agent task 배정
- approval gate 생성

Agent:
- 제안
- 분석
- tool_request 생성

Tool Gateway:
- 실제 도구 실행
- policy enforcement

GitHub App / Token:
- Issue / Project / PR / branch 작업
- merge는 별도 제한
```

---

## 13. Runtime Architecture

MVP runtime:

```text
Local machine
  ├─ Canopus process
  ├─ Agent runtime
  ├─ Git worktree
  ├─ SQLite or Postgres
  └─ Discord / GitHub adapters
```

확장 runtime:

```text
Canopus Server
  ├─ API / adapters
  ├─ Workflow Orchestrator
  ├─ Agent scheduler
  ├─ Tool Gateway
  ├─ Postgres
  ├─ Redis / queue
  └─ Worker pool

Agent Worker
  ├─ repo checkout / worktree
  ├─ coding agent CLI
  ├─ test runner
  └─ artifact uploader
```

---

## 14. Storage Recommendation

초기 MVP:

```text
SQLite
- workflow
- task
- agent message
- approval
- GitHub mapping
- audit event
```

중기:

```text
Postgres
- source of truth
- concurrent worker support
- complex query
- durable approval state
- audit log
```

보조 저장소:

```text
Redis / NATS
- queue
- event bus
- worker wake-up
- temporary lock
- rate limit
```

Object storage:

```text
- patch artifact
- logs
- test output
- screenshots
- large transcripts
```

---

## 15. MVP Scope

MVP에서는 전체 multi-bot 구조를 만들지 않는다.

MVP 목표:

```text
1. Discord에서 명령 수신
2. Canopus workflow 생성
3. GitHub Project item 생성 또는 연결
4. Planner / Coder / Reviewer / QA Agent 실행
5. 내부 AgentMessage Bus에 대화 저장
6. Discord에 transcript 출력
7. Git branch 생성
8. 코드 수정 simulation 또는 실제 patch
9. 테스트 실행
10. Draft PR 생성
11. Discord 승인 요청
12. 승인 후 merge 또는 merge-ready 상태 전환
```

MVP Agent:

```text
Orchestrator
Planner
Coder
Reviewer
QA
```

MVP GitHub 상태:

```text
Ready for Agent
Agent Working
Waiting Human Approval
Ready to Merge
Done
Failed
```

MVP Discord 명령:

```text
/canopus start
/canopus status
/canopus approve
/canopus reject
/canopus pause
/canopus resume
```

---

## 16. 확장 단계

## Phase 1: Local Patch MVP

```text
- Discord 명령 수신
- Workflow 생성
- Local branch 생성
- Agent work simulation
- Local check 실행
- Artifact 저장
- Discord 알림
```

---

## Phase 2: GitHub Project Integration

```text
- GitHub Project item 감시
- GitHub Issue 연결
- Project status update
- GitHub mapping 저장
```

---

## Phase 3: PR Workflow

```text
- branch push
- Draft PR 생성
- CI 상태 감시
- PR summary 작성
- Discord review 요청
```

---

## Phase 4: Approval Gate

```text
- Discord button approval
- approval_request / approval_decision 저장
- merge approval 처리
- branch protection 상태 확인
```

---

## Phase 5: Agent Conversation Engine

```text
- AgentMessage schema 정착
- turn-based 대화
- review council
- loop 방지
- transcript projection
```

---

## Phase 6: Autonomous Agenda Loop

```text
- scheduled review
- observation / finding
- agenda_proposal
- agent proposal review
- Discord approval
- GitHub Project item 생성
```

---

## Phase 7: Production Hardening

```text
- Postgres 도입
- Redis/NATS queue
- multi-worker
- audit dashboard
- policy engine
- secret isolation
- cost/runtime monitoring
```

---

## 17. 비목표

초기 단계에서 하지 않을 것:

```text
- 자체 이슈 보드 만들기
- GitHub Project를 대체하기
- 처음부터 모든 Agent를 별도 Discord Bot으로 만들기
- Discord 메시지를 source of truth로 사용하기
- Agent가 직접 main에 push하기
- Agent가 승인 없이 merge/deploy하기
- 완전 자율 merge
- 복잡한 web dashboard
- agent marketplace
- skill registry
```

---

## 18. 핵심 설계 원칙

```text
1. Discord는 명령/승인/control room이다.
2. GitHub Project는 공식 작업 보드다.
3. Canopus는 agent orchestration kernel이다.
4. Agent 간 대화는 내부 AgentMessage Bus에서 일어난다.
5. Discord는 내부 대화의 transcript/projection이다.
6. Agent는 tool을 직접 실행하지 않고 Tool Gateway에 요청한다.
7. Orchestrator만 workflow 상태를 전이시킨다.
8. 사용자 승인 없이는 merge/deploy하지 않는다.
9. GitHub branch protection은 최종 안전장치다.
10. 자율 Agent loop는 agenda_proposal까지만 자동 생성하고, 실행은 policy에 따른다.
11. 모든 action은 audit log에 남긴다.
12. GitHub Project와 Canopus 내부 상태는 reconciliation 가능해야 한다.
```

---

## 19. 차별화 포인트

Canopus의 차별화는 이슈 보드나 일반 agent assignment가 아니다.

Canopus의 차별화는 다음이다.

```text
- Discord-native command / approval control plane
- GitHub Project-aware agent orchestration
- Agent-to-agent structured conversation protocol
- Turn-based agent review / debate
- Autonomous agenda proposal loop
- Decision memory
- Policy-gated tool execution
- Discord transcript 중심 observability
```

한 줄 정의:

```text
Canopus는 GitHub Project 위의 작업을 Discord에서 지휘하고,
Agent들이 내부적으로 대화·검토·제안·실행하도록 만드는
multi-agent development orchestration kernel이다.
```

---

## 20. 최종 목표

최종적으로 사용자는 Discord에서 Canopus를 통해 프로젝트를 지휘한다.

Agent들은 GitHub Project를 계속 감시하고, 필요한 작업을 찾아내며, 서로 검토하고, 사용자 승인이 필요한 지점에서는 Discord로 요청을 올린다.

사용자가 승인하면 Canopus는 GitHub branch, PR, CI, merge 흐름을 안전하게 진행한다.

전체 과정은 GitHub Project에 공식 상태로 남고, Discord에는 실시간 transcript와 알림으로 투영된다.

최종 목표는 다음과 같다.

```text
사용자:
Discord에서 명령하고 승인한다.

GitHub Project:
공식 작업 상태를 가진다.

Canopus:
Agent 팀을 운영하고 상태를 제어한다.

Agent:
GitHub Project와 Repository를 감시하고,
작업을 수행하고,
스스로 agenda를 제안한다.

Discord:
전체 과정을 사용자가 관찰하고 개입하는 control room이 된다.
```

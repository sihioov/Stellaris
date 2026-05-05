# Stellaris v2 Agent Collaboration Architecture

## Purpose

Stellaris v2 evolves from a command-to-task pipeline into a room-based multi-agent collaboration system. Transport bots receive messages, GitHub Issues/Projects hold official agendas, Discord exposes human-visible meeting rooms, role agents participate in discussion, and user-confirmed decisions produce executable action items and tasks.

In Korean terms: Stellaris v2는 단순히 명령을 task로 바꾸는 시스템이 아니라, 모든 요청과 자율 제안을 GitHub Project의 아젠다 이슈로 등록하고, 여러 역할 에이전트가 Discord 회의실에서 대화한 뒤, 사용자의 컨펌을 받은 경우에만 실행 가능한 작업으로 전환하는 협업형 에이전트 시스템이다.

## Non-Negotiable Principles

1. **GitHub is the agenda ledger.** Every user request, agent proposal, maintenance idea, feature request, bug fix, design topic, or decision candidate must be represented as a GitHub Issue and tracked on a GitHub Project board before it becomes executable work.
2. **Discord is the observable meeting room.** Discord is used not only as a command transport, but also as the place where humans can watch role agents exchange opinions, debate tradeoffs, summarize reasoning, and prepare decisions.
3. **User confirmation is mandatory.** Agents may discuss, suggest, summarize, and prepare proposals, but they must not finalize decisions, modify code, create PRs, push branches, change external systems, or mark an agenda as accepted without explicit user confirmation.
4. **Tasks are downstream of agendas.** A `TaskMessage` is created only after an agenda has enough discussion context and the user has approved the resulting action items.
5. **Auditability beats hidden autonomy.** Agent reasoning should be summarized into Discord and persisted back to the GitHub Issue so the user can understand why a decision or task exists.

Direct informational answers, such as a simple `!ask`, may be returned immediately. However, if the answer becomes a project decision, feature request, maintenance proposal, or executable action, it must be converted into a GitHub agenda and wait for user confirmation.

## V1 to V2 Shift

V1 flow:

```text
User / Discord / CLI → Task → TON618 → Laniakea → Canopus → Result
```

V2 target flow:

```text
User request or agent observation
→ GitHub Issue / Project agenda
→ Discord room or meeting thread
→ Role-agent discussion visible to the user
→ Proposal
→ User-confirmed decision
→ Approved ActionItem
→ Task
→ Scheduler / Worker / App execution
→ Review back in Discord and GitHub
```

The key shift is that `Task` is no longer the first-class product of every user request. The first durable product is an `Agenda` backed by GitHub Issue/Project state. A task is an execution artifact created only after discussion, decision, and explicit human approval.

## Operating Modes

Stellaris v2 has two primary operating modes.

### 1. Request-Driven Development Mode

This mode starts from an explicit human request. The user may ask the AI system to create a project, answer a question, add a feature, fix a bug, review code, design a screen, write documentation, or perform another concrete development task.

```text
Human request
→ transport bot receives the message
→ GitHub Issue agenda is created or selected
→ GitHub Project tracks agenda status
→ Discord room or meeting is created or selected
→ relevant role agents discuss the request in visible messages
→ proposal and decision summary are prepared
→ user confirms or rejects the decision
→ approved action items become executable tasks
→ workers execute and report results back to Discord and GitHub
```

Examples:

- `!ask 오늘 날씨 어때?` — direct Q&A through a transport bot.
- `!run 로그인 화면 개선해줘` — create a collaboration flow that can produce design, development, QA, and review tasks.
- `새 프로젝트 만들어줘` — create a project room, initialize repository/workspace metadata, and assign planning tasks.

In this mode, the system responds to user intent and should keep the human-visible outcome clear. Even when the user initiated the work, the final agenda decision and all executable action items still require explicit user confirmation.

### 2. Autonomous Maintenance Mode

This mode runs even when no human has issued a direct command. Role agents can observe the project, discuss improvements, propose maintenance work, and prepare action items for human approval or scheduled execution.

```text
Hubble / Kepler / internal monitors / agent observations
→ candidate issue or improvement
→ GitHub Issue agenda and Project entry
→ maintenance Discord room discussion
→ role-agent debate and proposal
→ decision draft or PendingProposal
→ user confirms or rejects the proposal
→ approved action item
→ executable task
→ worker execution
→ review and audit trail in Discord and GitHub
```

Examples:

- Kepler finds repeated lint warnings and opens a maintenance discussion.
- ReviewerAgent notices duplicated code and proposes a cleanup.
- QAAgent detects missing regression coverage and proposes tests.
- DesignerAgent suggests a UX consistency improvement.
- SecurityAgent raises a dependency or trust-boundary concern.

Autonomous maintenance must not silently perform risky changes. It should prefer proposals, decisions, audit trails, and explicit approval gates before modifying code, pushing branches, opening PRs, or changing external systems. In this mode, agents can create and discuss GitHub agendas on their own, but execution remains blocked until the user confirms the agenda outcome.

## Core Concepts

| Concept | Responsibility |
|---|---|
| Workspace | Top-level collaboration boundary for projects, teams, and integrations. |
| Agenda | Official discussion/work item backed by a GitHub Issue and tracked on a GitHub Project. |
| GitHub Project | Source of truth for agenda lifecycle, priority, status, owner, and approval state. |
| Room | Long-lived collaboration space, such as project room, planning room, design room, or incident room. |
| Meeting | Bounded discussion session inside a room with a goal, participants, transcript, and outcome. |
| Participant | Human user or role agent participating in a room or meeting. |
| Role Agent | Specialized agent such as planner, developer, designer, reviewer, QA, security, or writer. |
| Message | Durable conversational event emitted by a human, transport bot, or role agent. |
| Proposal | Candidate plan, design, fix, or action suggested by one or more participants. |
| Decision | User-confirmed conclusion with rationale, constraints, and owner. |
| ActionItem | Assignable unit of work derived from a confirmed decision. |
| Task | Executable backend work item dispatched through Stellaris core after approval. |

## GitHub Agenda and Discord Meeting Model

GitHub and Discord have different responsibilities and both are required.

```text
GitHub Project = official agenda board
GitHub Issue   = durable agenda document and decision record
Discord Room   = human-visible meeting space
Agent Messages = observable discussion and reasoning summaries
User Confirm   = explicit approval gate
TaskMessage    = approved executable work item
```

A typical agenda issue should contain:

```md
## Agenda
What is being discussed or requested?

## Source
User request / Hubble / Kepler / RoleAgent observation

## Discord Room
Link to the channel, thread, or meeting where discussion happened

## Participants
Humans and role agents involved

## Discussion Summary
Condensed agent opinions and tradeoffs

## Proposal
Recommended outcome

## Risks
Known risks, costs, and alternatives

## Action Items
Work items to create only after confirmation

## User Confirmation
- [ ] Confirmed by owner
```

Suggested GitHub Project states:

```text
Inbox → Discussing → Waiting for Confirmation → Approved → In Progress → Pending Review → Done
Rejected / Deferred
```

## Transport Bots vs Role Agents

Transport bots are entry points. They should not own domain decisions. Discord is still important because it exposes the actual agent discussion to the user, but Discord remains the observable room rather than the official agenda ledger.

```text
DiscordBot / SlackBot / WebBot / CLI
→ normalize user input
→ create or link GitHub agenda
→ identify room/context
→ route to meeting or role agent
→ display discussion, proposals, confirmations, and results
```

Role agents are participants that do work or reasoning.

```text
PlannerAgent   → requirements, scope, milestones
DesignerAgent  → UX, flows, visual direction
DeveloperAgent → implementation plan and code work
ReviewerAgent  → correctness, architecture, maintainability
QAAgent        → test strategy and verification
SecurityAgent  → trust boundaries and risk review
WriterAgent    → docs and release notes
```

This separation lets Discord, Slack, Web, and CLI reuse the same role agents instead of duplicating behavior in each transport.

## Example Flow

```text
User: "로그인 화면 개선해줘"

DiscordBot:
  creates or finds a GitHub Issue agenda
  adds it to the GitHub Project board
  creates or finds a Discord room: "login-screen-improvement"

PlannerAgent:
  clarifies user goal and success criteria

DesignerAgent:
  proposes UX changes and interaction constraints

DeveloperAgent:
  checks implementation impact and likely files

ReviewerAgent:
  adds acceptance and accessibility review criteria

Decision draft:
  improve form feedback, CTA visibility, and accessibility labels

User confirmation:
  owner approves the decision in Discord or GitHub

Action items:
  1. DesignerAgent drafts UX proposal
  2. DeveloperAgent inspects current implementation
  3. QAAgent defines regression checks

Tasks:
  executable work items are emitted only after user confirmation
```

## Relationship With Existing Components

| Existing component | V2 role |
|---|---|
| Hubble | External signal collector that can propose GitHub agendas, rooms, meetings, or candidate work. |
| Kepler | Internal code signal collector that can propose technical GitHub agendas and maintenance discussion items. |
| GitHub Project | Official agenda board and confirmation/status source for both request-driven and maintenance modes. |
| GitHub Issue | Durable agenda document, proposal record, confirmation record, and audit trail. |
| Dysonsphere | Shared contracts for tasks, statuses, discoveries, and future agenda/room/meeting records. |
| TON618 | Scheduler for approved executable tasks, not meeting discussion. |
| Laniakea | Worker execution layer for action items converted into tasks. |
| Canopus | AI development workload used by role agents for planning, coding, review, and artifacts. |
| Discord bot | Observable meeting-room transport adapter, not the owner of role-agent behavior or official agenda state. |

## Boundaries

- A transport bot should not contain planner/developer/designer logic.
- A role agent should not depend on Discord-specific APIs.
- Every agenda must have a GitHub Issue and GitHub Project state before execution.
- Discord is for observable discussion; GitHub is the official agenda and decision ledger.
- A meeting can produce zero, one, or many tasks.
- A task must reference the GitHub agenda, confirmed decision, and action item that created it.
- User confirmation is required before finalizing agenda outcomes or creating executable work.
- No user confirmation means no code modification, PR creation, push, merge, external system mutation, or completion claim.
- Discovery findings should start as proposals, not executable work.

## Migration Strategy

1. Keep the current v1 task pipeline working.
2. Extract common bot services from `apps/europa/europa.py`.
3. Introduce GitHub agenda creation/linking before direct task creation.
4. Add role-agent names as first-class labels, for example `planner`, `developer`, `designer`, `reviewer`, `qa`, `security`, and `writer`.
5. Introduce lightweight room/meeting records before building a full conversation bus.
6. Route Discord channels/threads to rooms and GitHub agendas, not directly to execution tasks.
7. Let meetings emit decision drafts and action items, then convert only user-confirmed action items into `TaskMessage` records.
8. Persist agent-to-agent discussion summaries to the GitHub Issue so later transports can display the same conversation and audit trail.

## Open Questions

- Should room and meeting records live in Dysonsphere immediately, or start as a Canopus/app-level model?
- What is the minimum message schema for agent-to-agent conversation?
- How should a meeting decide that consensus is reached before asking the user for confirmation?
- What exact GitHub Project fields represent agenda status, priority, owner, mode, and confirmation?
- Should Discord channels map one-to-one to rooms, or can one channel contain multiple meetings?
- Which actions are allowed after confirmation but before a final human review?

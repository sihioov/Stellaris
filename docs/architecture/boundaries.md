# Stellaris Boundaries

This document codifies the current boundary decision for Europa, Canopus, and Dysonsphere. It is intentionally small: immediate work documents the boundary and records deferred specs; it does not move policy code, add CI gates, or relocate Europa again.

## 1. 3-layer architecture

**Europa / Discord UI adapter (Python)** is the human-facing Discord control surface. It translates Discord commands and responses, handles surface configuration, and stays thin enough that another surface can call the same engine behavior without copying business rules.

**Canopus / engine (Rust)** owns app policy, state mutation decisions, workflow semantics, GitHub/project gates, approval/finalization behavior, and durable artifacts. Canopus returns data or JSON-like results that surfaces format for their own channels.

**Dysonsphere / contract** owns the shared task and status contract used by producers, schedulers, workers, and app workloads. It is the stable interoperability layer, not a place for Canopus-only workflow policy or Discord-only presentation.

## 2. 5 boundary rules

1. Bot code must not make policy judgments; route approval semantics, workflow gates, GitHub mutation decisions, and finalization policy to Canopus.
2. Task and project I/O must go through Canopus-owned paths where Canopus is the policy owner; Europa may only perform adapter-local compatibility reads/writes that preserve the contract.
3. External system calls belong in Canopus or in an explicitly named adapter with a documented owner, not as ad hoc calls inside Discord handlers.
4. Europa owns only the Discord surface: command parsing, Discord permissions, embeds/messages, and operator-visible wording.
5. Growth signal rule: when a behavior must be reused by a second surface, move it behind a Canopus/Dysonsphere contract before adding the second implementation.

## 3. Symmetric anti-pattern

Boundary protection is symmetric: surface policy must not leak into the bot, and surface presentation must not leak into the engine. Engine(canopus)에 surface-specific 표현(Discord embed, Slack mrkdwn 등) 코드가 누출되면 그 자체로 boundary 위반이다 — 데이터/JSON만 반환한다.

The currently known Canopus-to-Discord presentation violations are deferred to `spec/canopus-discord-notify-extraction.md`; this document records the rule and backlog, not the extraction.

## 4. apps vs surfaces qualification test

**자격 테스트**: X가 최소 2개의 서로 다른 surface에서 호출 가능한가? Yes → `apps/`. No → `surfaces/`.

Canopus passes the test because CLI, Discord, future Slack/Web, and automation lanes can all call it as the AI workflow engine. Europa does not pass the test: it is Discord-specific and should be treated as a surface even while its current path remains `apps/europa/` for this small rename/docs phase.

## 5. Naming rules

Stellaris project names use celestial objects for durable components: Canopus, Dysonsphere, TON618, Laniakea, Hubble, Kepler, and Europa. Europa is the Discord moon-like surface around the Canopus engine, so the Europa rename removes technology-first naming without pretending the surface is engine code.

The long-term path is `surfaces/europa/`, but that move is intentionally deferred to `spec/europa-surface-move.md` so import paths, CI labels, docs, and operator runbooks can change together.

## 6. Migration backlog (deferred)

- `spec/europa-policy-migration.md` — Trigger: policy mutation 함수 6번째 추가 직전 또는 2026-Q3.
- `spec/europa-task-store-via-canopus.md` — Trigger: dysonsphere schema 변경 PR 또는 spec 1 머지 후 2주 이내.
- `spec/europa-surface-move.md` — Trigger: europa 외 두 번째 surface PR 작성 시점 또는 2026-12-31.
- `spec/europa-handler-split.md` — Trigger: europa.py 800줄 초과 시 자동 trigger.
- `spec/boundary-enforcement-ci.md` — Trigger: spec 1 또는 spec 2 머지 후 1 sprint 이내.
- `spec/canopus-discord-notify-extraction.md` — Trigger: europa stable 운영 후 또는 두 번째 surface 추가 시점, 늦어도 2026-Q4.

stub 미생성: 본 plan은 backlog 등록만 수행하며, 각 spec은 trigger 발동 시점에 별도로 작성한다.

## 7. Enforcement plan

Automatic enforcement is out of scope for this commit. `spec/boundary-enforcement-ci.md` will define grep/import/path gates, review checklist updates, and any CI behavior needed to keep Europa, Canopus, and Dysonsphere boundaries from drifting again.

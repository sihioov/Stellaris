# V1 Final for V2 Development — Spec

작성: 2026-05-05
ID: v1-final-for-v2-dev
근거 trace: [`docs/v1-closure-trace.md`](v1-closure-trace.md)
ambiguity at crystallization: 18% (threshold 20%)

---

## Goal

Stellaris V1 자기호스팅 운영 루프를 **mock runtime 위에서 끝낼 수 있는 모든 closure 작업**과 **V2 마이그레이션 비용을 영구 절감하는 hookpoint pre-decision**까지 닫아, V2 자기호스팅 개발에 안전하게 위임할 수 있는 상태로 만든다. 최초 실행 계획은 항목별 8개 PR 분해였고, 최종 구현은 PR-A 선행 merge 후 PR-B~C6를 closure 커밋 `6b61def`로 함께 닫았다. 외부 시스템 의존은 fixture/mock/dry-run으로 대체한다.

## Implementation Status

| PR | 상태 | 비고 |
|---|:--:|---|
| **PR-A** Agenda.source typed enum + GitHub deterministic agenda_id | ✅ **Merged** 2026-05-05 | `7bad6ec` · `f12935f` · `c43d88d`. Rust 10 + Python 4 회귀 테스트 추가. `cargo test --workspace` / clippy / python unittest green |
| **PR-B** AgentRunResult.message_log schema 예약 | ✅ **Closed** 2026-05-05 | `6b61def`. `AgentMessage` + `message_log` schema/default 호환 테스트 green |
| **PR-C1** PendingProposal happy-path 자동 검증 | ✅ **Closed** 2026-05-05 | `6b61def`. Europa PendingProposal → Pending happy-path + helper tests green |
| **PR-C2** v1_smoke + CI schedule 통합 | ✅ **Closed** 2026-05-05 | `6b61def`. `apps/canopus/tests/v1_smoke.rs` + `.github/workflows/ci.yml` schedule/job 추가. 로컬 smoke green |
| **PR-C3** Operator runbook 보강 | ✅ **Closed** 2026-05-05 | `6b61def`. `docs/v1-operator-runbook.md` 작성, dry-run/validate-read-only/live-ramp/V2-entry 절차 포함 |
| **PR-C4** validate-read-only 자동 호출 경로 | ✅ **Closed** 2026-05-05 | `6b61def`. `scripts/validate-read-only.ps1` + `start-pipeline.ps1` 연결, credential 없음 graceful skip 테스트 green |
| **PR-C5** finalize → delivery_finalize gate 연결 | ✅ **Closed** 2026-05-05 | `6b61def`. watch/finalize dry-run delivery gate sidecar 생성 및 idempotency 회귀 green |
| **PR-C6** Discord !show 식별성 강화 | ✅ **Closed** 2026-05-05 | `6b61def`. Discord IDs + finalize/delivery-gate artifact path 노출 테스트 green |

> **Execution update (2026-05-05)**: 최초 계획은 항목별 8개 PR이었으나, PR-B~C6는 최종 closure 커밋 `6b61def`에서 함께 landed 되었다. acceptance criteria는 아래에 보존하되, 현재 상태 기준으로 본 spec의 mock/offline V1 closure 범위는 closed 이다.

> **Live intake update (2026-05-05)**: 오늘 배포용 iteration-3 범위는 Discord 메시지 기반 작업지시/알림과 GitHub Issue intake까지 확장했다. `work-intake`는 `github_owner`/`github_repo`가 있는 등록만 live Issue를 만들고, Project v2 동기화는 `--project-sync off|best-effort|required` 정책과 gate/data preflight에 따르게 했다. `watch` finalization은 `Processed` 상태만으로는 실행하지 않고 decoded payload의 `approval_state=approved` 및 nonempty `finalize_requested_at` 증거를 요구한다. Europa approval payload는 `approved_by`, `approval_source=discord`, `approval_message_url` provenance를 기록한다.

## Current Closure Evidence

2026-05-05 로컬 검증 결과:

```bash
cargo test --workspace                              # green
cargo clippy --workspace --all-targets -- -D warnings # green
cargo fmt --all -- --check                         # green
python3 -m unittest apps/europa/test_bot_config.py # green
cargo test -p canopus --test v1_smoke              # green
```

구현 증거:

- PR-B: `apps/canopus/src/core/types.rs`, `apps/canopus/tests/mock_agent_runtime.rs`
- PR-C1: `apps/europa/canopus_client.py`, `apps/europa/bot.py`, `apps/europa/test_bot_config.py`
- PR-C2: `apps/canopus/tests/v1_smoke.rs`, `.github/workflows/ci.yml`
- PR-C3: `docs/v1-operator-runbook.md`, `docs/p0-local-dry-run-runbook.md`
- PR-C4: `scripts/validate-read-only.ps1`, `start-pipeline.ps1`, `apps/canopus/tests/validate_read_only_script.rs`
- PR-C5: `apps/canopus/src/cli/finalize.rs`, `apps/canopus/tests/p0_local_dry_run_loop.rs`, `apps/canopus/tests/v1_smoke.rs`
- PR-C6: `apps/europa/bot.py`, `apps/europa/payloads.py`, `apps/europa/test_bot_config.py`

Known validation gap: GitHub Actions remote CI green은 이 로컬 확인에서 직접 증명하지 않았다. `.github/workflows/ci.yml`의 PR/schedule job 구성과 동일한 로컬 명령은 green이다.

## Non-Goals

- **CommandAgentRuntime 운영 default 표준화 (P0-2)** — V1 마지막 별도 단계로 분리. 본 spec 범위 외.
- **live mutation gate 검증 절차 / live push/PR/finalize 1-cycle (P2-4 / Lane 3 갭 3)** — mock runtime swap과 동시 처리하는 V1 마지막 ramp-up 단계.
- **Q&A 답변 재주입 루프 (P2-1)** — V2 본체 작업.
- **Scheduler main path 통합 (Lane 2 결정 3)** — feature gate 코드 존재. V2 backlog 증가 시점에 켜는 것으로 충분.
- **AgentRole enum 변경 (Lane 2 결정 4)** — `Custom(String)` 충분히 유연. V2 안정화 후 처리.
- **DB/MQ 어댑터 추가** — V1 file-backed queue 기본.
- **추가 커밋/PR 묶기 변경** — 최초 계획의 항목별 8개 PR 분해는 acceptance criteria 추적 단위로 보존한다. 실제 PR-B~C6 구현은 `6b61def`에서 함께 landed 되었고, 이후 새 묶음 변경은 본 spec 범위 외다.

## Constraints

1. 8개 acceptance slice 모두 mock runtime + offline 환경에서 deterministic 검증 가능해야 한다.
2. 외부 시스템 호출(GitHub live API, Discord live webhook)은 fixture/mock/dry-run으로 대체.
3. 각 slice의 done 정의: (a) unit/integration test 추가, (b) `cargo test`/`python -m unittest` green, (c) `.github/workflows/ci.yml` 추가/수정. Remote CI green은 PR review 시 별도 확인한다.
4. 작업 단위: 항목별 PR 8개로 계획했으나, 실제 구현에서는 PR-B~C6가 `6b61def`에 함께 landed 되었다. acceptance criteria 추적은 slice 단위로 유지한다.
5. 작성/실행 주체: 에이전트(autopilot/team). 사용자는 review·merge만.
6. 실행 순서: V2 hookpoint pre-decision 먼저(PR-A, PR-B), 운영 closure 다음(PR-C1~C6).
7. 변경 안 되는 영역: `apps/canopus/src/core/run_identity.rs`(이미 결정론적), `AgentRole` enum, `Scheduler` feature gate, dysonsphere `Agenda` 위치(canopus 유지).
8. timeline: 4~7일 working time을 권장 상한으로 본다.

## Acceptance Criteria

### PR-A. Agenda.source typed enum + GitHub deterministic agenda_id  *(✅ Merged 2026-05-05 — 7bad6ec / f12935f / c43d88d)*

- **A1.** `apps/canopus/src/core/types.rs:5-26` `Agenda.source: String`을 `enum AgendaSource` 로 교체. variants 최소: `Cli`, `GitHubIssue { owner: String, repo: String, number: u64 }`, `GitHubProject { project_url: String, item_id: String }` (V2 확장 가능 형태).
- **A2.** `Agenda::from_github_issue(owner, repo, number, request)` 와 같은 source-aware 생성자 추가. agenda_id를 `"gh-{owner}-{repo}-{number}"` 형식으로 결정론적 생성 (`derive_run_identity` 의존, sanitize 결과 안정성 확인).
- **A3.** 호출자 변경:
  - `apps/canopus/src/cli/submit.rs` — `--github-issue-number` 인자가 있을 때 `Agenda::from_github_issue` 사용
  - `apps/europa/bot.py` — `!run`(GitHub Issue 컨텍스트 있을 때) 및 `!propose-approve`/`!propose-*` 핸들러에서 GitHub Issue 식별자 기반 agenda_id 전달
- **A4.** Serde 호환: 기존 `source: "cli"` 직렬화 read 호환을 위한 default/migrate 처리 (loose deserialize).
- **A5.** 회귀 테스트: 동일 GitHub Issue 입력에서 동일 agenda_id 생성, 다른 source 입력에서는 다른 ID. CLI/GitHub source 모두 round-trip 직렬화 통과.
- **A6.** `cargo test -p canopus` green, `cargo clippy --all-targets -- -D warnings` green.
- **A7.** Europa 핸들러 회귀: `apps/europa/test_bot_config.py`에 GitHub source 기반 propose 흐름이 결정론적 agenda_id를 만드는지 단위 테스트.

### PR-B. AgentRunResult.message_log schema 예약

- **B1.** `apps/canopus/src/core/types.rs:303-307` `AgentRunResult`에 `message_log: Vec<AgentMessage>` 필드 추가 (default empty).
- **B2.** `AgentMessage` struct 신규 정의: 최소 필드 `role: String`, `content: String`, `created_at: DateTime<Utc>`. (V2 meeting 모델에서 확장)
- **B3.** Serde round-trip 테스트, 빈 vec 직렬화/역직렬화 호환 (기존 record와 충돌 없음).
- **B4.** Persist 구현은 V2 초반 작업으로 미룸 (본 PR에서 schema와 default-empty 채움만).
- **B5.** `cargo test -p canopus` green, clippy green.

### PR-C1. PendingProposal happy-path 자동 검증

- **C1-1.** `apps/europa/test_bot_config.py`에 `test_propose_approve_happy_path_transitions_to_pending` 추가. `intake_github_work` mock이 성공을 반환할 때 task status가 `PendingProposal → Pending`으로 전환되는지 assert.
- **C1-2.** `promote_pending_proposal_with_intake` happy-path 단위 테스트(payload 업데이트 + 상태 전환).
- **C1-3.** `python3 -m unittest apps/europa/test_bot_config.py` green, 기존 실패-경로 테스트(`test_bot_config.py:251-288`)와 공존.

### PR-C2. smoke harness + CI schedule 통합

- **C2-1.** `apps/canopus/tests/v1_smoke.rs` 신규 통합 테스트:
  - seed `Pending` task → TON618 bounded dispatch (1 tick) → Laniakea bounded handler (mock canopus binary 또는 inline call) → assert `PendingReview` → mutate to `Processed` (approve) → run `canopus watch --once` → assert `.canopus/runs/<run_id>-finalize.txt` 존재 및 형식 검증.
  - 모든 외부 의존성(file paths, GitHub) tempdir/mock 사용.
- **C2-2.** `.github/workflows/ci.yml`에 새 job 추가 (smoke). PR 트리거 + `schedule:` cron 트리거 (예: 매일 1회).
- **C2-3.** `cargo test -p canopus --test v1_smoke` green.
- **C2-4.** GitHub Actions에서 smoke job green.

### PR-C3. Operator runbook 보강

- **C3-1.** 기존 `docs/p0-local-dry-run-runbook.md`를 확장하거나 새 `docs/v1-operator-runbook.md` 작성. 다음 섹션 포함:
  - `.env` 전체 키 (`.env.example`의 23개 변수 모두 의미 + 필수/선택 표시)
  - `pwsh start-pipeline.ps1 -DryRun`/`live` 분기 설명
  - approve/reject 기준 (어느 stage record/artifact를 보고 결정하는가)
  - 실패 복구 절차 (stuck task 청소, finalize record 누락 시 재실행)
  - `validate-read-only` 운영 절차 (자동 호출 경로 + 수동 1회 검증 방법)
  - **live mutation 전환 절차** — V1 본 spec 범위 밖이지만 마지막 단계 안내로 명시
  - **V2 진입 절차** — agent runtime swap (`CANOPUS_AGENT_RUNTIME=command`) 안내
- **C3-2.** PR 리뷰에서 마크다운 렌더 정상 + 모든 file:line 인용 유효성 (간단한 grep 스크립트로 가능).

### PR-C4. validate-read-only 자동 호출 경로

- **C4-1.** `start-pipeline.ps1` 또는 신규 보조 스크립트(`scripts/validate-read-only.ps1` 등)에 `CANOPUS_ENABLE_GITHUB=1` + `CANOPUS_GITHUB_PROJECT_MODE=validate-read-only`로 1회 실행하는 단계 추가. live mutation gate(`CANOPUS_ENABLE_LIVE_MUTATIONS`, `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION`)는 0으로 유지.
- **C4-2.** 자격증명 부재 시 graceful skip (env 미설정 또는 token 없음 → warn + skip, fail 아님). 단위 테스트에서 검증.
- **C4-3.** 기존 `apps/canopus/tests/github_project_v2.rs:53-79` `github_project_validate_read_only_queries_only`와 일관 — mutation 호출 없음 회귀 보장.
- **C4-4.** runbook(C3)에서 이 경로 호출 방법 인용.

### PR-C5. finalize → delivery_finalize gate 자동 연결

- **C5-1.** `apps/canopus/src/cli/finalize.rs:9-67` watch 경로에서 `post_approval` 후 `delivery_finalize` gate 검사를 dry-run으로 호출하도록 수정. gate 결과(`DeliveryGateReport`)를 finalize record(.txt)에 포함하거나 별도 sidecar(`<run_id>-delivery-gate.json`)로 저장.
- **C5-2.** dry-run에서 git/gh CLI 호출 0건 회귀 보장 (`finalize.rs:281-310` 패턴 재사용).
- **C5-3.** `cargo test -p canopus` green (idempotency 회귀 + 새 gate-record 회귀).

### PR-C6. Discord !show 식별성 강화

- **C6-1.** `apps/europa/bot.py:562-582` `!show` 링크 섹션에 추가:
  - `discord_channel_id`
  - `discord_message_id`
  - finalize record 경로 (`<state>/runs/<run_id>-finalize.txt`)
  - delivery gate sidecar 경로 (PR-C5 산출물 있을 때)
- **C6-2.** `apps/europa/payloads.py:154-178` `_artifact_paths`가 `<lookup_id>-finalize.txt` 및 `<lookup_id>-delivery-gate.json` 패턴도 탐색.
- **C6-3.** `apps/europa/test_bot_config.py`에 `!show` 출력 검증 단위 테스트 추가.
- **C6-4.** `python3 -m unittest apps/europa/test_bot_config.py` green.

## Assumptions Exposed

- 본 closure 기간 동안 `CANOPUS_AGENT_RUNTIME`은 미설정 또는 mock으로 유지된다 (사용자 명시 결정, 2026-05-05).
- `derive_run_identity(agenda_id, task_id)`는 결정론적 통과 함수이다 (`apps/canopus/src/core/run_identity.rs:4-19` 직접 확인). PR-A의 deterministic agenda_id는 호출자 수준에서만 만들어지면 된다.
- Europa = 이전 `apps/discord-bot/`의 리네임 (커밋 `9f48db9`).
- `start-pipeline.ps1`은 Windows/WSL PowerShell 환경. CI에서는 별도 bash 등가물 또는 `pwsh` 호출.
- 기존 unit/integration tests의 회귀를 깨지 않는다 (모든 PR이 추가 변경, 호출자 외 기존 동작 보존).
- 한 번에 한 PR만 머지된다고 가정. 병합 충돌 위험은 PR 순서 (A → B → C1~C6)로 완화.

## Technical Context

**워크스페이스 / 핵심 파일:**

- `apps/canopus/src/core/{types.rs, run_identity.rs, pipeline.rs}`
- `apps/canopus/src/cli/{mod.rs, submit.rs, finalize.rs, args.rs}`
- `apps/canopus/src/adapters/{github/client.rs, agent_runtime/{command.rs, mod.rs}, artifact_store/local_file.rs, tool_gateway/local.rs}`
- `apps/canopus/tests/{p0_local_dry_run_loop.rs, github_project_v2.rs, v1_smoke.rs(신규)}`
- `apps/europa/{bot.py, payloads.py, config.py, test_bot_config.py}`
- `ton618/src/main.rs`
- `laniakea/src/{worker.rs, handlers/custom.rs}`
- `dysonsphere/src/{discovery/mod.rs, db/task_table_file.rs, status.rs, message.rs}`
- `start-pipeline.ps1`
- `.github/workflows/ci.yml`
- `docs/p0-local-dry-run-runbook.md`, `docs/v1-operator-runbook.md`(신규 또는 확장)

**검증 명령:**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 -m unittest apps/europa/test_bot_config.py
cargo test -p canopus --test v1_smoke    # PR-C2 이후
```

**관련 v3 readiness 매핑:**

- 본 spec PR-A/B = Lane 2 결정 1+2+5
- PR-C1 = Lane 1 갭 4
- PR-C2 = Lane 1 갭 1 + Lane 3 갭 4 (수렴)
- PR-C3 = Lane 1 갭 5 + Lane 3 갭 2 (수렴)
- PR-C4 = Lane 3 갭 1
- PR-C5 = Lane 1 갭 3
- PR-C6 = Lane 3 갭 5

## Ontology

| Entity | Owning crate / file | V1 변경 | V2 확장 의도 |
|--------|---------------------|--------:|--------------|
| Agenda | canopus `core/types.rs` | source 필드 typed enum | Room/Meeting/Decision 얹기 |
| AgendaSource (신규) | canopus `core/types.rs` | Cli / GitHubIssue / GitHubProject variants | Slack/Web 등 트랜스포트 추가 |
| AgentTask | dysonsphere | 변경 없음 | — |
| StageRecord | canopus `core/types.rs` | 변경 없음 | persist 형식 호환 유지 |
| AgentRunResult | canopus `core/types.rs` | message_log 필드 추가 (default empty) | 토론 로그 채워넣기 |
| AgentMessage (신규) | canopus `core/types.rs` | role/content/created_at 최소 schema | meeting/decision link 추가 |
| TaskStatus | dysonsphere | 변경 없음 | — |
| AgentRole | canopus `core/types.rs` | 변경 없음 | V2 role을 Custom으로 또는 추후 named variant |
| GitHubProjectMode | canopus `adapters/github/client.rs` | 변경 없음 | — |
| run_id | `derive_run_identity(agenda_id, task_id)` 결과 | 변경 없음 (결정론적 통과) | — |
| Finalize Record | `<state>/runs/<run_id>-finalize.txt` | 변경 없음 | — |
| Delivery Gate Sidecar (신규) | `<state>/runs/<run_id>-delivery-gate.json` | PR-C5에서 신설 | — |

## Ontology Convergence

- **Agenda ↔ GitHub Issue 식별 bridge**: PR-A에서 `AgendaSource::GitHubIssue { owner, repo, number }` 변경 + agenda_id를 `"gh-{owner}-{repo}-{number}"`로 명시 생성. 동일 Issue 입력 → 동일 agenda_id (V2 ledger idempotency 기반). 결정론은 `derive_run_identity`가 입력을 sanitize 통과하므로 호출자 수준에서 고정.
- **StageRecord(결과 메타) vs AgentMessage(중간 토론)**: 서로 다른 layer. PR-B의 message_log는 AgentRunResult layer에서 stage 결과와 분리 보관.
- **Finalize record vs Delivery gate sidecar**: 둘 다 `<state>/runs/`에 두되 파일 suffix로 구분 (`-finalize.txt` / `-delivery-gate.json`). PR-C6의 `_artifact_paths`가 양쪽을 동일 lookup으로 노출.

## Trace Findings

trace 결과(`.omc/specs/deep-dive-trace-v1-final-for-v2-dev.md`) 핵심을 다음과 같이 spec에 반영했다.

- **Most likely explanation**: V1 종료 = 3묶음 — (1) mock-friendly closure, (2) V2 hookpoint pre-decision, (3) V1 마지막 ramp-up. 본 spec은 (1)+(2)만 다룬다. (3)은 별도 단계.
- **Lane 1 critical unknown 해소**: PendingProposal happy-path 자동 검증은 PR-C1로 닫혔다.
- **Lane 2 critical unknown 해소**: `derive_run_identity`가 결정론적이므로 PR-A의 호출자 수정 비용이 매우 낮다.
- **Lane 3 critical unknown 일부 해소**: validate-read-only 자동 호출 경로는 PR-C4로 닫혔다. live e2e 1회 실행은 V1 마지막 ramp-up(non-goal)으로 분리.
- **수렴 1 (runbook)**: Lane 1 갭 5 + Lane 3 갭 2 → PR-C3 단일 작업.
- **수렴 2 (smoke + CI)**: Lane 1 갭 1 + Lane 3 갭 4 → PR-C2 단일 작업.
- **수렴 3 (agenda bridge)**: Lane 2 결정 1+5 → PR-A 단일 작업. struct 위치 이동 없음.

## Interview Transcript

| Round | 질문 | 답변 | ambiguity |
|------|------|------|----------:|
| 1 | V1 종료 = done 범위 | mock-friendly closure 6 + V2 hookpoint pre-decision 2 = 8개 모두 포함 | 0.85 → 0.60 |
| 2 | AC 강도 / 작업 단위 | 표준 closure (test + green + CI 통합, 항목별 PR, 4-7일) | 0.60 → 0.45 |
| 3 | 실행 순서 | V2 hookpoint 먼저 (PR-A → PR-B → PR-C1~C6) | 0.45 → 0.30 |
| 4 | 검증 주체 / 외부 의존성 | 에이전트 자동 + offline-only (mock/fixture, 사용자는 review·merge) | 0.30 → 0.18 |

---

## 메타: V1 closure 후 잔여 단계 (이 spec의 직후)

- **V1 마지막 ramp-up (별도 spec)**: P0-2 CommandAgentRuntime 운영 default 표준화 + Lane 3 갭 3 live mutation gate 검증 시퀀스 + validate-read-only e2e 1회 실행
- **V2 본체 작업**: agenda Room/Meeting/Decision 모델, message_log persist 구현, GitHub Issue를 agenda ledger source-of-truth로 승격, V2 role agent 분리

본 spec의 mock/offline V1 closure 범위는 닫혔다. V1 마지막 ramp-up 1개 PR(CommandAgentRuntime 운영 default 표준화 + live mutation gate 검증 시퀀스 + validate-read-only 실제 e2e 1회)이 닫히면 v2 위임 가능성 ~85~90% 도달 (메타 게이지).

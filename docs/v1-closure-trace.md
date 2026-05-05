# Deep Dive Trace: v1-final-for-v2-dev

작성: 2026-05-05
대상: Stellaris V1 종료 작업 — V2 자기호스팅 개발 진입을 위한 마지막 마무리

## Observed Result / Problem Statement

사용자가 "v2를 개발하기 위한 v1 최종 작업"에 진입하려 한다. 사전 합의:

- v3 readiness 문서(2026-05-03) 시점 P0 6개 중 5개가 이미 닫혀 있다 (P0-1 CLI 호환, P0-3 watch persist, P0-4 idempotency, P0-5 launcher watch, P0-6 runbook 부분).
- 9개 성공 기준 중 8개 통과. 미통과는 5번(default mock)·9번(smoke harness).
- 사용자가 명시적으로 결정: **AI mock runtime 표준화(P0-2)는 V1 작업 중 가장 마지막에 처리**한다. 즉 mock 위에서 끝낼 수 있는 closure 작업이 그 앞 모든 단계를 차지한다.

따라서 본 trace의 질문은 "이 시점에서 V1 종료 직전에 무엇을 더 해야 V2를 이 시스템 위에서 개발할 수 있는가"이다.

## Status Addendum — 2026-05-05

이 trace는 closure spec 작성 전의 발견/우선순위 기록이다. 이후 `docs/v1-closure-spec.md` 기준 PR-A~C6 중 PR-A는 선행 merge 되었고, PR-B~C6는 closure 커밋 `6b61def`에서 함께 landed 되었다.

현재 상태:

- mock/offline V1 closure 범위: **closed**
- 로컬 검증: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `python3 -m unittest apps/europa/test_bot_config.py`, `cargo test -p canopus --test v1_smoke` green
- remote GitHub Actions green: 이 로컬 trace update에서는 미확인
- 잔여 V1 작업: 별도 V1 마지막 ramp-up — `CANOPUS_AGENT_RUNTIME=command` 운영 default 표준화, live mutation gate 검증 시퀀스, validate-read-only 실제 e2e 1회

아래 Ranked Hypotheses / Evidence Summary는 당시 판단 근거를 보존하기 위한 historical record이며, 최신 상태 판단은 spec의 `Implementation Status`와 `Current Closure Evidence`를 우선한다.

## Ranked Hypotheses

| Rank | Hypothesis | Confidence | Evidence Strength | Why it leads |
|------|------------|------------|-------------------|--------------|
| 1 | **Lane 1 운영 루프 closure**: PendingProposal happy-path 자동 검증, smoke harness, runbook 보강이 V1 종료의 직접적 배관 마무리. mock 위에서 즉시 닫을 수 있고 회귀 가시성 확보 | High | Strong | 갭 4가 mock 위에서 한 번도 자동 검증된 적 없음. 갭 1은 단일 명령 smoke 부재. 사용자 결정상 mock-friendly 작업이 우선 |
| 2 | **Lane 2 V2 hookpoint 구조 결정**: Agenda source enum + GitHub deterministic ID 생성 (결정 1+5가 단일 변경) + AgentRunResult message_log 필드 예약 (결정 2). V2 마이그레이션 비용 영구 절감 | High | Strong | `derive_run_identity`가 이미 결정론적이므로 호출자에서 `gh-owner-repo-number` 형태로 넘기는 것으로 V1 마감 비용 거의 0. 결정 2는 schema 정의만 (persist 구현은 V2 초반) |
| 3 | **Lane 3 신뢰/검증 갭**: live mutation gate 검증 절차, validate-read-only 운영, regression CI 자동화. V2 위임 안심 축이지만 사용자 결정(mock 마지막)에 의해 일부는 마지막 단계로 묶임 | Medium | Strong | 갭 3(live gate 검증) "차단" 등급이지만, mock runtime swap 이후에야 가치가 발생. 갭 1·2는 V1 closure에 함께 처리 가능 |

## Evidence Summary by Hypothesis

### Lane 1 (Strong — 갭 4가 가장 큰 closure 항목)
- **갭 1 E2E smoke harness**: `scripts/v1-smoke.sh` 부재 확인. `apps/canopus/tests/p0_local_dry_run_loop.rs:46`은 watch+finalize 단편만 커버. 다중 프로세스 (TON618 → Laniakea → Canopus → Europa) fixture 없음. 강도 Strong.
- **갭 2 watch idempotency 회귀**: `apps/canopus/tests/p0_local_dry_run_loop.rs:46` `watch_and_explicit_finalize_share_idempotent_dry_run_record` 존재. **이미 닫힘**.
- **갭 3 finalize → PR dry-run 경로**: `apps/canopus/src/cli/finalize.rs:116-124` watch dry-run은 finalize record만 생성, `delivery_finalize` gate 호출 없음. 의도적이나 watch에서 PR dry-run 시뮬레이션이 자동 연결 안 됨.
- **갭 4 PendingProposal happy-path**: `apps/europa/test_bot_config.py:251-288` propose-approve 테스트가 **실패 경로만 커버**. canopus tests에 `PendingProposal`/`propose`/`Hubble`/`Kepler`/`discovery` 0건. **mock 위에서도 happy-path 한 번도 자동 검증 안 됨**. 강도 Strong.
- **갭 5 runbook 완성도**: `docs/p0-local-dry-run-runbook.md` 52줄 — approve/reject 기준, 실패 복구 절차, live vs dry-run 차이 누락. 강도 Moderate.

### Lane 2 (Strong — 결정 1+5는 거의 무비용 V1 마감)
- **결정 1 Agenda 위치**: `apps/canopus/src/core/types.rs:5-26` canopus app-level. `source: String`이 `"cli"`로 하드코딩. struct 이동 비용 vs 가치 비교 시 **canopus 유지가 의존성 그래프 정합**.
- **결정 2 메시지 영속화**: `apps/canopus/src/core/types.rs:309-317` StageRecord는 결과 메타데이터만. `AgentRunResult.summary: String` (~types.rs:303-307) 하나로 압축. V2 multi-agent meeting 모델 위해 `message_log: Vec<AgentMessage>` schema 정의 필요. 영향 High.
- **결정 3 Scheduler main path**: `ton618/src/main.rs:46-81` 단순 polling. priority queue/runner는 `#[cfg(feature = "scheduler-cron")]` dead code. V2 backlog 증가 시점에 켜도 충분. **V2 초반 권장**.
- **결정 4 V2 role agent 정렬**: `apps/canopus/src/core/types.rs:29-44` `AgentRole::Custom(String)` 충분히 유연. as_str() pattern-match 비파괴적. **V2 초반 이후 권장**.
- **결정 5 GitHub agenda bridge**: `derive_run_identity(agenda_id, task_id)` (`apps/canopus/src/core/run_identity.rs:4-19`)가 입력을 그대로 sanitize·통과. **호출자가 `"gh-{owner}-{repo}-{number}"`만 agenda_id로 넘기면 결정론적 ID 생성**. 즉 V1 마감 비용 = 호출 경로 수정 + source enum 도입 = **매우 낮음**.

### Lane 3 (Strong — 일부 항목은 mock swap 시점과 묶임)
- **갭 1 validate-read-only 운영**: `apps/canopus/src/adapters/github/client.rs:178-312` 코드 완성. `start-pipeline.ps1:67,76` 자동 호출 경로 부재. `.github/workflows/ci.yml` 단계 없음. `payloads.py:48-53` env 의존, 기본값(dry-run-offline) 변경 시점 없음. 강도 Strong, V2 차단도 안심 저하.
- **갭 2 runbook 보강**: `.env.example` 23개 변수 vs runbook 5개 언급. validate-read-only / live mutation 전환 절차, 실패 복구, V2 진입 절차 부재. 강도 Strong. (Lane 1 갭 5와 부분 수렴)
- **갭 3 live mutation gate 검증 절차**: `start-pipeline.ps1:76` gate 0 고정. dry-run vs live 자동 비교 wrapper 없음. rollback 경로 없음. 강도 Strong, V2 차단. **단 사용자 결정상 mock runtime swap 이후 처리되는 V1 마지막 단계 작업**.
- **갭 4 regression 신호 자동화**: `.github/workflows/ci.yml:3-5` push/pull_request만, schedule 트리거 없음, smoke harness CI 미통합. 강도 Moderate. (Lane 1 갭 1과 수렴)
- **갭 5 Discord artifact visibility**: `apps/europa/europa.py:562-582` `discord_channel_id`/`discord_message_id` 노출 안 됨. finalize record 경로 패턴(`<task-id>-finalize.txt`) 탐색 미보장. 강도 Weak-Moderate.

## Evidence Against / Missing Evidence

- **Lane 1 against**: `cargo test -p canopus`는 단편 회귀를 잘 잡음. watch idempotency는 이미 회귀 테스트 보유. 즉 "전부 빠진" 게 아니라 "PendingProposal happy path + multi-process e2e + runbook 운영 절차"가 핵심 결손.
- **Lane 2 against**: AgentRole::Custom(String)은 V2 role을 코드 변경 없이 받아주므로 결정 4는 V1 cost 0. Scheduler feature gate가 이미 코드로 존재해 결정 3은 켜는 비용만 남음.
- **Lane 3 against**: validate-read-only / live gate 강제는 코드에 정확히 구현됨 (`client.rs:600-616`). 단위 테스트도 존재 (`github_project_v2.rs:53-79`). 즉 코드 부재가 아닌 운영 절차/자동 호출 부재.

## Per-Lane Critical Unknowns

- **Lane 1**: PendingProposal happy-path가 mock/offline 환경에서 **한 번이라도 자동 검증된 바 있는가**. 현재 테스트는 실패 경로만 존재. → discriminating probe: `apps/europa/test_bot_config.py`에 `test_propose_approve_happy_path_transitions_to_pending` 추가.
- **Lane 2**: ~~`derive_run_identity` 구현~~ → **본 turn에서 해소**. 결정론적 통과 함수로 확인. 결정 5의 V1 마감 비용은 호출자 수정 + source enum 도입 수준.
- **Lane 3**: validate-read-only가 실제 자격증명으로 한 번이라도 e2e 실행되어 project_id/item_id/option_id 해석이 성공했는지. → discriminating probe: 실제 자격증명으로 `canopus submit --github-project-mode validate-read-only` 1회 실행.

## Rebuttal Round

**Leader (Lane 1 + Lane 2 V1 마감)**: V1 종료 작업은 운영 루프 closure(특히 PendingProposal happy-path) + V2 hookpoint 구조 결정(특히 Agenda source enum + GitHub deterministic ID)이다.

**Best rebuttal (Lane 3)**: live mutation gate 검증 절차 부재가 V2 위임의 가장 직접적 차단이다. 이걸 V1 종료 작업에서 빼면 첫 live 실행이 곧 첫 검증이 된다.

**Why leader holds**: 사용자가 "AI mock runtime은 V1 가장 마지막"이라고 명시했고, mock runtime swap 전에는 live mutation도 가치가 없다. 따라서 Lane 3 갭 3(live gate 검증)은 V1 *closure*가 아니라 V1 *최종 ramp-up* 단계에 묶이는 것이 정합적이다. 두 사건은 같은 commit/시점이 아니다. Lane 3 갭 1·2(validate-read-only 자동 호출 경로 추가, runbook 보강)는 mock 위에서도 가치 있어 V1 closure에 함께 처리.

## Convergence / Separation Notes

- **수렴 1 — runbook**: Lane 1 갭 5 + Lane 3 갭 2 → 단일 runbook 보강 작업으로 닫힌다 (approve 기준, 실패 복구, validate-read-only/live 전환 절차, V2 진입 절차).
- **수렴 2 — smoke + CI**: Lane 1 갭 1 + Lane 3 갭 4 → smoke harness를 작성해 CI에 schedule 트리거로 넣으면 양쪽 동시 closure.
- **수렴 3 — agenda bridge 단일 변경**: Lane 2 결정 1 + 결정 5 → `Agenda.source` typed enum + 호출자에서 GitHub Issue 기반 deterministic agenda_id. struct 이동 불필요.
- **분리 — live gate 검증과 mock swap**: Lane 3 갭 3은 mock runtime swap (P0-2)과 같은 V1 마지막 단계에서 함께 다루어야 함. 그 전에 다루는 것은 비효율.

## Most Likely Explanation

V1 종료 작업은 **3 묶음**으로 구성된다:

1. **mock-friendly closure** (즉시 처리 가능, V1 본체)
   - PendingProposal happy-path 자동 검증 (Lane 1 갭 4)
   - smoke harness 작성 + CI schedule 통합 (Lane 1 갭 1 ↔ Lane 3 갭 4)
   - runbook 보강 (Lane 1 갭 5 ↔ Lane 3 갭 2)
   - validate-read-only 자동 호출 경로 (mutation 없으므로 mock과 무관, Lane 3 갭 1)
   - finalize → delivery_finalize gate 자동 연결 (Lane 1 갭 3)
   - Discord !show 식별성 보강 (Lane 3 갭 5)

2. **V2 hookpoint pre-decision** (영구 비용 절감, V1 본체에 포함)
   - Agenda.source typed enum + GitHub deterministic agenda_id 호출자 적용 (Lane 2 결정 1+5)
   - AgentRunResult.message_log 필드 schema 예약 (Lane 2 결정 2)

3. **V1 마지막 ramp-up** (사용자 결정상 마지막)
   - CommandAgentRuntime 운영 default 표준화 (P0-2)
   - live mutation gate 검증 시퀀스 + rollback 경로 (Lane 3 갭 3)

V1 종료 = (1) + (2) + (3)을 이 순서로 닫는 것. 진척도 추정으로는 (1)+(2) 완료 시점이 v2 위임 가능성 ~75%, (3) 완료가 ~85~90%.

## Critical Unknown (synthesized)

가장 결정적으로 남은 단일 불확실성: **위 3 묶음 중 (1) 묶음의 항목들에 대한 우선순위와 범위**. 사용자가 어디까지를 V1 closure로 보는가 — 6개 항목 전부인가, "가장 큰 갭" 4개(PendingProposal happy-path, smoke+CI, runbook, validate-read-only 운영)인가, 더 좁은 minimum-viable 셋인가. 또한 V2 hookpoint pre-decision에서 결정 5의 호출자 변경 범위 (Europa `!run` 만인가, propose-* 도 포함인가).

## Recommended Discriminating Probe

다음 1개 질문으로 (1)+(2) 묶음의 범위와 순서가 거의 결정된다:

> "V1 closure 완료 시점에 다음 6개 항목 중 어느 셋을 'V1 종료 = done' 조건으로 두는가, 그리고 V2 hookpoint pre-decision에서 GitHub deterministic agenda_id를 Europa `!run`만 적용할지 propose-*도 같은 변경에 포함할지?"

이 질문이 spec의 Goal/Constraints/Acceptance Criteria 절반을 즉시 결정한다.

---

## 메타: 추정 v2 위임 가능성 게이지

- **현재** (이 turn 시점): 코드 ~85% / 운영 루프 ~80% / 위임 ~55~65%
- **묶음 (1)+(2) 완료**: 코드 ~92% / 운영 루프 ~95% / 위임 ~70~75% (mock 한계는 의도적 cap)
- **묶음 (3) 완료 (V1 종료)**: 위임 ~85~90%, live mutation 1-cycle은 V2 작업과 동시 검증 OK

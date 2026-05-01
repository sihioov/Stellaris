# Stellaris / Canopus v1 통합 보안 설계안

## 0. 배경 / 문서 위치

- **이 문서**: `docs/stellaris-canopus-v1-security-integration.md`
- **기준 문서**: `docs/stellaris-canopus-architecture-v1.md` (특히 §11 안전장치, §14 설계 원칙)
- **출발점**:
  - `main` `36559db` — v1 §12.1 P0 체크리스트 표면 항목 구현
  - `ralph/ancient-hejlsberg` `fb65dee` — §11 anti-pattern 방어 + cross-process 안전성 강화
- **범위**: 두 커밋을 v1 안전 모델 기준으로 통합. **feature 확장은 별 PR에서.**

---

## 1. 통합 원칙

1. **Base는 `ralph/ancient-hejlsberg`.**
   - §11 anti-pattern 항목 충족도와 §13 v2 forward-compat 매핑이 더 정확.
   - 특히 `main`은 `TON618` 필터를 갱신하지 않아 **PendingProposal 자동 dispatch가 일어나는 실제 결함**이 있다 (§4.3·§11.5 위반).
2. **`main`에서 흡수할 두 가지** (협상 여지 없음):
   - `find_git_subcommand()` — git global flag 정규화로 `git -C /path push --force` 류 우회 봉쇄.
   - `chrono::DateTime<Utc>` RFC3339 — `unix:<secs>` 문자열 폐기.
3. **Scope-creep 금지.** LLMAgentRuntime, Laniakea log 캡처, max_retries는 통합 이후 별도 작업.

---

## 2. 통합 후 컴포넌트별 명세

### 2.1 Tool Gateway (`apps/canopus/src/adapters/tool_gateway/local.rs`)

- `check_policy` 진입부에서 **`find_git_subcommand`로 global flag 정규화 후 subcommand 매칭**.
- Deny rules (branch에서 유지):
  - `git push --force / -f / --force-with-lease[=…]`
  - 보호 브랜치 push: refspec RHS 파싱 (`main`, `refs/heads/main`, `feature:main`, `+main` 등)
  - 현재 브랜치가 보호 브랜치인 상태에서 인자 없는 `git push` (implicit push)
  - `git reset --hard`
  - `git clean -f / -fd / -fdx`
- 위반 처리:
  - `CanopusError::Tool("policy: …")` 반환
  - `notify_policy_violation()` → Discord 알림 (§4.7 명시 요건)
  - 호출자(`submit`)는 `try_stage!` 흐름에서 `failed`로 stage 기록

### 2.2 상태 전이 — CAS 단일화

- `dysonsphere::FileTaskTable::update_status_if_current(id, expected, next) -> Result<bool>`
- 호출 위치:
  - `ton618`: `Pending → Dispatched`
  - `laniakea` (성공): `Dispatched → PendingReview`
  - `laniakea` (실패): `Dispatched → Failed`
- 효과: `!cancel` / `!reject`로 Failed가 된 task를 worker가 silently 부활시킬 수 없음.

### 2.3 Dispatcher 필터 좁히기

- `ton618/src/file.rs::FileDataSource::fetch` → `matches!(status, TaskStatus::Pending)` 만.
- `!= Processed` 패턴 금지 (PendingProposal/PendingReview/Failed/Dispatched가 모두 잡혀버림).
- PendingProposal 자동 dispatch의 **1차 방어선** (CAS는 2차).

### 2.4 Kepler / Hubble discovery proposal & dedup

- `dysonsphere::discovery`가 공통 `Discovery` trait, FNV-1a ID, `PendingProposal` 등록, seen ledger, Discord 알림을 제공한다.
- **Kepler**: 코드베이스 스캐너. `cargo clippy` 기반 finding을 `TaskMessage { meta.status = PendingProposal }` 로 등록한다.
- **Hubble**: 외부 데이터 collector/scraper. v1 통합 범위에서는 source trait + stub만 유지하고, 첫 RSS/SNS/news source는 별도 PR에서 붙인다.
- `discovery_id`: **FNV-1a 직접 구현** (`DefaultHasher` 금지 — 프로세스/버전 비결정성).
- Ledger:
  - Kepler code finding: `.canopus/kepler/seen.json`
  - Hubble external signal: `.canopus/hubble/seen.json`
  ```json
  {
    "<id>": {
      "first_seen": "...",
      "task_id": "<id>",
      "status": "pending_proposal"
    }
  }
  ```
  - `table.create()` 성공 또는 `table.fetch()` 가 기존 task를 확인한 경우에만 기록.
- 발견마다 Discord 알림: `📝 새 후보 발견: <title> — !propose-approve <id>`.

### 2.5 Discord 측 동시성 (`apps/discord-bot/bot.py`)

- `fcntl.flock` 기반 cross-process lock (`LOCK_EX` / `LOCK_SH`), 락 파일 `<tasks>.lock`.
- 헬퍼: `read_tasks`, `write_tasks`, `append_task_locked`, `update_task_status_locked`.
- 모든 propose/approve/reject/cancel은 단일 헬퍼 `update_task_status_locked` 경유.
- 명령 인벤토리:
  ```
  !run, !approve, !reject,
  !propose-approve, !propose-reject, !cancel, !show,
  !status, !help, !new-project, !register
  ```

### 2.6 Audit log — StageRecord

- `.canopus/runs/<agenda_id>.json` 에 stage 단위 **incremental persist** (성공·실패 모두).
- 통합 후 스키마:
  ```rust
  #[derive(Serialize, Deserialize)]
  pub struct StageRecord {
      pub name: String,
      pub started_at: DateTime<Utc>,   // chrono RFC3339, "unix:<secs>" 폐기
      pub ended_at: DateTime<Utc>,
      pub duration_secs: u64,
      pub status: String,              // "ok" | "failed"
      pub artifacts: Vec<String>,
  }
  ```
- v2 `audit_events` 테이블과 row 단위 1:1 매핑 가능 (§13.1).

---

## 3. v1 §11 안티패턴 ↔ 방어 메커니즘 매트릭스

| §11 항목 | 방어 위치 |
|---|---|
| 1. Pipeline 외부 상태 전이 | `update_status_if_current` (모든 worker/scheduler) |
| 2. ToolGateway 직접 우회 | `check_policy` + `find_git_subcommand` |
| 3. Deny 반환 무시 | global flag 정규화 + 단위 테스트 (`policy_rejects_dangerous_git_commands`) |
| 4. !approve 없는 PR | `watch`가 `Processed`만 pick (기존 로직 유지) |
| 5. Discovery source PendingProposal 우회 | dispatcher 필터 + CAS + `seen.json` |
| 6. main/master 직접 commit | refspec RHS 검사 + 현재 브랜치 검사 (implicit push) |
| 7. 추가 source of truth 금지 | `tasks-{cat}.json` 단일 유지 (`seen.json`은 dedup hint, 큐 아님) |
| 8. LLM prompt에 secret 노출 | LLMAgentRuntime 도입 시 별도 적용 |
| 9. .env / credentials artifact 저장 | LLMAgentRuntime 도입 시 별도 적용 |
| 10. 무한 재시도 | **잔여 — §5 참조** |

---

## 4. 통합 검증 체크리스트

```text
[ ] cargo fmt --all -- --check
[ ] cargo clippy --workspace --all-targets -- -D warnings
[ ] cargo test --workspace
[ ] python3 -m py_compile apps/discord-bot/bot.py

[ ] e2e: kepler scan → PendingProposal 등록 → ton618 dispatcher가 dispatch하지 않음
[ ] e2e: Dispatched task에 !cancel 적용 후 worker 종료 → status=Failed 유지 (부활 X)
[ ] e2e: git -C <path> push --force 시도 → Deny + Discord 알림 도착
[ ] e2e: 보호 브랜치 체크아웃 상태 + 인자 없는 git push → Deny + Discord 알림 도착
[ ] e2e: stage 중간 실패 → runs/<agenda_id>.json 에 status:"failed" 기록 + chrono RFC3339 timestamp
[ ] e2e: kepler 재시작 후 동일 clippy 경고 재스캔 → 새 task 등록되지 않음 (FNV stable)
```

---

## 5. 통합과 별개로 남은 v1 잔여 작업

문서 §12.1 기준, 통합 PR 머지 후 우선순위:

- **LLMAgentRuntime adapter** — 두 브랜치 모두 미구현. v1 P0 본진.
- Laniakea stdout/stderr → `.canopus/logs/<task_id>.log` (§4.4)
- Stage timeout / `max_retries=2` (§11.10)
- Q&A Issue 폴링 timeout + collaborator 검증 (§4.5.4)
- `CanopusPayload` typed wrapper (§5 — payload schema 안정화)

---

## 6. 한 줄 요약

> v1 안전 모델 = **branch의 안티패턴 방어 ⊕ main의 정책 정밀도 ⊕ main의 audit 표준**.
> 이 통합이 완료되어야 LLMAgentRuntime 단계에서도 v2 (`audit_events`, `approval_requests`, `findings`) 로 무손실 마이그레이션이 가능하다.

# Canopus v1 구현 방향 비교 의견

## 결론

`stellaris-ralph-ancient-hejlsberg` 워크트리의 수정사항을 기본 채택하는 방향이 더 적합하다.

이유는 해당 변경이 `docs/stellaris-canopus-architecture-v1.md`가 강조하는 핵심 방향인 **approval-gated automation**, **상태머신 기반 안전 전이**, **Hubble 발견의 proposal화**, **감사 가능한 stage 기록**에 더 가깝기 때문이다.

## 판단 기준

Canopus v1의 핵심은 기능을 많이 붙이는 것이 아니라, 자동화가 다음 흐름을 반드시 지키게 만드는 것이다.

```text
Hubble finding
→ PendingProposal
→ human propose-approve
→ Pending
→ scheduler dispatch
→ worker execution
→ PendingReview
→ human approve/reject
→ PR creation
```

따라서 좋은 구현은 다음을 만족해야 한다.

1. Hubble이 작업을 바로 실행하지 않고 후보로만 등록한다.
2. 사람 승인 없이는 `Pending`이나 PR 생성 단계로 가지 않는다.
3. 취소·거절된 작업을 scheduler/worker가 나중에 덮어쓰지 않는다.
4. ToolGateway가 위험한 git 동작을 차단한다.
5. 각 stage 결과가 `.canopus/runs/<id>.json`에 남아 v2 audit/event 모델로 확장 가능하다.

## 비교 의견

### ralph 쪽이 더 나은 점

- `update_status_if_current()`로 상태 전이를 조건부로 수행한다.
  - `Pending → Dispatched`
  - `Dispatched → PendingReview / Failed`
  - 취소/반려된 작업이 뒤늦게 되살아나는 문제를 줄인다.
- Hubble 발견을 `PendingProposal`로 등록하고 `.canopus/hubble/seen.json`으로 중복 발견을 관리한다.
- Discord `!propose-approve`, `!propose-reject`, `!cancel`, `!show` 흐름이 파일락 기반으로 더 안전하다.
- StageRecord를 단계별로 저장하고 실패 stage도 기록한다.
- `cargo fmt`, `cargo test`, Python compile 검증 결과가 더 안정적이다.

### 내 수정사항에서 가져올 만한 점

- ToolGateway의 git global option 파싱 방향은 더 좋다.
  - 예: `git -C repo push`, `git -c key=value push`, `git --work-tree=... push`
  - 실제 subcommand를 찾아 policy를 적용하는 방식은 ralph 쪽 policy에 이식할 가치가 있다.
- `RunRecord`처럼 run 단위 wrapper를 두는 구조도 v2 migration 관점에서 고려할 수 있다.

## 권장 설계안

ralph 워크트리를 기준으로 병합하되, 다음 보강을 추가한다.

1. **ralph 기본 채택**
   - proposal 승인 흐름
   - conditional status transition
   - Hubble seen ledger
   - stage별 audit 기록
   - 테스트 추가분

2. **ToolGateway 보강**
   - 내 구현의 `find_git_subcommand()` 아이디어를 ralph policy에 통합한다.
   - 이후 문서 기준에 맞춰 `cargo`, `gh`, `git` 하위 명령 allowlist를 더 세밀하게 제한한다.

3. **식별자 정리**
   - Discord task id, Canopus agenda id, artifact task id가 서로 어긋나지 않도록 매핑 규칙을 정한다.
   - `!show <task_id>`가 실제 run record와 artifact를 안정적으로 찾게 한다.

4. **남은 P0 명확화**
   - LLMAgentRuntime은 아직 미구현이므로, 현재 상태를 “v1 기반 안전장치 완성”으로 부르고 “v1 완성”으로 표현하지 않는다.

## 최종 판단

```text
채택 비율: ralph 80%, 내 수정사항 20%
```

ralph 쪽은 Canopus를 단순 자동 실행기가 아니라 **사람 승인과 정책 게이트 뒤에서 움직이는 orchestration kernel**로 만드는 방향에 더 가깝다. 내 수정사항은 일부 유용한 policy parsing 아이디어가 있지만, 전체 구조와 검증 상태는 ralph 쪽이 더 성숙하다.


# 📌 Stellaris Project summary

### ࣪🪐 Project summary
- **Proejct name:** Stellaris
- **Goal:** High-Performance Data Distributed Processing System
- **Architecture:**  
  Scraper(Hubble, Python) → TaskQueue([TON618], Rust) → Worker(Laniakea, Rust) → AI/Processing → Result save(Redis)

---

## 🧩 Component

### 📡 [Hubble] (Scraper)
- Role: Data crawling
- Stack: Python
- 특징: 수집된 데이터를 DB에 저장 (초기에는 JSON 등 사용 가능)

### 🛰️ [TON618] (TaskQueue)
- Role: DB에서 데이터를 읽고, 메시지(Task)를 생성하여 분산 처리
- Stack: Rust
- 메시지 포맷: 초기엔 JSON 사용, 구조 안정 후 ProtoBuf 전환 예정
- 메시지 생성 시점: cron 기반
- 중복 처리 방지: 추후 설계 예정
- Task 구조: `TaskMessage` 등으로 명확히 정의
- 데이터 소스 추상화: `TaskDataSource` 트레잇으로 구현체 교체 가능 (예: JSON, SQLite 등)

### 🌌 [Laniakea] (Worker)
- Role: Task 처리
- Stack: Rust
- Instance name: Use glaxy name (ex: Andromeda, M87)
- Save processing results to 🛢️Redis

---

## 🔗 메시지 플로우

1. Hubble → DB에 데이터 저장
2. TON618 → DB에서 데이터 읽고 TaskMessage 생성
3. Task → Laniakea에 전달
4. 처리 결과는 Redis에 저장

---

## 🧱 Tech stack and configuration

| Component | Stack |
|-----------|------|
| Scraper | Python |
| TaskQueue ([TON618]) | Rust |
| Worker (Laniakea) | Rust |
| 메시지 큐 | RabbitMQ (아직은 내부 큐로 대체) |
| 저장소 | Redis (결과 저장), DB (임시 저장: JSON, SQLite 등) |

---

## 🧠 Design philosophy
- 메시지는 Command가 아닌 Task 개념으로 정리 (주체의 차이)
- 구성 요소 간 경계를 명확히 함
- 모든 구성 요소에 세계관 기반 코드네임 사용
- 작업 구조는 분산형이지만, 구현은 단순함을 유지
- Rust async 이해를 바탕으로 비동기 구조 적극 활용

---

## 📂 GitHub strategy
- GitHub (beginning: mono repo structure)
- GitHub Issues + GitHub Projects
- Commit message  `[hubble]`, `Closes #42` format
- Discord ↔ GitHub Link plan available

---

## 🎋 Git branch strategy
- main: Stay deployable at all times, manage release tags

- feature/: feature/[module/]function name (ex: feature/dysonsphere/task-crud), Delete after merge to main upon completion

- hotfix/: hotfix/[module/]hotfix name (ex: hotfix/ton618/schema-fix), After modification, merge to main + retag

- release/ (Optional): QA, document, and final debugging with release/vX.Y.Z → merge to main + tag

---

## 🔖 Other terminology
- **Session migration:** Documentation of key projects so far as the session moves


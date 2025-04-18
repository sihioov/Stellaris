
# Stellaris 프로젝트 - 세션 마이그레이션 핵심 요약

## 🌌 프로젝트 개요
- **프로젝트명:** Stellaris
- **목표:** 고성능 데이터 분산 처리 시스템
- **전체 아키텍처:**  
  Scraper(Hubble, Python) → TaskQueue([TON618], Rust) → Worker(Laniakea, Rust) → AI/처리 → 결과 저장(Redis)

---

## 🧩 구성 요소

### 📡 Scraper (Hubble)
- 역할: 데이터 수집
- 언어: Python
- 특징: 수집된 데이터를 DB에 저장 (초기에는 JSON 등 사용 가능)

### 🛰️ TaskQueue ([TON618])
- 역할: DB에서 데이터를 읽고, 메시지(Task)를 생성하여 분산 처리
- 언어: Rust
- 메시지 포맷: 초기엔 JSON 사용, 구조 안정 후 ProtoBuf 전환 예정
- 메시지 생성 시점: cron 기반
- 중복 처리 방지: 추후 설계 예정
- Task 구조: `TaskMessage` 등으로 명확히 정의
- 데이터 소스 추상화: `TaskDataSource` 트레잇으로 구현체 교체 가능 (예: JSON, SQLite 등)

### 🌌 Worker (Laniakea)
- 역할: Task 처리
- 언어: Rust
- 각 인스턴스 이름: 은하 이름 사용 (예: Andromeda, M87)
- 처리 결과는 Redis에 저장

---

## 🔗 메시지 플로우

1. Hubble → DB에 데이터 저장
2. TON618 → DB에서 데이터 읽고 TaskMessage 생성
3. Task → Laniakea에 전달
4. 처리 결과는 Redis에 저장

---

## 🧱 기술 스택 및 구성

| 구성 요소 | 기술 |
|-----------|------|
| Scraper | Python |
| TaskQueue ([TON618]) | Rust |
| Worker (Laniakea) | Rust |
| 메시지 큐 | RabbitMQ (아직은 내부 큐로 대체) |
| 저장소 | Redis (결과 저장), DB (임시 저장: JSON, SQLite 등) |

---

## 🧠 설계 철학
- 메시지는 Command가 아닌 Task 개념으로 정리
- 구성 요소 간 경계를 명확히 함
- 모든 구성 요소에 세계관 기반 코드네임 사용
- 작업 구조는 분산형이지만, 구현은 단순함을 유지
- Rust async 이해를 바탕으로 비동기 구조 적극 활용

---

## 📂 GitHub / 협업 전략
- GitHub 사용 (초기: mono repo 구성)
- GitHub Issues + GitHub Projects + Notion 조합으로 프로젝트 관리
- 커밋 메시지에 `[hubble]`, `Closes #42` 등의 형식 사용
- Discord ↔ GitHub 연동 계획 있음

---

## 🔖 기타 용어 정리
- **세션 마이그레이션:** 세션 이동 시 지금까지의 핵심 프로젝트 내용을 요약해 문서화하는 것

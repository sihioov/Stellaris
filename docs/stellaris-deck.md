stellaris/                         ← 🛰️ Monorepo Root (Cargo workspace)
├── Cargo.toml                     ← Workspace 설정 [구현됨]
│
├── dysonsphere/                   ← ☀️ 공통 유틸리티 및 공유 구조체 모듈 [구현됨]
│   ├── src/
│   │   ├── lib.rs                 ← 라이브러리 진입점 [구현됨]
│   │   ├── message.rs             ← ✅ TaskMessage 정의 (전 모듈 공용) [구현됨]
│   │   ├── status.rs              ← 작업 상태 및 상태 코드 정의 [구현됨]
│   │   ├── error.rs               ← 공통 에러 타입 [구현됨]
│   │   └── db/
│   │       ├── mod.rs             ← DB 모듈 정의 [구현됨]
│   │       ├── task_table.rs      ← Task 관련 CRUD 인터페이스 [구현됨]
│   │       └── task_table_file.rs ← 파일 기반 TaskTable 구현 [구현됨]
│   └── Cargo.toml                 ← dysonsphere 크레이트 설정 [구현됨]
│
├── ton618/                        ← 🌀 TaskQueue (Producer/Dispatcher) [개발 진행 중]
│   ├── src/
│   │   ├── main.rs                ← 진입점, 기본 폴링 루프 구현 [구현됨]
│   │   ├── scheduler/             ← 스케줄러 관련 구현 [개발 진행 중]
│   │   │   ├── mod.rs             ← 스케줄러 모듈 정의 [구현됨]
│   │   │   ├── job.rs             ← Job 트레잇 정의 [구현됨]
│   │   │   ├── schedule.rs        ← 스케줄 계산 로직 [구현됨]
│   │   │   ├── queue.rs           ← 우선순위 큐 구현 (ScheduledJob) [구현됨]
│   │   │   └── runner.rs          ← 스케줄러 실행 로직 [구현됨]
│   │   ├── datasource.rs          ← TaskDataSource 트레잇 정의 [구현됨]
│   │   ├── file.rs                ← 파일 기반 DataSource 구현 [구현됨]
│   │   ├── schedule.rs            ← 단순 스케줄링 로직 [구현됨]
│   │   ├── rdb/                   ← 관계형 DB 연결 모듈 [초기 구조만 구현]
│   │   │   ├── mod.rs             ← 모듈 정의 [구현됨]
│   │   │   ├── sqlite.rs          ← SQLite 어댑터 [계획됨]
│   │   │   ├── postgres.rs        ← PostgreSQL 어댑터 [계획됨]
│   │   │   └── rdb_datasource.rs  ← RDB 기반 DataSource [계획됨]
│   │   └── nosql/                 ← NoSQL DB 연결 모듈 [초기 구조만 구현]
│   │       ├── mod.rs             ← 모듈 정의 [구현됨]
│   │       └── mongo.rs           ← MongoDB 어댑터 [계획됨]
│   └── Cargo.toml                 ← ton618 크레이트 설정 [구현됨]
│
├── laniakea/                      ← 🌌 Worker (실제 처리기) [초기 단계]
│   ├── src/
│   │   ├── main.rs                ← 기본 진입점 [구현됨]
│   │   ├── processor.rs           ← Task 처리 로직 [계획됨]
│   │   └── task_queue.rs          ← 내부 큐 [계획됨]
│   └── Cargo.toml                 ← laniakea 크레이트 설정 [구현됨]
│
├── hubble/                        ← 🔭 SpaceProbe - 외부 데이터 수집기 [초기 단계]
│   ├── src/                       ← Rust 기반으로 구현 [초기 구조만 구현]
│   │   └── main.rs                ← 기본 진입점 [구현됨]
│   └── Cargo.toml                 ← hubble 크레이트 설정 [구현됨]
│
├── apps/
│   └── canopus/                 ← portable AI development orchestration layer [MVP]
│
├── docs/                          ← 📚 프로젝트 문서 [구현됨]
│   ├── architecture.md            ← 현재 프로젝트 아키텍처 기준 [구현됨]
│   ├── stellaris-deck.md          ← 디렉토리 구조 문서 [구현됨]
│   ├── commit.md                  ← 커밋 메시지 가이드 [구현됨]
│   └── snippet.md                 ← 코드 스니펫 모음 [구현됨]
│
└── README.md                      ← 프로젝트 루트 문서 [업데이트 필요]

---

## 🔹 현재 개발 진행 상황

1. **dysonsphere** - ✅ 기본 핵심 구조체 및 인터페이스 구현 완료
   - TaskMessage, TaskStatus, 공통 에러 핸들링
   - 파일 기반 TaskTable 구현

2. **ton618** - 🔄 개발 진행 중
   - 기본 작업 큐 로직 및 파일 기반 데이터 소스 구현 완료
   - 고급 스케줄링 기능 구현 중 (우선순위 큐 기반)
   - DB 연결 모듈 초기 구조 구현

3. **laniakea** - 🚧 초기 구조만 구현
   - 작업 처리 로직 계획 중

4. **hubble** - 🚧 초기 구조만 구현
   - Rust 기반으로 변경 (기존 Python 계획에서)

## 🔹 다음 개발 단계

1. ton618의 스케줄러 모듈을 메인 로직과 통합
2. 관계형 DB 연결 구현 (SQLite 우선)
3. laniakea 작업 처리 로직 구현
4. 전체 시스템 통합 테스트

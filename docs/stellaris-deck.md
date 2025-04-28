stellaris/                         ← 🛰️ Monorepo Root (Cargo workspace)
├── Cargo.toml                     ← Workspace 설정
│
├── dysonsphere/                   ← ☀️ 공통 유틸리티 및 공유 구조체 모듈
│   ├── src/
│   │   ├── lib.rs                 ← 라이브러리 진입점
│   │   ├── message.rs             ← ✅ TaskMessage 정의 (전 모듈 공용)
│   │   ├── status.rs              ← 작업 상태 및 상태 코드 정의
│   │   ├── config.rs              ← 환경 설정 로딩
│   │   ├── error.rs               ← 공통 에러 타입
│   │   └── db/
│   │       ├── mod.rs
│   │       ├── task_table.rs      ← Task 관련 CRUD
│   │       └── schema.rs          ← SQL 정의 (Diesel 등 ORM)
│   └── Cargo.toml                 ← dysonsphere 크레이트 설정
│
├── ton618/                        ← 🌀 TaskQueue (Producer/Dispatcher)
│   ├── src/
│   │   ├── main.rs                ← 진입점, scheduler 및 dispatcher 실행
│   │   ├── scheduler/             ← 스케줄러 관련 구현
│   │   │   ├── mod.rs             ← 스케줄러 모듈 정의
│   │   │   ├── job.rs             ← Job 트레잇 정의
│   │   │   ├── schedule.rs        ← 스케줄 계산 로직
│   │   │   ├── queue.rs           ← 우선순위 큐 구현 (ScheduledJob)
│   │   │   └── runner.rs          ← 스케줄러 실행 로직
│   │   ├── datasource.rs          ← TaskDataSource 트레잇 정의 및 구현체
│   │   ├── file.rs                ← 파일 기반 Task 저장소 구현
│   │   └── task/
│   │       ├── mod.rs
│   │       ├── dispatcher.rs      ← TaskDispatcher 트레잇 및 구현
│   │       ├── generator.rs       ← TaskMessage 생성기 (e.g., TaskGen)
│   │       └── queue.rs           ← InMemory/FIFO 큐
│   └── Cargo.toml                 ← ton618 크레이트 설정
│
├── laniakea/                      ← 🌌 Worker (실제 처리기)
│   ├── src/
│   │   ├── main.rs                ← 워커 실행 루프
│   │   ├── processor.rs           ← Task 처리 로직
│   │   └── task_queue.rs          ← 내부 큐 (필요시 FIFO 등)
│   └── Cargo.toml                 ← laniakea 크레이트 설정
│
├── hubble/                        ← 🔭 SpaceProbe (Python) - 외부 데이터 수집기
│   ├── __init__.py
│   ├── main.py                    ← 크롤러/스크래퍼 실행
│   └── api/
│       └── newsapi.py             ← 예: NewsAPI를 이용한 데이터 수집기
│
├── tests/                         ← 통합 테스트 및 단위 테스트
└── dist/                          ← 결과물 또는 압축 파일 등 배포 디렉토리

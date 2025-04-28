# Stellaris 프로젝트 Rule
- 해당 프로젝트는 우주 컨셉을 가진 고성능 분산 처리 시스템이다.
- 따라서 각 모듈이나 컨셉은 우주컨셉 및 용어를 따른다

## 목적 
- 이 규칙 세트는 Stellaris 프로젝트의 대한 정리 및 AI 답변에 관련한 규칙이다

## 프로젝트 구성
### [Hubble](scrapper)
- 이름 유래: 최초 우주 탐사 개
- 역할: 뉴스, SNS등 외부 데이터 수집
- 언어: python
- 동작
  - news api, sns 데이터 등등을 스크래핑, 크롤링을 하여 데이터로 저장
  - 저장 포맷은 TaskMessage 형태로 구조화
- 향후 확장
  - 여러 Telescope 인스턴스를 동시에 띄워 다양한 데이터 소스 대응
  - 각 인스턴스마다 hubble 같은 기존에 실제로 존재하는 우주 탐사선을 인스턴스명으로 채용

### [TON618](taskQueue)
- 역할: DB/파일에서 수집된 데이터를 읽어 TaskMessage 생성 및 분배
- 언어: Rust
- 구조
  - Job기반 구조 도입
  - TaskMessage 정의
- 추후
  - TaskSender(이름 미정) 트레잇 설계 예정 (MQ 전환을 위한 구조 대비)

### [Laniakea](worker)
- 역할: TaskMessage 처리
- 언어: Rust
- 구조
  - 각 Worker는 갤럭시 이름으로 명명됨 (예: Andromeda, M87)
  - TaskMessage를 받아서 수행
  - 처리 결과는 **Redis**에 받아서 저장(Redis cache 기능도 사용 예정)
- 향후
  - 여러 Laniakea 인스턴스를 동시에 실행 가능
  - Job 구조 도입시에 확장가능

### [DysonSphere](common)
- 역할: 공통 모듈
- 언어: Rust
- 주요 구성
  - TaskMessage, TaskType, TaskStatus
  - TaskDataSource 트레잇
  - config.rs, error.rs 공통 유틸 포함

## 코드 구조
```
stellaris/                      🛰️ monorepo root
├── Cargo.toml                  [workspace]
│
├── dysonsphere/               ☀️ 공통 구조 (TaskMessage, 설정, 데이터소스)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             ← 공용 진입점
│       ├── message.rs         ← TaskMessage, TaskType, TaskStatus
│       ├── config.rs          ← 설정 로딩 (env, toml)
│       ├── error.rs           ← 공용 에러 처리
│       └── db/
│           ├── mod.rs
│           ├── task_table.rs  ← FileTaskTable (JSON 기반)
│           └── datasource.rs  ← TaskDataSource 트레잇
│
├── ton618/                    🛰️ TaskQueue 모듈 (Scheduler)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            ← Job 등록 및 런타임 시작
│       └── job/               ← 스케줄링 관련 전체 구조 집중
│           ├── mod.rs         ← pub use self::{traits, schedule, queue, runner}
│           ├── traits.rs      ← Job 트레잇, JobResult
│           ├── schedule.rs    ← FixedSchedule, CronSchedule
│           ├── queue.rs       ← JobQueue (MinHeap 기반) 
│           ├── runner.rs      ← Runner loop               
│           └── task_gen.rs    ← TaskGenJob                
│
├── laniakea/                  🌌 Worker 모듈 (작업 처리기)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            ← TaskMessage 수신 및 처리
│       └── (구현 예정)        ← TaskWorker, Redis 저장기
│
└── hubble/                    📡 Scraper (뉴스, 외부 데이터 수집)
    ├── requirements.txt
    └── hubble/
        ├── __init__.py
        ├── main.py            ← TaskMessage 포맷으로 JSON 저장
        └── datasource.py      ← 수집 대상 처리

```

## 답변 규칙
- 질문자는 Rust 초보이지만 C++는 꽤 익숙한 상태를 인지
- Rust 프로그래밍에 대한 질문을 할때는 질문자가 이해를 못했다고 생각될 땐 C++와 비교하여 설명
- 설계를 진행할때와 코드를 구현할때의 질문을 잘 구별
- 기본 설계를 원칙으로 하대 "스캐폴드를 짜줘" or "구현해줘" 같은 직접적인 요청이 있을때 코드 구현으로 들어가야 함함
- 설계적으로 좀 더 좋은 규칙이나 안좋은 구조 또는 설계에 대한 얘기를 질문자가 할경우 적극적인 의견 피력
- 구현되어 있는 부분 안되어 있는 부분 인지

## 코드 생성 규칙
- 중요 내용이나 기타 개인 의견이 필요한 곳에 주석 형태로 의견 제시
- 주석으로 내용 전달할시 이모지는 넣지 않음


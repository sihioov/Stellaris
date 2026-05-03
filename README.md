# 🌌 Stellaris: 고성능 분산 데이터 처리 시스템

<p align="center">
  <img src="docs/stellaris.drawio" alt="Stellaris Architecture" width="600" />
</p>

## 📋 프로젝트 소개

Stellaris는 대규모 데이터를 효율적으로 수집, 처리, 분석하기 위한 고성능 분산 처리 시스템입니다. 우주적 현상과 구조물에서 영감을 받은 모듈 구조로 설계되었으며, 확장성과 유연성을 핵심 가치로 합니다. 복잡한 데이터 처리 워크플로우를 단순화하고 자동화하는 것이 주요 목표입니다.

### 🚀 핵심 설계 원칙

- **명확한 책임 분리**: 각 모듈은 단일 책임 원칙을 따라 설계
- **느슨한 결합도**: 모듈 간 의존성 최소화를 통한 유연한 확장
- **공통 인터페이스**: 표준화된 메시지 형식과 트레잇(trait) 기반 추상화
- **성능 최적화**: 메모리 안전성과 동시성을 보장하는 Rust 언어 사용
- **비동기 처리**: Tokio 기반 비동기 프로그래밍으로 높은 처리량 확보

## 🧩 주요 구성요소 및 아키텍처

### 1. 🔭 Hubble (데이터 수집기)

**목적**: 외부 소스로부터 데이터를 수집하고 가공하여 중앙 저장소에 저장

**주요 기능**:
- 다양한 외부 API에서 데이터 수집 (뉴스 API, 날씨 데이터, 소셜 미디어 등)
- 웹 페이지 크롤링 및 데이터 추출
- 수집 데이터의 기본 검증 및 정규화
- 수집 주기 관리 및 중복 데이터 처리

**아키텍처**:
- 플러그인 방식의 데이터 소스 어댑터
- 비동기 HTTP 클라이언트로 효율적인 네트워크 요청 처리
- 확장 가능한 데이터 추출기(Extractor) 시스템

### 2. 🌀 TON618 (작업 큐 및 스케줄러)

**목적**: 저장소의 데이터를 기반으로 작업 메시지를 생성하고 처리 작업을 스케줄링

**주요 기능**:
- 다양한 데이터 소스(파일, DB)로부터 작업 데이터 수집
- 우선순위 기반 작업 스케줄링
- 작업 상태 관리 및 실패 작업 재시도
- 작업 처리 분산 및 로드 밸런싱

**아키텍처**:
- `TaskDataSource` 트레잇: 다양한 데이터 소스 추상화
- 고급 스케줄링 엔진:
  - 우선순위 큐 기반 작업 관리
  - `Schedule` 타입으로 고정 간격 및 Cron 표현식 지원
  - `Job` 트레잇으로 다양한 작업 타입 정의
- 장애 복구 메커니즘

### 3. 🌌 Laniakea (작업 처리기)

**목적**: TON618로부터 받은 작업을 실제로 처리하고 결과를 저장

**주요 기능**:
- 작업 메시지 수신 및 역직렬화
- 작업 타입별 전문 처리기(Handler) 실행
- 처리 결과 집계 및 저장
- 처리 상태 모니터링 및 오류 보고

**아키텍처**:
- 플러그인 방식의 처리기 시스템
- 병렬 작업 처리를 위한 워커 풀
- 결과 캐싱 및 중복 제거

### 4. ☀️ Dysonsphere (공통 코어 라이브러리)

**목적**: 모든 모듈에서 공유하는 핵심 구조체, 인터페이스, 유틸리티 제공

**주요 기능**:
- `TaskMessage`: 모든 모듈 간 통신의 기본 단위
- `TaskStatus`: 작업 상태 관리 (Pending, Processed, Failed)
- `TaskTable`: 작업 데이터 저장소 인터페이스
- 공통 오류 처리 및 결과 타입

**아키텍처**:
- 최소한의 외부 의존성
- 명확한 공용 인터페이스 정의
- 다양한 구현체 지원을 위한 트레잇 기반 설계

### Canopus (AI development orchestration layer)

Canopus is an application layer built on top of Stellaris. It keeps AI development orchestration outside the core engine by using ports and adapters for task backends, agent runtimes, tool gateways, artifact storage, and intake surfaces.

The first Canopus milestone is a Local Patch MVP: a CLI request creates a local branch, simulates bounded agent work, runs local checks, and stores plan, diff, test, and review artifacts without pushing or creating a PR.

## 🔄 데이터 흐름 및 작업 처리 과정

1. **데이터 수집 단계**:
   - Hubble이 외부 소스로부터 원시 데이터 수집
   - 데이터 정규화 및 기본 검증 후 저장소에 저장

2. **작업 생성 단계**:
   - TON618이 저장소를 주기적으로 스캔하여 처리할 데이터 확인
   - 데이터를 기반으로 `TaskMessage` 생성
   - 작업 우선순위 및 스케줄에 따라 큐에 배치

3. **작업 처리 단계**:
   - Laniakea가 큐에서 작업을 가져와 적절한 처리기로 라우팅
   - 작업 처리 및 결과 생성
   - 처리 결과 및 상태 업데이트

4. **결과 저장 단계**:
   - 처리된 결과 Redis 등의 고성능 저장소에 저장
   - 작업 완료 상태 업데이트
   - 필요시 통계 및 메트릭 수집

## 🛠️ 기술 스택 및 구현 세부사항

### 프로그래밍 언어 및 프레임워크
- **Rust**: 메모리 안전성, 성능, 동시성을 위한 주력 언어
- **Tokio**: 비동기 런타임으로 높은 동시성 처리
- **Serde**: 데이터 직렬화/역직렬화

### 스토리지 및 데이터 관리
- **초기 구현**: JSON 파일 기반 스토리지
- **확장 계획**:
  - **SQLite**: 개발 및 소규모 배포용
  - **PostgreSQL**: 고성능 관계형 데이터베이스
  - **MongoDB**: 유연한 스키마의 NoSQL 데이터베이스
  - **Redis**: 빠른 결과 캐싱 및 임시 저장

### 핵심 라이브러리
- **cron**: 작업 일정 관리
- **chrono**: 날짜 및 시간 처리
- **async-trait**: 비동기 트레잇 정의
- **anyhow**: 에러 처리

## 📚 프로젝트 문서

자세한 내용은 다음 문서를 참고하세요:
- [프로젝트 개요 및 아키텍처](docs/architecture.md)
- [Canopus v1 앱 설계](docs/canopus-v1.md)
- [Stellaris v2 에이전트 협업 방향](docs/stellaris-v2-agent-collaboration.md)
- [모듈별 디렉토리 구조](docs/stellaris-deck.md)
- [개발 가이드라인](docs/commit.md)
- [코드 스니펫 및 사용 예제](docs/snippet.md)

## 📋 시작하기

### 필수 요구사항
- Rust (1.65+)
- Cargo

### 빌드 및 실행
```bash
# 전체 워크스페이스 빌드
cargo build

# TON618 모듈 단독 실행
cargo run -p ton618
```

## 📝 개발 현황 및 계획

- [x] 기본 아키텍처 설계
- [x] 공통 라이브러리(Dysonsphere) 개발
- [x] 파일 기반 데이터 소스 구현
- [ ] 우선순위 큐 기반 스케줄러 통합
- [ ] 데이터베이스 연결 구현
- [ ] 작업 처리기 구현
- [ ] 시스템 통합 테스트

## 📚 문서

자세한 내용은 다음 문서를 참고하세요:
- [프로젝트 개요](docs/architecture.md)
- [디렉토리 구조](docs/stellaris-deck.md)
- [커밋 메시지 가이드](docs/commit.md)
- [코드 스니펫](docs/snippet.md)

## 👤 기여자

- 개발자명 / 연락처

## 📄 라이선스

이 프로젝트는 MIT 라이선스 하에 배포됩니다.

## Rust feature matrix

The Phase 1 Rust lifecycle checks are expected to pass in three profiles:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --no-default-features
cargo test --workspace --no-default-features
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
cargo build -p ton618 --no-default-features --features file-dispatch
```

`ton618` reserved modules are feature-gated: `file-dispatch` (default),
`rabbitmq-dispatch`, `scheduler-cron`, `rdb`, and `nosql`.

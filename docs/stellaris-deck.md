```
stellaris/                         ← 🛰️ monorepo root
├── Cargo.toml                     ← [workspace] 정의
│
├── dysonsphere/                   ← ☀️ 공통 핵심 모듈 (TaskMessage, DB, 설정 등)
│   ├── Cargo.toml
│   └── src/
│       ├── message.rs             ← ✅ TaskMessage 정의 (전 모듈 공유)
│       ├── db/
│       │   ├── mod.rs
│       │   ├── task_table.rs      ← 공통 Task CRUD
│       │   └── schema.rs          ← SQL 정의용 (선택)
│       ├── config.rs              ← 설정 로딩 (toml/env)
│       └── error.rs               ← 공통 에러 타입
│
├── ton618/                        ← 🛰️ TaskQueue Agent (Producer)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                ← 주기 실행 루프
│       ├── datasource.rs          ← TaskDataSource 트레잇 정의
│       ├── file.rs                ← FileDataSource 구현
│
│       ├── rdb/
│       │   ├── mod.rs
│       │   ├── rdb_datasource.rs  ← RDBDataSource + RDBEngine + impl
│       │   ├── sqlite.rs          ← SQLiteAdapter
│       │   └── postgres.rs        ← PostgresAdapter (stub)
│
│       ├── nosql/                 ← (계획 중)
│       │   ├── mod.rs
│       │   ├── mongo.rs
│       │   └── dynamo.rs
│
│       ├── config.rs              ← 설정 로딩 (선택 사항)
│       └── scheduler.rs           ← 주기 실행 제어 (선택 사항)
│
├── laniakea/                      ← 💫 Worker (Consumer)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       └── processor.rs           ← TaskMessage 처리
│
├── hubble/                        ← 🔭 Scraper (Python 또는 Rust)
│   ├── pyproject.toml / Cargo.toml
│   └── main.py or main.rs
```
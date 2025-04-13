```
stellaris/                         ← 🛰️ monorepo root
├── Cargo.toml                     ← [workspace] define
│
├── dysonsphere/                   ← ☀️ common core module (TaskMessage, DB, 설정 등)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 ← Entry point
│       ├── message.rs             ← ✅ TaskMessage define (Shared whole project)
│       ├── db/                     
│       │   ├── mod.rs
│       │   ├── task_table.rs      ← Common task CRUD
│       │   └── schema.rs          ← SQL define
│       ├── config.rs              ← Config loading (toml/env)
│       └── error.rs               ← Common error type
│
├── ton618/                        ← 🛰️ TaskQueue Agent (Producer)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                ← Run loop
│       ├── datasource.rs          ← Define trait TaskDataSource
│       ├── file.rs                ← FileDataSource Implementation
│
│       ├── rdb/
│       │   ├── mod.rs
│       │   ├── rdb_datasource.rs  ← RDBDataSource + RDBEngine + impl
│       │   ├── sqlite.rs          ← SQLiteAdapter
│       │   └── postgres.rs        ← PostgresAdapter (stub)
│
│       ├── nosql/                 ← (Todo)
│       │   ├── mod.rs
│       │   ├── mongo.rs
│       │   └── dynamo.rs
│
│       ├── config.rs              ← Config loading (Option)
│       └── scheduler.rs           ← Run control (Option)
│
├── laniakea/                      ← 💫 Worker (Consumer)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       └── processor.rs           ← TaskMessage processing
│
├── hubble/                        ← 🔭 Scraper (Python or Rust)
│   ├── pyproject.toml / Cargo.toml
│   └── main.py or main.rs
```
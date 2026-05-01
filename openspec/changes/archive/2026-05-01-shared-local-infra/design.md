## Context

`pm` 의 현재 포트 관리(`src/commands/ports.rs`, `src/models.rs`)는 5개의 `PortKind`(Frontend, Backend, Database, Redis, Infra)에 대해 프로젝트마다 별도 포트를 할당한다. 로컬 개발에서 프로젝트 N개는 곧 Postgres N개, Redis N개를 의미하며, docker-compose 스택이 비대해지고 메모리도 폭증한다.

Postgres 는 인스턴스 하나 안에 다수의 데이터베이스를 둘 수 있고, Redis 도 key prefix 컨벤션으로 데이터를 분리할 수 있다. 따라서 로컬 환경에서는 Database/Redis 를 "공용 인프라"로 분리하고, 프로젝트별로는 식별자(db name / key prefix)만 다르게 주입하는 모델이 자연스럽다.

제약:
- `~/.config/pm/ports.json` 의 schema version 변경이 필요하며, 사용자 환경에 이미 v1 데이터가 존재한다.
- `pm` 는 외부 컨테이너(Postgres/Redis)를 직접 띄우지 않는다. 사용자가 docker-compose 등으로 띄운 인스턴스의 포트만 등록한다.
- Project name 은 workspace 내에서만 unique 이므로 DB name 은 workspace prefix 가 필요.

## Goals / Non-Goals

**Goals:**
- Database/Redis 를 단일 공용 인스턴스로 사용하면서 프로젝트별 데이터 격리를 보장한다.
- 기존 v1 `ports.json` 사용자가 명령 실행만으로 자동 마이그레이션되도록 한다.
- `pm run` 의 환경 변수 인터페이스를 호환 가능한 범위에서 유지한다 (`LOCAL_POSTGRES_PORT`, `DATABASE_URL`, `REDIS_URL` 등은 이름은 그대로, 가리키는 포트만 공용으로 변경).

**Non-Goals:**
- Postgres/Redis 컨테이너의 자동 기동/관리 (docker-compose 통합).
- 운영(prod) 환경의 포트 모델 변경. 본 변경은 로컬 개발 한정.
- Redis 논리 DB 번호(`SELECT`)를 이용한 격리. 16개 한계로 확장성이 없음.
- `pm` 가 Postgres 의 데이터베이스 자체를 `CREATE DATABASE` 로 생성/삭제하는 기능. 이번 change 의 범위는 "이름 규칙 정의 + 환경 변수 주입" 까지.

## Decisions

### D1. 공용 인프라는 `PortsData.shared` 신규 섹션에 둔다
```rust
pub struct PortsData {
    pub version: u32,            // 2
    pub shared: SharedInfra,     // 신규
    pub ranges: HashMap<PortKind, PortRange>,
    pub projects: HashMap<String, PortProject>,
}

pub struct SharedInfra {
    pub postgres_port: u16,      // 기본 5432
    pub redis_port: u16,         // 기본 6379
}
```
**Why**: `projects` 와 동급의 최상위 필드로 두면 마이그레이션·시리얼라이즈가 단순하고, 향후 다른 공용 서비스(Mongo 등)를 추가하기 쉽다. **Alternative**: 각 프로젝트의 `services` 안에 가상 "shared" 서비스를 넣는 방식 — 거부 (개념상 공용인데 프로젝트에 묶이는 게 어색).

### D2. `PortKind::Database` / `Redis` enum 자체는 유지하되, "shared kind" 로 재정의
- `PortKind::is_shared(self) -> bool` 추가, Database/Redis 만 true.
- `pm ports assign` 은 shared kind 가 들어오면 에러 + 안내 메시지 출력 (`pm ports shared` 사용 유도).
- `pm ports list` 출력에서는 shared 섹션이 별도 헤더로 표시.

**Why**: enum 제거는 `serde` 호환성 + CLI `value_enum` 안정성을 깨뜨린다. **Alternative**: enum 분할 (`PerProjectKind` / `SharedKind`) — 거부 (변경 폭이 큰 데 비해 이득이 적음).

### D3. v1 → v2 마이그레이션
- `load_ports()` 가 raw JSON 을 먼저 읽어 `version` 검사.
- `version == 1` 또는 필드 부재 시:
  1. `~/.config/pm/ports.json.bak.v1` 으로 원본 백업.
  2. 모든 프로젝트의 `services` 에서 `kind == Database || Redis` 인 항목 제거.
  3. `shared.postgres_port = 5432`, `shared.redis_port = 6379` 기본값 주입.
  4. `version = 2` 로 저장.
- 사용자에게 `pm ports` 다음 실행 시 stderr 로 1회 안내 출력.

**Why**: 자동·idempotent 마이그레이션이 사용자 마찰을 최소화한다. 백업 파일은 디버그/롤백 경로.

### D4. DB name 정규화
- 신규 함수 `local_database_name(workspace, project) -> String`.
- 규칙: `format!("{workspace}_{project}_local")` 후 `[a-z0-9_]` 외 문자는 `_` 로 치환, 소문자화.
- 예: workspace=`work`, project=`my-app` → `work_my_app_local`.

**Why**: Postgres identifier 안전 + workspace 차원 충돌 방지. **Alternative**: 해시 prefix — 거부 (가독성 악화).

### D5. Redis 격리는 key prefix 컨벤션으로 위임
- `pm run` 이 `REDIS_KEY_PREFIX={workspace}:{project}` 환경 변수를 추가 주입.
- 적용 책임은 애플리케이션 레이어. `pm` 자체는 Redis 와 통신하지 않는다.

**Why**: Redis 자체에 namespace 개념이 없고, 논리 DB 는 16개로 부족. 산업 표준 패턴. **Trade-off**: 앱 측 코드 변경이 필요. README 에 명시 필요.

### D6. 신규 명령 `pm ports shared`
```
pm ports shared                          # 현재 값 출력
pm ports shared --postgres 5433          # postgres_port 만 변경
pm ports shared --redis 6380             # redis_port 만 변경
pm ports shared --postgres 5433 --redis 6380
```
**Why**: 기존 `assign/release/lock` 모델은 per-project 전용이라 공용 인프라 조작에 안 맞음. 별도 서브커맨드가 명료.

### D7. `pm ports assign` 의 기본 kinds 변경
- 변경 전: `[Backend, Database, Redis]`.
- 변경 후: `[Backend]`.
- 사용자가 `--kind frontend` / `--kind infra` 를 명시하면 추가 할당.

**Why**: shared kind 는 더 이상 per-project 가 아니므로 기본값에서 제외. Backend 만 거의 모든 프로젝트가 필요로 함.

## Risks / Trade-offs

- **Risk**: 사용자가 v1 `ports.json` 을 직접 편집하던 경우 마이그레이션이 비표준 데이터를 잃을 수 있음.
  → **Mitigation**: `ports.json.bak.v1` 자동 백업 + 첫 실행 시 stderr 안내.
- **Risk**: `REDIS_KEY_PREFIX` 컨벤션을 앱이 따르지 않으면 다른 프로젝트 키와 충돌.
  → **Mitigation**: README 에 명시. 충돌은 앱 책임 — `pm` 가 강제할 수 없음.
- **Risk**: 동일 호스트에서 여러 워크스페이스가 같은 db name 으로 충돌할 가능성.
  → **Mitigation**: D4 의 workspace prefix 정규화로 해결. 단, workspace 이름 자체에 `_local` 같은 서픽스가 들어가도 충돌은 없음 (concat 후 정규화).
- **Trade-off**: Postgres 의 권한 분리(roles per project)는 다루지 않음. 모든 앱이 `postgres` 슈퍼유저로 접속. 로컬 한정이라 수용.

## Migration Plan

1. `cargo build` 후 첫 `pm ports` 실행 시 v1→v2 자동 마이그레이션 트리거.
2. README/README-ko 갱신: docker-compose 예시(단일 Postgres + Redis), `pm ports shared` 사용법, `REDIS_KEY_PREFIX` 컨벤션.
3. **Rollback**: `~/.config/pm/ports.json.bak.v1` 을 `ports.json` 으로 복원 + 이전 버전 바이너리 사용. v2 데이터는 Database/Redis 정보를 잃었으므로 재할당 필요.

## Open Questions

- `pm ports shared` 가 변경 시 실제 포트 가용성을 검사할지(`TcpListener::bind`)? — Yes 권장. 사용자가 잘못된 포트를 등록하면 모든 `pm run` 이 깨짐.
- Frontend/Infra 도 shared 화 필요한지? — 본 change 범위 외. 별도 제안에서 다룸.

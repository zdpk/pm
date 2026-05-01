## Why

현재 `pm ports` 는 모든 프로젝트에 Database/Redis 포트를 개별 할당한다. 프로젝트 수가 늘어날수록 로컬에서 띄워야 할 Postgres/Redis 컨테이너가 1:1로 비례 증가해 메모리·관리 비용이 폭증한다. 로컬 개발에서는 사실상 단일 Postgres/Redis 인스턴스를 공유하면서 데이터만 격리하면 충분하다.

## What Changes

- **BREAKING**: `PortKind::Database`, `PortKind::Redis` 를 per-project 포트 할당 대상에서 제외한다. `pm ports assign` 의 기본 kinds 도 `Backend` 만 남긴다 (Frontend/Infra 는 명시적 요청 시).
- `PortsData` 에 신규 `shared` 섹션 추가: `postgres_port` (기본 5432), `redis_port` (기본 6379). `~/.config/pm/ports.json` 의 schema version 을 2로 올리고 v1→v2 마이그레이션을 수행한다 (기존 per-project Database/Redis 엔트리 제거).
- `pm run` 의 환경 변수 주입을 변경:
  - `LOCAL_POSTGRES_PORT` = `shared.postgres_port`
  - `DATABASE_URL` = `postgres://postgres:postgres@127.0.0.1:{shared.postgres_port}/{workspace}_{project}_local` (workspace prefix 로 글로벌 유일성 확보)
  - `LOCAL_REDIS_PORT` = `shared.redis_port`
  - `REDIS_URL` = `redis://127.0.0.1:{shared.redis_port}`
  - `REDIS_KEY_PREFIX` = `{workspace}:{project}` (앱 측 prefix 컨벤션용)
- `pm ports list/check` 출력에서 공용 인프라 정보를 별도 섹션으로 표시한다.
- 신규 명령 `pm ports shared [--postgres <port>] [--redis <port>]` 로 공용 포트를 조회/변경할 수 있다.
- `db name` 정규화 함수가 workspace prefix 를 포함하도록 수정한다 (기존 `local_database_name` 은 project 만 사용했음).

## Capabilities

### New Capabilities
- `local-port-management`: 로컬 개발용 포트 할당과 공용 인프라(Postgres/Redis) 공유, 그리고 `pm run` 환경 변수 주입을 포괄하는 capability. 현재 코드는 존재하지만 spec 이 없으므로 이번 change 에서 최초로 명시한다.

### Modified Capabilities
<!-- 비어 있음: openspec/specs/ 가 비어 있어 수정 대상 spec 이 없음 -->

## Impact

- **Code**:
  - `src/models.rs` — `PortsData` 에 `shared: SharedInfra` 필드 추가, `PortKind` 의 역할 재정의
  - `src/config.rs` — v1→v2 마이그레이션 로직 (`load_ports`)
  - `src/commands/ports.rs` — `assign` 기본 kinds 변경, Database/Redis 거부, `shared` 서브커맨드 추가, `list/check` 출력 변경
  - `src/commands/run.rs` — `build_port_env` 가 shared 포트 + workspace prefix db name 사용
  - `src/cli.rs` — `PortsCommand::Shared` 추가
- **Config 파일**: `~/.config/pm/ports.json` 의 스키마 버전 변경. 사용자가 직접 편집한 경우 마이그레이션이 데이터 손실을 일으킬 수 있으므로 백업 처리가 필요.
- **사용자 워크플로우**: 기존 `pm ports assign --kind database` / `--kind redis` 호출은 에러를 반환. 사용자는 docker-compose 등에서 단일 Postgres/Redis 컨테이너를 띄우고 `pm ports shared` 로 포트만 등록하는 흐름으로 전환.
- **문서**: `README.md`, `README-ko.md` 의 Local Port Management 섹션 갱신 (CLAUDE.md 룰).

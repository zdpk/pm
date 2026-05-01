## 1. Models & Schema

- [x] 1.1 `src/models.rs`: `SharedInfra { postgres_port: u16, redis_port: u16 }` 구조체 추가 (Serialize/Deserialize)
- [x] 1.2 `PortsData` 에 `shared: SharedInfra` 필드 추가, `Default` 구현이 `postgres_port=5432, redis_port=6379` 반환
- [x] 1.3 `PortKind::is_shared(self) -> bool` 메서드 추가 (Database/Redis 만 true)
- [x] 1.4 `PortsData::default()` 의 `version` 을 `2` 로 변경, `ranges` 에서 Database/Redis 항목 제거 (per-project 할당 대상 아님)

## 2. Migration

- [x] 2.1 `src/config.rs`: `load_ports()` 가 raw `serde_json::Value` 로 먼저 읽어 `version` 검사하도록 리팩터
- [x] 2.2 v1 또는 version 필드 부재 시: 원본을 `~/.config/pm/ports.json.bak.v1` 로 백업 (이미 존재하면 덮어쓰지 않음)
- [x] 2.3 마이그레이션 로직: 모든 `projects.<key>.services` 에서 `kind == "database" || kind == "redis"` 항목 제거
- [x] 2.4 마이그레이션 시 기본 `shared` 주입 후 `version = 2` 로 저장
- [x] 2.5 마이그레이션 실행 시 stderr 로 1회성 안내 메시지 출력 (`migrated ports.json v1 → v2; backup saved`)
- [x] 2.6 단위 테스트: v1 픽스처 로드 → v2 변환·백업 생성 검증, v2 픽스처는 변경 없음 검증

## 3. CLI: `pm ports shared`

- [x] 3.1 `src/cli.rs`: `PortsCommand::Shared { postgres: Option<u16>, redis: Option<u16> }` 변형 추가
- [x] 3.2 `src/commands/ports.rs`: `shared()` 함수 — 인자 없으면 현재 값 출력, 있으면 해당 필드만 갱신 후 저장
- [x] 3.3 갱신 시 `is_port_available(port)` 호출해 가용성 검증, 사용 중이면 경고 (Open Question 1: 검증 수행)
- [x] 3.4 `--postgres 0` 등 invalid port 거부
- [x] 3.5 `run()` 디스패치에 `Shared` 매칭 추가

## 4. CLI: `pm ports assign` 동작 변경

- [x] 4.1 `normalize_kinds()` 의 기본값을 `vec![PortKind::Backend]` 로 변경
- [x] 4.2 `assign()` 진입부에서 `kind.is_shared()` 인 항목이 있으면 에러 + `pm ports shared` 안내 후 종료
- [x] 4.3 `repair()` 도 shared kind 가 남아 있으면 무시(이미 마이그레이션됐어야 함)

## 5. CLI: `pm ports list/check` 출력

- [x] 5.1 `print_rows` 호출 전 SHARED 섹션을 별도 헤더로 출력 (`postgres`, `redis` 행)
- [x] 5.2 SHARED 행의 status 도 `is_port_available` 로 계산 (`free` / `bound`)
- [x] 5.3 `check()` 의 종합 판정에 SHARED `bound` 도 포함

## 6. `pm run` 환경 변수

- [x] 6.1 `src/commands/run.rs::build_port_env`: `local_database_name(workspace, project)` 새 시그니처로 변경 (workspace prefix 포함, 정규화)
- [x] 6.2 항상 `LOCAL_POSTGRES_PORT`, `DATABASE_URL` (shared 포트 + 정규화 db name) 주입
- [x] 6.3 항상 `LOCAL_REDIS_PORT`, `REDIS_URL`, `REDIS_KEY_PREFIX={workspace}:{project}` 주입
- [x] 6.4 기존 `PortKind::Database/Redis` 매칭 분기 제거 (shared 로 일원화)
- [x] 6.5 단위 테스트: 정규화 함수(소문자화, 비안전문자 치환, workspace 분리), env map 빌드 검증

## 7. 통합 검증

- [x] 7.1 `cargo build`, `cargo clippy --all-targets -- -D warnings` 통과
- [x] 7.2 `cargo test` 통과
- [x] 7.3 임시 v1 `ports.json` 픽스처로 마이그레이션 수동 검증 (백업 파일 존재 확인)
- [x] 7.4 `pm ports shared --postgres 5433` → `pm run <project> -- env | grep DATABASE_URL` 출력 확인
- [x] 7.5 `pm ports assign <project> --kind database` 가 에러로 종료하는지 확인

## 8. 문서화

- [x] 8.1 `README.md` Local Port Management 섹션 갱신: shared infra 모델, `pm ports shared`, `REDIS_KEY_PREFIX` 컨벤션, db name 규칙
- [x] 8.2 `README-ko.md` 동일 섹션 갱신 (N/A: 파일이 현재 저장소에 존재하지 않음 — 과거 커밋에서 제거됨)
- [x] 8.3 docker-compose 예시(단일 Postgres + Redis) 추가
- [x] 8.4 마이그레이션 안내 문구 추가 (v1 사용자 대상)

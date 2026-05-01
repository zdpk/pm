## 1. 의존성 및 Config 스키마

- [x] 1.1 `Cargo.toml` 에 sync `postgres` 추가 (Stage 1 자동 DB 생성용). `hyper`/`hyper-util`/`tokio-postgres`/`tokio` 는 Stage 2 데몬 작업 시 추가
- [x] 1.2 `src/models.rs` 에 `DevConfig { auto_start_docker, proxy_port, control_port, postgres_image, redis_image }` 추가, `Config` 에 `#[serde(default)] pub dev: DevConfig`
- [x] 1.3 `Default for DevConfig` 구현 (auto_start_docker=true, proxy_port=7100, control_port=7101, images: "postgres:16", "redis:7")
- [x] 1.4 단위 테스트: 기존 config.json 이 dev 섹션 없이 로드되어도 default 적용 (config_loads_with_default_dev_section_when_missing) + override 동작 (config_dev_section_overrides_apply)

## 2. `.proj.yaml` services 스키마

- [x] 2.1 `src/project.rs` 의 `ProjConfig` 에 `services: HashMap<String, ServiceDef>` 필드 추가
- [x] 2.2 `ServiceDef { dir, dev_cmd, port_kind, framework, path }` 모두 Optional 로 정의
- [x] 2.3 framework→defaults 추론 함수 `resolve_service_defaults(&ServiceDef, project_framework: Option<&str>) -> ResolvedService` 구현 (D9 의 표 그대로)
- [x] 2.4 nextjs 의 dev_cmd 기본값을 `"pnpm next dev --turbopack"` 으로 고정
- [x] 2.5 `.proj.yaml` parsing: services 섹션 부재 시 빈 HashMap, 호환성 보장
- [x] 2.6 단위 테스트 9개: framework 별 기본값, 사용자 override 우선, service-level framework 우선, dir 기본값/오버라이드, services 부재 시 v0.3.0 호환, path 필드 파싱 (Phase 2 reserved)

## 3. Docker 컨테이너 lifecycle

- [x] 3.1 신규 `src/commands/db.rs` 모듈
- [x] 3.2 `docker_available() -> bool` (외부 `docker` CLI 호출 가능 여부 체크)
- [x] 3.3 `ensure_postgres(image: &str)` — 컨테이너 존재 확인 → 없으면 `docker run -d --name pm-local-db -p 5432:5432 ...`, 있고 stopped 면 `docker start`
- [x] 3.4 `ensure_redis(image: &str)` — 동일 패턴, `pm-local-redis` + `pm-local-redis-volume`
- [x] 3.5 `is_port_available(5432)` 가 false 면 외부 인스턴스 가정, 컨테이너 skip + 로그 출력
- [x] 3.6 `pm db status/start/stop` CLI 분기 (`Commands::Db(DbCommand)`)
- [x] 3.7 Docker 미설치 / 미실행 시 친절한 에러: "Docker is required for `pm run`. Install Docker or set `dev.auto_start_docker: false`"
- [x] 3.8 단위 테스트 2개: ContainerState enum 동등성, error message 형식 (mock 추상화는 Stage 2 의 통합 테스트로 이연)

## 4. 자동 DB 생성

- [x] 4.1 sync `postgres` 클라이언트로 `127.0.0.1:5432` 연결 헬퍼 (`admin_connection_string`). tokio-postgres 는 Stage 2 에서 데몬용으로 추가
- [x] 4.2 `database_exists(client, name) -> bool` (`SELECT 1 FROM pg_database WHERE datname = $1`)
- [x] 4.3 `ensure_database_on_loopback(client, host, name)` — 미존재 시 `CREATE DATABASE "{name}"` (이중 큰따옴표 escape 가드)
- [x] 4.4 connection target 가드: `is_loopback_host(host)` 가 false 면 ensure_database skip + stderr 안내
- [x] 4.5 `local_database_name(workspace, project)` 의 `_local` 서픽스 제거 (BREAKING)
- [x] 4.6 `legacy_v03_databases()` + `emit_v03_migration_notice()` 헬퍼 추가 (Stage 2 에서 orchestrator 가 호출)
- [x] 4.7 기존 `commands/run.rs` 단위 테스트 갱신 (`work_my_app` 등 `_local` 없이 기대) + no-suffix 검증 테스트 추가

## 5. 데몬 본체 (`pm __daemon`)

- [x] 5.1 `src/cli.rs` 에 숨김 서브커맨드 `Commands::Daemon` (`#[command(name = "__daemon", hide = true)]`) + `ProxyCommand` 추가
- [x] 5.2 신규 `src/commands/proxy/{mod,daemon,reverse,control}.rs` — 데몬 모듈 분리
- [x] 5.3 `tokio::runtime::Builder` 멀티스레드 런타임, `daemon.pid` 파일 쓰기, 종료 시 정리
- [x] 5.4 `hyper` http1 으로 `127.0.0.1:{proxy_port}` listener, `serve_connection.with_upgrades()` 로 WebSocket 호환
- [x] 5.5 `Host` 헤더 → `RoutesCache` lookup, 없으면 404
- [x] 5.6 `routes.json` mtime 캐싱 (`refresh_if_changed` 매 요청 시 `metadata.modified()` 비교)
- [x] 5.7 graceful shutdown: `tokio::signal` 으로 SIGTERM/SIGINT 처리, `Notify` 로 양 listener 동시 종료
- [x] 5.8 데몬 stdout/stderr 를 `~/.config/pm/logs/daemon.log` 로 redirect (`spawn_detached` 의 `Stdio::from`)
- [x] 5.9 `#[cfg(unix)]` 로 게이트, non-Unix 빌드에서는 `pm proxy`/`pm __daemon` 모두 안내 에러

## 6. Control plane

- [x] 6.1 `127.0.0.1:{control_port}` 에 별도 hyper http1 listener
- [x] 6.2 `GET /health` → `200 OK { pid }`
- [x] 6.3 `GET /status` → `StatusBody { pid, uptime_sec, proxy_port, control_port, routes_count }`
- [x] 6.4 `POST /reload` → routes.json 은 매 요청마다 mtime 체크라 사실상 no-op, 호환성 위해 200 응답
- [x] 6.5 `POST /stop` → `Notify::notify_waiters()` 로 graceful shutdown trigger
- [x] 6.6 `/spawn` 은 Phase 2 (Stage 3) — 현재는 CLI 가 직접 서비스 spawn 후 routes.json 등록

## 7. CLI 측 데몬 ensure 로직

- [x] 7.1 `daemon::check_alive() -> Result<Option<u32>>` — daemon.pid + `nix::sys::signal::kill(pid, None)` + control plane `/health` 200 모두 통과 시 Some(pid)
- [x] 7.2 `daemon::spawn_detached()` — `current_exe()` + `__daemon` 인자, Unix `pre_exec` 로 `nix::unistd::setsid()`, stdio null/log
- [x] 7.3 `wait_until_ready` — exponential backoff (25→250ms), 최대 5초
- [x] 7.4 stale daemon.pid 처리: PID 가 죽었거나 control plane 무응답이면 파일 삭제 + None 반환

## 8. routes.json 관리

- [x] 8.1 `RouteEntry { hostname, upstream_port, project_key, service_key }` + `RoutesData { version, entries }` 모델 (`src/routes.rs`)
- [x] 8.2 `load_routes()` / `save_routes()` — tmp 파일 + atomic `fs::rename`
- [x] 8.3 atomic rename 으로 torn write 방지 (advisory `flock` 은 차후 — atomic rename 만으로 충분히 안전)
- [x] 8.4 `register_service(workspace, project, service, port)` — canonical hostname + (workspace == default 시) 단축 alias 동시 등록, 기존 `(project_key, service_key)` 항목 idempotent 교체
- [x] 8.5 `unregister_project()` / `unregister_service()` — Stage 3 의 `pm stop` 에서 호출 예정

## 9. Service 기동 (`pm run` orchestrator 모드)

- [ ] 9.1 `commands/run.rs` 의 grammar disambiguation: `--` 있으면 v0.3.0 모드, 없고 services 정의 있으면 orchestrator 모드
- [ ] 9.2 첫 인자가 `.proj.yaml` services 키면 service identifier, 아니면 project name 으로 해석 (services 우선)
- [ ] 9.3 orchestrator 모드 진입 시: `ensure_postgres` → `ensure_redis` → `ensure_database` → `ensure_daemon` → `register_routes` → spawn dev_cmd
- [ ] 9.4 dev_cmd spawn: `current_dir = service.dir`, env vars (LOCAL_POSTGRES_PORT 등) 주입, stdout/stderr 를 service log file 로 redirect
- [ ] 9.5 spawned service 의 PID 를 `~/.config/pm/state/services.json` 에 저장 (`{project_key, service: {pid, started_at, port}}`)
- [ ] 9.6 SIGINT 처리: foreground 실행 시 Ctrl+C 로 spawned service 종료 + routes.json 에서 제거
- [ ] 9.7 `pm run` 인자 0 → 모든 service, `pm run <svc>` → 한 service 만
- [ ] 9.8 `pm run <svc> <project>` → 다른 project 의 service 도 가능 (workspace 정규화)

## 10. `pm logs` / `pm stop`

- [ ] 10.1 `Commands::Logs { service, project }` CLI 추가
- [ ] 10.2 `pm logs <service>` — `tail -f ~/.config/pm/logs/<workspace>_<project>_<service>.log` 동등 구현 (notify crate 또는 polling)
- [ ] 10.3 `Commands::Stop { service, project }` CLI 추가
- [ ] 10.4 `pm stop` (인자 0) → 현재 project 의 모든 service SIGTERM
- [ ] 10.5 `pm stop <svc>` → 해당 service 만
- [ ] 10.6 종료 후 services.json 갱신 + routes.json 항목 제거

## 11. 로그 파일 rotation

- [ ] 11.1 `LogWriter` 구조체 — append + size check
- [ ] 11.2 10MiB 초과 시 `.log → .log.1`, `.log.1 → .log.2` ... `.log.3` 까지, 그 위는 삭제
- [ ] 11.3 단위 테스트: rotation 트리거 + 오래된 파일 삭제 검증

## 12. `pm proxy` 명령

- [ ] 12.1 `Commands::Proxy(ProxyCommand)` — Status / Stop / Start
- [ ] 12.2 `pm proxy status` → control plane `/status` 호출, 결과 출력
- [ ] 12.3 `pm proxy stop` → `/stop` 호출, daemon.pid 삭제 대기
- [ ] 12.4 `pm proxy start --foreground` → 데몬을 foreground 로 실행 (디버깅용)

## 13. Next.js 컨벤션 enforcement

- [ ] 13.1 `configs/typescript/nextjs/.npmrc` 신규 추가 (`engine-strict=true`, `auto-install-peers=true`, `strict-peer-dependencies=false`)
- [ ] 13.2 `configs/typescript/nextjs/manifest.yaml` 에 `.npmrc` 항목 (strategy: managed) 추가
- [ ] 13.3 `pm proj init -f nextjs` 가 lockfile 검사: `package-lock.json` 또는 `yarn.lock` 존재 시 stderr 경고
- [ ] 13.4 `pm proj init` 시 `--no-services` 플래그가 없으면 `.proj.yaml` 에 `services: { front: { framework: <framework> } }` 자동 추가
- [ ] 13.5 단위 테스트: `--no-services` 동작 확인, lockfile 경고 출력 확인

## 14. Windows / 비-Unix 가드

- [ ] 14.1 `pm run` orchestrator 모드 진입 시 `#[cfg(not(unix))]` 빌드에서는 명확한 에러 ("Orchestrator mode requires Unix; use `pm run -- <cmd>` for stateless mode")
- [ ] 14.2 daemon 진입점도 동일 가드
- [ ] 14.3 `pm run -- <cmd>` 등 stateless 명령은 Windows 포함 모든 타겟에서 동작 (CI release.yml 의 Windows 타겟은 별도 추가 검토)

## 15. 통합 검증

- [ ] 15.1 `cargo build --target {linux,macos}` 통과
- [ ] 15.2 `cargo clippy --all-targets` — 신규 warning 0 (기존 base 와 비교)
- [ ] 15.3 `cargo test` — 신규·기존 테스트 모두 pass
- [ ] 15.4 수동 E2E: `.proj.yaml` 에 services 정의 → `pm run` → `curl http://front.api.work.localhost:7100/` 로 dev 서버 응답 확인
- [ ] 15.5 수동 E2E: `pm run back` 단독 기동, `pm logs back`, `pm stop`, `pm proxy stop`
- [ ] 15.6 수동 E2E: Docker 없는 환경에서 친절한 에러 출력 확인
- [ ] 15.7 수동 E2E: 외부 Postgres 5432 점유 시 컨테이너 skip 동작 확인
- [ ] 15.8 v0.3.0 호환: `.proj.yaml` 없는 프로젝트에서 `pm run -- echo hi` 가 v0.3.0 동일 동작
- [ ] 15.9 BREAKING: `work_api_local` 존재하는 환경에서 `pm run` 시 마이그레이션 안내 출력 확인

## 16. 문서화

- [ ] 16.1 `README.md` Local Port Management 섹션 갱신: orchestrator 모드, hostname 라우팅, services 스키마
- [ ] 16.2 신규 섹션 "Local Dev Orchestrator" 작성: `pm run`/`pm logs`/`pm stop`/`pm db`/`pm proxy` 사용법
- [ ] 16.3 Next.js 컨벤션 (pnpm + Turbopack + .npmrc) 명시
- [ ] 16.4 v0.3.0 → v0.4.0 마이그레이션 가이드 (DB 이름 변경, `pg_dump`/`psql` 명령 예시)
- [ ] 16.5 비-Unix 사용자용 안내 (Windows = Phase 2)
- [ ] 16.6 path-based routing 은 Phase 2 라고 명시 (사용자가 `path:` 필드 시도해도 무시됨)

## 17. 버전 bump 및 릴리스

- [ ] 17.1 Cargo.toml version 0.3.0 → 0.4.0
- [ ] 17.2 git commit + tag v0.4.0
- [ ] 17.3 GitHub Actions release workflow 정상 동작 확인 (3 타겟 빌드)
- [ ] 17.4 GitHub Release notes 에 BREAKING 명시

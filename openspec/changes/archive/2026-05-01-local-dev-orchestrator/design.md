## Context

`pm` 은 v0.3.0 까지 **stateless CLI** 로 동작했다. 모든 명령은 짧게 실행되고 종료되며, 영구 상태는 `~/.config/pm/*.json` 파일에 있고, `pm run` 은 자식 프로세스를 spawn 후 자식의 종료까지만 살아있다. 이 모델은 단순하고 portable 하지만 다음 한계가 있다.

1. **사용자가 항상 `pm run` 래퍼를 거쳐야 환경 변수가 보임** — IDE, dotenv 라이브러리, docker-compose, 셸 직접 실행 등 정적 도구는 `pm` 을 모름
2. **monolith 워크플로 수동 작업** — front + back 을 다른 터미널에서 따로 띄우고 포트 충돌·환경 변수 일치를 사용자가 관리
3. **로컬 인프라 (Postgres/Redis) 매 프로젝트 수동 세팅** — docker-compose.yml 작성·기동·DB 생성을 사용자가 반복

vercel-labs/portless 가 검증한 "데몬 + hostname routing + 자동 spawn" 패턴은 이 셋을 동시에 해결한다. 본 change 는 portless 의 핵심 통찰을 Rust 단일 바이너리에 맞게 재구현하면서, **stateless 명령들 (`pm ports`, `pm ws`, `pm sync` 등) 은 그대로 유지**하고 새 stateful 모드를 옵트인으로 도입한다.

제약 / 환경:
- 단일 정적 Rust 바이너리 (Node 등 외부 런타임 0)
- macOS / Linux / Windows 3타겟 모두 지원
- Docker 는 옵션 (있으면 자동 활용, 없으면 graceful)
- `*.localhost` 자동 해석은 RFC 6761 + 주요 OS 기본값 의존

## Goals / Non-Goals

**Goals:**
- `pm run` 한 번으로 dev 환경 전체 (Postgres, Redis, front, back, hostname routing) 가 일관되게 기동
- monolith 사용자가 `pm run front` / `pm run back` 으로 service 단위 제어
- 데몬은 첫 호출 시 자동 기동되며 별도 systemd/launchd 등록 불필요
- v0.3.0 사용자가 이 기능을 옵트인으로 채택할 수 있고, `.proj.yaml` 없으면 v0.3.0 동작 그대로

**Non-Goals:**
- production 배포 자동화 (k8s, fly.io, etc.) — 로컬 dev 한정
- HTTPS / TLS 자동 설정 — 차기 change 에서 다룸 (portless 의 `trust` 명령에 해당)
- 운영 DB 와 동기화·마이그레이션 (Diesel, sqlx-migrate 등 기존 도구에 위임)
- 분산/원격 데몬 (Docker Desktop, k3d 등) — 로컬 단일 호스트만
- Windows 의 `:80`/`:443` 바인딩 — 비특권 포트만 지원 (Linux 도 동일하게 비특권 기본)
- 데몬의 hot reload 와 file watching — 사용자의 dev cmd (`next dev`, `cargo watch`) 가 처리

## Decisions

### D1. 같은 바이너리, 두 모드 (gpg-agent 패턴)
`pm` 바이너리에 숨김 서브커맨드 `pm __daemon` 추가. CLI 모드(`pm run` 등) 가 데몬 부재를 감지하면 `Command::new(current_exe()).arg("__daemon")` 으로 detached spawn (Unix: `pre_exec` 로 `setsid()`, Windows: `CREATE_NEW_PROCESS_GROUP`).

**Why**: 별도 데몬 바이너리 분리 시 배포 복잡도 ↑. 같은 바이너리의 두 모드는 portless·tmux·docker-compose 가 모두 채택한 검증된 패턴.
**Alternative**: systemd user unit 자동 등록 — 거부 (Linux 만 작동, macOS/Windows 별도 처리 필요, 사용자에게 systemctl 학습 강요).

### D2. Reverse proxy: hyper + tower
데몬은 `tokio` + `hyper` 의 `hyper::server::conn` 으로 직접 HTTP 1.1 처리. WebSocket / HTTP/2 도 지원. 라우팅은 매 요청 시 `routes.json` 을 읽어 (간단·캐싱 불필요·rare update) `Host` 헤더로 매칭.

**Why**: nginx/caddy 외부 의존을 피하고 Rust 생태계 표준 (hyper) 사용. tokio-tungstenite 추가 시 WebSocket 자연스럽게 지원.
**Alternative**: `pingora` (Cloudflare) — 거부 (overkill, 의존성 무거움). caddy embed — 거부 (Go 런타임 필요).

### D3. Hostname 형식: `<service>.<project>.<workspace>.localhost`
DNS 관습에 따라 specific → general (예: `front.api.work.localhost`). `<workspace>` 가 `default` 인 경우는 생략 가능 (`front.api.localhost`) — 사용자 friction ↓.

**Why**: `*.localhost` 는 OS 자동 해석. workspace 차원이 들어가야 글로벌 충돌 0. 라벨 분리(`.`) 로 정렬·필터링 용이.
**Alternative**: `<workspace>-<project>-<service>.localhost` (단일 라벨) — 거부 (가독성 ↓, Host 헤더 매칭 시 패턴 분기 어려움).

### D4. 라우팅 모델: 기본 subdomain, opt-in path
기본은 service 별 subdomain (`front.api.work.localhost`, `back.api.work.localhost`). `.proj.yaml` 의 service 가 `path: /api` 같은 옵션 명시 시 모델 B (path-based) 로 전환 — 같은 hostname 안에서 path 분기.

**Why**: 단순 subdomain 이 routes.json 을 평면적으로 유지. path 모델은 prod parity 가 필요한 사용자만 활성화.
**Trade-off**: 모델 A 에서는 CORS / cookie 도메인을 다르게 다뤄야 함. 사용자에게 명확히 안내 필요.

### D5. 데몬 ↔ CLI 통신: 파일 + control plane
- **routes.json**: CLI 가 쓰고 데몬이 매 요청마다 stat → mtime 변경 시 reload. 빈번하지 않은 갱신이라 inotify 불필요.
- **daemon.pid**: PID + 데몬이 listen 중인 control plane endpoint. CLI 가 데몬 alive check 시 `kill -0 <pid>` + `127.0.0.1:<control_port>/health` 두 단계 검증.
- **control plane**: `127.0.0.1:7100` 의 작은 HTTP — `/health`, `/stop`, `/reload`, `/spawn`, `/logs`. `pm proxy stop` 등 명시적 제어용.

**Why**: 파일은 단순·atomic 하지만 즉시 반영 안 됨. control plane 은 명시적 호출에 필요. 두 채널 분리가 깔끔.
**Alternative**: Unix socket 만 사용 — 거부 (Windows 호환성, 디버깅 시 `curl` 으로 확인 어려움).

### D6. 데몬이 listen 할 포트
**기본 `127.0.0.1:7100`** (비특권 포트). `pm config dev.proxy_port` 로 변경 가능. `:80`/`:443` 은 사용자가 명시적으로 권한 부여 시만.

**Why**: macOS / Linux 의 비루트 사용자는 `:80` 바인딩 불가. 첫 사용자 경험을 sudo 없이 만드는 게 우선.
**Trade-off**: hostname 접근 시 `http://front.api.work.localhost:7100` 로 포트 명시 필요 → 약간의 마찰. portless 의 `:443` 자동 사용 경험과 비교해 열등. 차후 setcap/setuid helper 고려.

### D7. Docker 컨테이너 lifecycle
- `pm run` 진입 → `is_port_available(5432)` 검사
- 외부 Postgres 발견 (포트 점유) → 컨테이너 skip, 외부 사용
- 미발견 → `docker run -d --name pm-local-db -p 5432:5432 -e POSTGRES_PASSWORD=postgres -v pm-local-volume:/var/lib/postgresql/data postgres:16`
- 컨테이너 stop 은 명시적 (`pm db stop`) 만. `pm run` 종료 시 컨테이너 보존.
- `docker` CLI invoke 방식 (외부 프로세스) — bollard 라이브러리 의존 회피, Docker 미설치 시 panic 없음.

**Why**: 사용자가 자기 Postgres 를 이미 띄워둔 케이스를 우선 존중. CLI invoke 가 라이브러리보다 가벼움 (Docker socket 경로 자동 처리, 인증 자동).
**Alternative**: bollard — 거부 (의존성 + Docker socket 경로·환경 변수 처리 직접). podman 별도 분기 — Phase 2.

### D8. 자동 DB 생성
`pm run` 이 Postgres 가 ready (헬스체크 5초 대기) 한 후 `tokio-postgres` 로 `SELECT 1 FROM pg_database WHERE datname = $1` → 없으면 `CREATE DATABASE "<workspace>_<project>"`.

**Why**: 사용자 첫 실행 시 마찰 0. 멱등성: 매 `pm run` 마다 검사하지만 존재 시 no-op.
**Risk**: 사용자가 DB 이름을 dev 와 prod 에서 같게 쓰고 실수로 prod 호스트에 연결 시 prod DB 까지 만들 위험. 가드: connection target 이 `127.0.0.1` 이거나 `localhost` 일 때만 자동 CREATE.

### D9. `.proj.yaml` 의 services 스키마
```yaml
language: ts
framework: nextjs

services:
  front:
    dir: frontend         # 기본값: .
    dev_cmd: "pnpm dev"   # 생략 시 framework 기반 추론
    port_kind: frontend   # 생략 시 framework 기반 추론
    path: /               # 생략 시 모델 A (subdomain)
  back:
    dir: backend
    framework: axum       # service 단위 framework override
    dev_cmd: "cargo run"
```

framework → defaults:
| framework | dev_cmd 기본 | port_kind |
|---|---|---|
| nextjs | `pnpm next dev --turbopack` | frontend |
| vite | `pnpm dev` | frontend |
| nestjs | `pnpm start:dev` | backend |
| axum | `cargo run` | backend |
| fastapi | `uvicorn main:app --reload` | backend |
| flutter | `flutter run` | frontend |

**Next.js 컨벤션 (pnpm + Turbopack)**: pm 은 Next.js 프로젝트의 기본 도구로 **pnpm + Turbopack** 을 강제한다.
- **pnpm**: npm/yarn 대비 디스크 사용량 ↓ (CAS 기반 hard-link), monorepo workspace 지원, 일관된 lockfile 결정성. pm 의 multi-project 워크플로와 정합성 ↑.
- **Turbopack**: webpack 대비 dev 빌드 속도 압도적, Next.js 14+ 에서 안정 단계 진입 (Next.js 16 에서 default). 명시적 `--turbopack` 플래그로 버전 무관하게 동일 동작 보장.
- **dev_cmd 형태**: `pnpm dev` 가 아닌 `pnpm next dev --turbopack` — 사용자의 `package.json` `scripts.dev` 정의에 의존하지 않고 pm 단독으로 동작 보장. 사용자가 다른 동작을 원하면 `.proj.yaml` 의 `dev_cmd` 로 명시 override.

추가 enforcement 메커니즘 (Phase 2 task):
- `configs/typescript/nextjs/.npmrc` 에 `engine-strict=true`, `auto-install-peers=true` 등 pnpm 최적화 설정 추가
- (옵션) `package.json` 의 `packageManager: "pnpm@9.x"` 설정 안내 — `pm proj init` 시 사용자가 이미 npm 으로 init 했다면 경고
- (옵션) `"preinstall": "npx only-allow pnpm"` 권장 — 팀원이 npm install 시도 시 즉시 거부

**Why**: 사용자 90% 케이스는 `services: { front: {}, back: {} }` 처럼 키만 선언, 나머지는 추론. `.proj.yaml` 가 없으면 service 0 → `pm run` 의 새 동작 비활성, 기존 grammar 작동.

### D10. `pm run` grammar 디스앰비규에이션
```
1. `pm run` (인자 0)         → .proj.yaml 의 모든 service 기동
2. `pm run <token>`           → token 이 .proj.yaml 의 service 키면 service 기동, 아니면 project 로 해석 후 모든 service 기동
3. `pm run <s> <p>`           → service s, project p
4. `pm run <p> -- <cmd...>`   → 기존 mode (legacy 임의 cmd 실행) — `--` 가 결정타
5. `pm run -- <cmd...>`       → 기존 mode, 현재 project
```

`--` 의 유무가 mode 분기. 첫 인자가 .proj.yaml service 키와 충돌하는 project 이름이면 `pm run @<workspace>/<project>` 의 `@` prefix 로 강제 가능.

**Why**: 후방호환 유지가 최우선. v0.3.0 사용자가 `pm run myproj -- cargo run` 을 그대로 쓸 수 있어야 함.

### D11. 로그 저장
service spawn 시 stdout/stderr 를 `~/.config/pm/logs/<workspace>_<project>_<service>.log` 에 append + rotation (10MB × 3개). `pm logs <service>` 는 `tail -f` 로 동작.

**Why**: 사용자가 다른 터미널·세션에서도 로그 확인 가능. file rotation 으로 디스크 폭주 방지.

### D12. 의존성 추가 최소화
- `hyper` + `hyper-util` (이미 `tokio` transitive 가능성)
- `tokio-postgres` (DB 자동 생성용)
- 외부 `docker` CLI invoke (`std::process::Command`) — bollard 거부
- `tokio` 런타임 (이미 있음 가능성, 재확인)

**Why**: 새 crate 2개로 한정. Cargo.toml 비대화 방지.

## Risks / Trade-offs

- **Risk**: 비특권 포트 7100 사용 시 hostname URL 에 `:7100` 명시 필요 → portless (443) 보다 열악한 UX.
  → **Mitigation**: README 에 명확히 표기. 차후 change 에서 setcap/launchd 옵션 도입.
- **Risk**: `*.localhost` 자동 해석이 일부 Linux 배포(특히 nss-myhostname 미설치) 에서 작동 안 할 수 있음.
  → **Mitigation**: 데몬 시작 시 `getaddrinfo("test.localhost")` 검증 → 실패 시 `/etc/hosts` 추가 가이드 출력.
- **Risk**: Docker auto-start 가 사용자 의도와 다르게 5432 점유 (예: 사용자가 곧 자기 컨테이너 띄울 예정).
  → **Mitigation**: `config.json` 의 `dev.auto_start_docker: false` 옵션 + 첫 실행 시 동의 prompt.
- **Risk**: 데몬 crash 시 모든 routes 사라짐, dev 끊김.
  → **Mitigation**: 데몬 stdout 도 로그로 보존. CLI 가 health check 실패 시 자동 재기동.
- **Risk**: routes.json 동시 쓰기 race (두 `pm run` 동시 실행).
  → **Mitigation**: 파일 lock (advisory `fcntl`) 또는 atomic rename (`tmp` → `rename`).
- **Risk**: BREAKING DB name 변경으로 v0.3.0 → v0.4.0 사용자가 데이터 분실 체감.
  → **Mitigation**: 첫 `pm run` 시 v0.3.0 형식 DB 발견하면 stderr 로 안내 (`work_api_local 이 발견됨; 새 이름은 work_api 입니다. pg_dump | pg_restore 로 이전하세요`).
- **Trade-off**: 데몬·Docker·routes.json 도입은 pm 의 stateless 정체성 약화.
  → **Mitigation**: 새 기능은 `.proj.yaml` 의 services 가 있을 때만 활성, 없으면 v0.3.0 동작 그대로 → 옵트인 보장.

## Migration Plan

1. **v0.4.0 릴리스 노트**에 BREAKING 두 가지 명시:
   - DB 이름 `_local` 제거 → 기존 사용자는 `pg_dump work_api_local | psql work_api` 로 이전
   - `pm run <token>` 의 token 우선순위 변경 (services 키 우선)
2. **Rollback 경로**: v0.4.0 사용자가 v0.3.0 으로 돌아가도 ports.json v2 는 호환. routes.json/daemon.pid 은 v0.3.0 가 무시.
3. **첫 실행 마이그레이션 안내**: `.proj.yaml` 에 services 가 있고 `<workspace>_<project>_local` 이 존재하면 stderr 1회 안내.

## Resolved Decisions (v0.4.0 scope)

1. **데몬 종료**: 사용자 명시 `pm proxy stop` 까지 살아있음. service 가 0이 되어도 자동 종료 안 함. idle 데몬 자원 부담은 작고, 다음 `pm run` 시 warm start 이점.
2. **OS 지원 범위**: **Unix only** (macOS / Linux). Windows 에서 orchestrator-mode (`.proj.yaml` 의 services 가 있는 상태에서 `pm run`) 호출 시 친절한 에러로 안내. stateless 명령 (`pm run -- cmd`, `pm ports`, `pm ws` 등) 은 Windows 포함 v0.3.0 호환 유지. Unix-specific 코드는 `#[cfg(unix)]` 로 게이트.
3. **`config.json` 의 신규 섹션**: `dev:` 신설. 필드:
   - `auto_start_docker: bool` (default `true`)
   - `proxy_port: u16` (default `7100`)
   - `control_port: u16` (default `7101`)
   - `postgres_image: String` (default `"postgres:16"`)
   - `redis_image: String` (default `"redis:7"`)
4. **`pm proj init` 의 services 자동 생성**: 기본 활성. `--language ts --framework nextjs` 만 주면 `.proj.yaml` 에 `services: { front: { framework: nextjs } }` 자동 추가. 끄려면 `--no-services` 플래그.
5. **path-based routing (모델 B)**: Phase 2. v0.4.0 에서는 subdomain (모델 A) 만 구현. `.proj.yaml` 의 `path:` 필드는 파서가 받아들이되 무시 + TODO 코멘트 + 사용자에게 "Phase 2" 안내.

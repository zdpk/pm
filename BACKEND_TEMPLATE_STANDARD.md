# Rust Axum SQLx Backend Template Standard

이 문서는 Rust + Axum + SQLx + PostgreSQL 백엔드 템플릿의 표준을 정리한다.

목표는 새 웹서비스를 만들 때 아키텍처, 환경 분리, 로컬 인프라, Docker, 테스트 구조를 일관되게 시작하는 것이다.

# Stack

기본 스택:

- Rust
- Axum
- Tokio
- SQLx
- PostgreSQL
- Redis 또는 Redis-compatible cache, 필요 시
- Testcontainers
- Docker image for shared environments

로컬 개발에서는 API를 직접 실행한다.

```sh
cargo run
```

로컬 backing service만 Docker Compose로 실행한다.

```sh
docker compose up -d db redis
```

# Architecture

기본 구조는 실용적인 3-tier architecture다.

- `handler`: HTTP boundary
- `service`: application / business logic
- `repository`: persistence boundary

기본 dependency direction:

```text
handler -> service -> repository -> database
```

규칙:

- handler는 얇게 유지한다.
- service가 비즈니스 결정을 가진다.
- repository가 SQL, DB mapping, transaction을 가진다.
- domain logic은 Axum, SQLx, HTTP type에 직접 의존하지 않는다.

# Repository Pattern

Production에서는 concrete SQLx repository를 사용한다.

```rust
pub struct PgUserRepository {
    pool: PgPool,
}
```

repository trait은 기본값이 아니다.

service unit test에서 dependency substitution이 필요할 때만 도입한다.

# Test Doubles

Rust 백엔드에서는 전면 mocking보다 다음 조합이 자연스럽다.

```text
real database in integration tests
small fakes in unit tests
mock framework only at external boundaries
```

unit test:

- service / domain 중심
- hand-written fake 또는 stub 선호

integration test:

- testcontainers 기반 real PostgreSQL 사용
- migration 적용
- SQLx mapping, constraint, transaction, HTTP response mapping 검증

mock framework가 적합한 대상:

- 외부 API client
- payment gateway
- email sender
- object storage
- queue publisher
- clock, random, ID generator

mock을 피할 대상:

- pure domain logic
- internal service by default
- SQL behavior
- owned data structures

# Runtime Environments

환경 이름은 다음으로 통일한다.

```text
local
dev
stg
prod
```

`APP_ENV` 값도 이 네 가지만 허용한다.

```text
APP_ENV=local|dev|stg|prod
```

사용하지 않는 이름:

- `stag`
- `staging`
- `prd`

`stg`를 config, code, deployment name, resource suffix, dashboard, CI/CD environment name에 일관되게 쓴다.

# Environment Roles

## local

개발자 머신이다.

목적:

- 빠른 inner-loop 개발
- 로컬 디버깅
- `cargo run`으로 API 실행
- Docker Compose로 PostgreSQL / Redis 실행

규칙:

- API는 host에서 직접 실행한다.
- PostgreSQL은 Docker Compose로 실행한다.
- Redis는 필요할 때 Docker Compose로 실행한다.
- `.env` 사용 가능.
- secret은 fake, sandbox, developer-specific 값만 사용한다.
- production data를 직접 사용하지 않는다.

## dev

공유 integration 환경이다.

목적:

- 한 개발자 머신 밖에서 merged code 검증
- container startup, runtime config, migration, external sandbox service 검증
- 내부 사용자와 개발자가 함께 보는 test target 제공

규칙:

- API는 Docker image로 실행한다.
- PostgreSQL은 managed cloud DB 또는 cost-reduced managed equivalent를 사용한다.
- Redis/cache가 app behavior에 중요하면 managed service를 사용한다.
- main integration branch에서 자동 배포 가능.
- data는 disposable이어도 된다.
- external service는 sandbox credential을 사용한다.

## stg

production rehearsal 환경이다.

목적:

- release candidate 검증
- migration rehearsal
- production-like config, network, TLS, auth, observability 검증
- 배포 및 data-shape 문제를 prod 전에 잡기

규칙:

- prod에 배포할 같은 Docker image를 사용한다.
- PostgreSQL은 managed cloud DB를 사용한다.
- Redis/cache도 managed service를 사용한다.
- scale은 작아도 되지만 production shape와 같아야 한다.
- migration은 prod 전에 여기서 검증한다.
- test data는 realistic 또는 anonymized여야 한다.

## prod

실사용자와 실제 데이터를 처리하는 환경이다.

규칙:

- promoted Docker image를 실행한다.
- PostgreSQL은 managed production DB를 사용한다.
- Redis/cache는 managed production cache를 사용한다.
- config와 secret은 runtime platform 또는 secret manager에서 주입한다.
- `.env` 파일은 image나 server에 복사하지 않는다.
- deploy는 CI/CD를 통해 진행한다.
- logs는 structured + redacted.
- migration은 명시적인 release step으로 실행한다.
- DB 직접 수동 변경은 피한다.

# Configuration

Twelve-factor configuration을 따른다.

configuration은 environment variable에서 온다.

commit하는 파일:

- `.env.example`

local-only 파일:

- `.env`

ignore할 파일:

- `.env`
- `.env.*`

commit하지 않는 파일:

- `.env.dev`
- `.env.stg`
- `.env.prod`
- real secrets

`.env.dev`, `.env.stg`, `.env.prod`는 표준 configuration mechanism으로 쓰지 않는다.

이런 파일은 공유 환경 secret을 개발자 머신으로 퍼뜨리고, prod 값을 local에서 실수로 쓰게 만들 수 있다.

# Config Injection

`local`만 checked-out `.env`에 의존한다.

`dev`, `stg`, `prod`는 runtime platform에서 환경 변수를 주입한다.

예시:

- GCP Cloud Run env vars + Secret Manager references
- AWS ECS task env vars + Secrets Manager 또는 SSM Parameter Store
- Kubernetes ConfigMap + Secret
- Fly.io / Render / Railway platform config
- CI/CD deployment variables
- Vault / Doppler / central secret manager

secret은 runtime에 주입한다.

Docker image에 bake하지 않는다.

production-like debugging이 필요하면 `.env.prod` 파일보다 secret-manager CLI나 temporary shell environment를 쓴다.

# Typed Config

환경 변수는 app startup에서 한 번 typed config로 로드한다.

required value가 없으면 server 시작 전에 fail fast한다.

secret value는 log에 남기지 않는다.

공통 변수:

```text
APP_ENV=local|dev|stg|prod
APP_HOST=127.0.0.1
APP_PORT=...
POSTGRES_DB=...
DATABASE_URL=postgres://...
REDIS_URL=redis://...
RUST_LOG=info
```

local port 변수:

```text
LOCAL_POSTGRES_PORT=...
LOCAL_REDIS_PORT=...
```

app-specific secret:

```text
JWT_SECRET=...
COOKIE_KEY=...
OAUTH_CLIENT_ID=...
OAUTH_CLIENT_SECRET=...
```

Rust loading rule:

```rust
pub fn load_config() -> Result<Config, ConfigError> {
    let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "local".to_string());

    if app_env == "local" {
        dotenvy::dotenv().ok();
    }

    Config::from_env()
}
```

`dotenvy`는 local convenience다.

`.env` 파일이 없어도 application은 동작해야 한다.

# Local Backing Services

`local`에서는 Docker Compose로 stateful dependency를 실행한다.

기본 local services:

- PostgreSQL
- Redis 또는 Redis-compatible cache, 필요 시

API는 보통 직접 실행한다.

```sh
cargo run
```

이 방식은 rebuild와 debugger 사용이 빠르면서도 DB/cache는 실제 network service처럼 다룰 수 있다.

Docker Compose는 이 표준에서 local-only다.

`dev`, `stg`, `prod`의 표준 배포 방식으로 사용하지 않는다.

# Managed Backing Services

`dev`부터는 managed cloud resource를 기본으로 한다.

AWS 예시:

- RDS PostgreSQL
- Aurora PostgreSQL
- ElastiCache

GCP 예시:

- Cloud SQL for PostgreSQL
- AlloyDB
- Memorystore

`dev`는 비용 때문에 작은 managed instance나 disposable managed instance를 쓸 수 있다.

초기 prototype에서는 containerized DB를 dev에서 잠깐 쓸 수 있다.

하지만 이건 cost-saving exception으로 문서화해야 한다.

`stg`부터는 prod와 같은 shape의 managed resource를 사용한다.

`prod`에서 self-managed DB/cache를 쓰려면 별도 decision record가 필요하다.

# Local Port Problem

여러 service가 같은 template으로 만들어지면 local port 충돌이 난다.

예:

```text
service-a APP_PORT=4000
service-b APP_PORT=4000
```

또는:

```text
service-a LOCAL_POSTGRES_PORT=15432
service-b LOCAL_POSTGRES_PORT=15432
```

해결 원칙:

- host port를 template에 hardcode하지 않는다.
- `.env`에 app별 port를 둔다.
- port 배정은 script가 수행한다.
- script는 충돌을 검사한다.
- 충돌이 있으면 `--repair`로 `.env`를 수정한다.

# Local Port Ranges

표준 port range:

```text
3000-3999    frontend dev servers
4000-4999    backend APIs
15000-15999  PostgreSQL
16000-16999  Redis or Redis-compatible cache
17000-17999  object storage, search, queues, mail tools, other local infra
```

# Local Port Registry

중앙 registry 파일을 둔다.

```text
standards/local-port-registry.md
```

역할:

- local port assignment의 human-readable inventory
- retired app port 추적
- 충돌 방지 규칙 문서화

하지만 실제 충돌 방지는 script가 수행한다.

script는 다음을 검사한다.

- `~/workspace/apps/*/.env`에 이미 배정된 포트
- `127.0.0.1`에 이미 bind된 포트
- 현재 `.env` 내부 중복

# Local Env Initializer

새 앱에는 다음 script를 둔다.

```text
scripts/init-local-env.py
```

새 `.env` 생성:

```sh
python scripts/init-local-env.py
```

기존 `.env` 충돌 검사:

```sh
python scripts/init-local-env.py --check
```

기존 `.env` 충돌 복구:

```sh
python scripts/init-local-env.py --repair
```

script가 관리하는 값:

```text
APP_ENV
APP_HOST
APP_PORT
POSTGRES_DB
LOCAL_POSTGRES_PORT
LOCAL_REDIS_PORT
DATABASE_URL
REDIS_URL
RUST_LOG
```

충돌이 있으면 기본 실행은 실패한다.

`--repair`를 쓰면 새 port를 배정하고 URL도 같이 갱신한다.

# Local .env Example

```dotenv
APP_ENV=local
APP_HOST=127.0.0.1
APP_PORT=4274
POSTGRES_DB=my_app_local
LOCAL_POSTGRES_PORT=15546
LOCAL_REDIS_PORT=16060
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:15546/my_app_local
REDIS_URL=redis://127.0.0.1:16060
RUST_LOG=debug,sqlx=warn,tower_http=debug
```

# Docker Compose Template

template 파일명:

```text
docker-compose.template.yml
```

실제 앱에 적용할 때는 복사해서:

```text
docker-compose.yml
```

compose는 local-only다.

host port는 `.env`에서 읽는다.

```yaml
services:
  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: ${POSTGRES_DB:-app_local}
    ports:
      - "127.0.0.1:${LOCAL_POSTGRES_PORT:?set LOCAL_POSTGRES_PORT in .env}:5432"
    volumes:
      - postgres-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d $${POSTGRES_DB:-app_local}"]
      interval: 5s
      timeout: 5s
      retries: 10

  redis:
    image: redis:7-alpine
    ports:
      - "127.0.0.1:${LOCAL_REDIS_PORT:?set LOCAL_REDIS_PORT in .env}:6379"
    volumes:
      - redis-data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 5s
      retries: 10

volumes:
  postgres-data:
  redis-data:
```

포트 변수가 없으면 compose가 실패한다.

# Docker Image Policy

`dev`, `stg`, `prod`는 Docker image로 실행한다.

shared environment flow:

```sh
docker build -t app-api:<git-sha> --build-arg APP_BIN=app-api .
docker push <registry>/app-api:<git-sha>
deploy <registry>/app-api:<git-sha>
```

runtime platform이 env vars와 secrets를 주입한다.

`.env` 파일을 `dev`, `stg`, `prod`로 복사하지 않는다.

# Image Promotion

commit마다 image를 한 번 build한다.

같은 image를 순차 promote한다.

```text
dev -> stg -> prod
```

환경별로 image를 다시 build하지 않는다.

환경 차이는 runtime config에 둔다.

# SQLx Build Rule

SQLx compile-time checked macro를 쓴다면 Docker build에서는 offline mode를 선호한다.

기본 규칙:

```sh
cargo sqlx prepare --workspace
```

`.sqlx/` metadata를 commit한다.

Docker build에서는:

```text
SQLX_OFFLINE=true
```

Docker image build 중 live database가 필요하면 안 된다.

# Migration Rule

production migration을 Docker entrypoint 안에 숨기지 않는다.

명시적인 release step으로 실행한다.

권장 흐름:

```text
1. Build image
2. Deploy to dev
3. Run migrations in dev
4. Promote image to stg
5. Run migrations in stg
6. Promote image to prod
7. Run migrations in prod as controlled release step
```

작은 internal tool에서는 startup migration도 가능하다.

다만 의도적으로 선택하고 문서화해야 한다.

# Dockerfile Standard

하나의 multi-stage Dockerfile을 표준으로 쓴다.

전제:

- binary name은 `APP_BIN` build arg로 주입
- `cargo build --release --locked`
- SQLx offline metadata 사용
- runtime image에는 source code, Cargo cache, `.env` 없음
- non-root user로 실행
- log는 stdout/stderr

template:

```dockerfile
ARG RUST_VERSION=1

FROM rust:${RUST_VERSION}-bookworm AS builder

ARG APP_BIN

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release --locked --bin ${APP_BIN}

FROM debian:bookworm-slim AS runtime

ARG APP_BIN

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system app \
    && useradd --system --gid app --home /app app

WORKDIR /app

COPY --from=builder /app/target/release/${APP_BIN} /usr/local/bin/app

USER app

ENV APP_ENV=prod
ENV APP_HOST=0.0.0.0
ENV APP_PORT=8080
ENV RUST_LOG=info

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/app"]
```

OpenSSL runtime package는 dependency가 native TLS를 요구할 때만 추가한다.

가능하면 Rustls-compatible dependency를 선호한다.

# .dockerignore Template

```dockerignore
target/
.git/
.github/
.idea/
.vscode/
.DS_Store

.env
.env.*
!.env.example

node_modules/
.next/
dist/
coverage/

*.log
tmp/
temp/
```

# Project Shape

기본 module shape:

```text
src/
  main.rs
  lib.rs
  app.rs
  config.rs
  error.rs
  state.rs
  modules/
    users/
      mod.rs
      handler.rs
      service.rs
      repository.rs
      model.rs
      dto.rs
tests/
  common/
    mod.rs
  users_api.rs
  users_repository.rs
migrations/
.env.example
Dockerfile
docker-compose.yml
scripts/
  init-local-env.py
```

`main.rs`는 작게 유지한다.

router construction은 `lib.rs` 또는 `app.rs`에서 노출한다.

integration test가 network server를 띄우지 않고 app을 build할 수 있어야 한다.

# Template Application Flow

새 앱을 만들 때:

```text
1. Copy Dockerfile template to Dockerfile
2. Copy dockerignore template to .dockerignore
3. Copy docker-compose.template.yml to docker-compose.yml
4. Copy init-local-env.py to scripts/init-local-env.py
5. Run python scripts/init-local-env.py
6. Review generated .env
7. Run docker compose up -d db redis
8. Run sqlx migrate run
9. Run cargo run
```

충돌이 나면:

```sh
python scripts/init-local-env.py --repair
```

# Final Defaults

핵심 default:

```text
local uses .env
dev/stg/prod use runtime config and secret manager
local uses Docker Compose for backing services only
dev/stg/prod use managed backing services
dev/stg/prod run Docker images
one image is promoted through dev -> stg -> prod
ports are assigned by script, not manually guessed
```

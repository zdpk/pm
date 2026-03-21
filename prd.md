# proj — PRD (Product Requirements Document)

## 개요

`proj`는 여러 프로젝트 레포의 설정 파일(린터, 포매터, Dockerfile, CI/CD 등)을 중앙에서 관리하고 동기화하는 CLI 도구.

레포가 늘어날수록 설정 파일 관리가 반복 작업이 되는 문제를 해결한다.

## 구조

```
proj-config/          # config repo — 설정 파일 원본 (single source of truth)
├── rust/
│   ├── axum/
│   ├── clap/
│   └── common/       # clippy.toml, rustfmt.toml, .editorconfig 등
├── typescript/
│   ├── nextjs/
│   ├── nestjs/
│   └── common/       # eslint.config.js, tsconfig.json, prettier 등
├── python/
│   ├── fastapi/
│   └── common/       # pyproject.toml, ruff.toml 등
├── dart/
│   ├── flutter/
│   └── common/       # analysis_options.yaml 등
├── c/
│   └── common/       # .clang-format, .clang-tidy 등
└── shared/           # 언어 무관 공통 파일
    ├── Dockerfile
    ├── .github/
    │   └── workflows/
    ├── .editorconfig
    ├── .gitignore
    └── ...

proj/                 # CLI repo — Rust로 작성
└── src/
```

## 지원 언어 및 프레임워크

| 언어 | 프레임워크 | 주요 설정 파일 |
|------|-----------|---------------|
| Rust | Axum | clippy.toml, rustfmt.toml, Cargo.toml 템플릿, Dockerfile |
| Rust | Clap (CLI) | clippy.toml, rustfmt.toml, Cargo.toml 템플릿, cargo-dist 설정 |
| TypeScript | Next.js | eslint.config.js, tsconfig.json, prettier, next.config.js, Dockerfile |
| TypeScript | NestJS | eslint.config.js, tsconfig.json, prettier, nest-cli.json, Dockerfile |
| Python | FastAPI | pyproject.toml, ruff.toml, Dockerfile |
| Dart | Flutter | analysis_options.yaml, pubspec.yaml 템플릿 |
| C | (일반) | .clang-format, .clang-tidy, Makefile/CMakeLists.txt 템플릿 |

## Interaction Modes

All CLI output and prompts are in **English**.

### Interactive Mode (default)

`proj init` without arguments launches interactive mode:

```
$ proj init

? Setup mode: (Use arrow keys)
> Auto-detect
  Manual

# ── Auto-detect ──
# Scans current directory for language markers (Cargo.toml, package.json, etc.)

Detected: TypeScript (package.json found)
? Framework: (Use arrow keys)
> Next.js (next.config detected)
  NestJS
  None

? Include CI/CD? (Y/n)
? Include Dockerfile? (Y/n)
? Include pre-commit hooks? (Y/n)

✓ Project initialized (ts/nextjs)

# ── Manual ──
? Language: (Use arrow keys)
> Rust
  TypeScript
  Python
  Dart
  C

? Framework: (Use arrow keys)
> Axum
  None

? Include CI/CD? (Y/n)
? Include Dockerfile? (Y/n)
? Include pre-commit hooks? (Y/n)

✓ Project initialized (rust/axum)
```

Auto-detect markers:
| Marker | Detected as |
|--------|------------|
| `Cargo.toml` | Rust |
| `package.json` | TypeScript |
| `pyproject.toml` / `requirements.txt` | Python |
| `pubspec.yaml` | Dart |
| `Makefile` / `CMakeLists.txt` / `*.c` | C |

Framework detection (within language):
| Marker | Detected as |
|--------|------------|
| `next.config.*` | Next.js |
| `nest-cli.json` | NestJS |
| `actix` / `axum` in Cargo.toml deps | Axum |
| `fastapi` in pyproject.toml deps | FastAPI |
| `flutter` in pubspec.yaml | Flutter |

### Non-interactive Mode

For CI/CD, scripts, or when you know exactly what you want:

```bash
proj init --language rust --framework axum --ci --docker --no-interactive
proj init -l ts -f nextjs --all --no-interactive
```

All interactive prompts have corresponding CLI flags:
| Flag | Short | Description |
|------|-------|-------------|
| `--language` | `-l` | Language |
| `--framework` | `-f` | Framework |
| `--ci` | | Include CI/CD workflows |
| `--docker` | | Include Dockerfile |
| `--hooks` | | Include pre-commit hooks |
| `--all` | | Include everything (ci + docker + hooks) |
| `--no-interactive` | `-y` | Skip all prompts |

## CLI Commands

### `proj init [language] [framework]`

Initialize project config files.

```bash
proj init                  # Interactive mode
proj init rust axum        # Rust + Axum backend
proj init rust clap        # Rust + Clap CLI app
proj init -l ts -f nextjs --all -y  # Full non-interactive with flags
```

**Behavior:**
1. Determine language/framework (interactive or from args)
2. Fetch files from config repo: `common/` + `framework/` + selected `shared/` files
3. Copy to current directory
4. Generate `.proj.yaml`

```yaml
# .proj.yaml (auto-generated)
language: rust
framework: axum
config_version: "a3f2b1c"   # config repo commit hash
```

### `proj sync`

현재 프로젝트의 설정 파일을 config repo 최신 버전에 맞춰 동기화.

```bash
proj sync              # 현재 디렉토리의 프로젝트 동기화
proj sync --all        # 등록된 모든 프로젝트 동기화
proj sync --dry-run    # 변경될 파일만 미리 보기
```

**동작:**
1. `.proj.yaml` 읽어서 언어/프레임워크 파악
2. config repo 최신 버전과 비교
3. 변경된 파일 업데이트
4. `.proj.yaml`의 `config_version` 갱신

### `proj check`

설정이 outdated인지 확인.

```bash
proj check             # 현재 프로젝트 체크
proj check --all       # 모든 등록 프로젝트 체크
```

**출력 예시:**
```
✗ my-api (rust/axum) — outdated (3 files changed)
  - clippy.toml
  - Dockerfile
  - .github/workflows/ci.yml
✓ my-frontend (ts/nextjs) — up to date
```

### `proj list`

등록된 프로젝트 목록 표시.

```bash
proj list
```

**출력 예시:**
```
my-api         rust/axum       /home/s/projects/my-api         v2026-03-21
my-frontend    ts/nextjs       /home/s/projects/my-frontend    v2026-03-15
my-app         dart/flutter    /home/s/projects/my-app         v2026-03-10
```

### `proj add`

기존 프로젝트를 proj 관리 대상에 등록 (init 없이).

```bash
proj add --language rust --framework axum
```

### `proj diff`

현재 프로젝트의 설정 파일과 config repo 최신 버전의 차이를 표시.

```bash
proj diff
```

## 설정 파일 관리 정책

### 파일 분류

1. **Managed (완전 관리)** — config repo에서 100% 덮어씀. 로컬 수정 불가.
   - 예: .editorconfig, .clang-format, rustfmt.toml

2. **Merged (병합 관리)** — config repo 기본값 + 프로젝트별 커스텀 병합.
   - 예: .gitignore (공통 + 프로젝트 고유 패턴), Dockerfile

3. **Template (초기만)** — init 시에만 생성, 이후 sync 대상 아님.
   - 예: Cargo.toml, package.json (프로젝트마다 다르니까)

```toml
# proj-config/rust/common/manifest.toml
[[files]]
path = "rustfmt.toml"
strategy = "managed"

[[files]]
path = ".gitignore"
strategy = "merged"

[[files]]
path = "Cargo.toml"
strategy = "template"
```

## config repo 배포 방식

- config repo는 Git으로 관리
- CLI가 config repo를 로컬에 clone/pull해서 사용
- `~/.proj/config/` 에 캐시
- `proj update` 명령으로 config repo 최신화

```bash
proj update            # config repo를 최신으로 pull
```

## 글로벌 설정

```toml
# ~/.proj/config.toml
[config]
repo = "https://github.com/s/proj-config.git"
cache_dir = "~/.proj/config"

[projects]
# proj add 시 자동 등록
"/home/s/projects/my-api" = { language = "rust", framework = "axum" }
"/home/s/projects/my-frontend" = { language = "ts", framework = "nextjs" }
```

## 기술 스택

- **CLI 언어:** Rust
- **주요 크레이트:**
  - `clap` — CLI 인자 파싱
  - `serde_yaml` — .proj.yaml 파싱
  - `toml` — config repo manifest 파싱
  - `git2` — config repo Git 연동
  - `dialoguer` — 인터랙티브 프롬프트
  - `colored` — 터미널 출력 꾸미기
  - `walkdir` — 파일 탐색
  - `similar` — diff 비교

## Release & Distribution

### Pipeline

```
PR merge → release-please (auto version bump + CHANGELOG + tag)
        → git tag push
        → cargo-dist (cross-compile + GitHub Release + install script)
```

### cargo-dist

Rust CLI 빌드 및 GitHub Release 자동화.

**Setup:**
```bash
cargo install cargo-dist
cargo dist init
# → Cargo.toml에 [workspace.metadata.dist] 추가
# → .github/workflows/release.yml 자동 생성
```

**Cargo.toml config:**
```toml
[workspace.metadata.dist]
cargo-dist-version = "0.x.x"
ci = "github"
installers = ["shell", "powershell", "homebrew"]
targets = [
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
]
```

**Supported targets:**
| Platform | Architecture | Target |
|----------|-------------|--------|
| Linux | x86_64 | `x86_64-unknown-linux-gnu` |
| Linux | ARM64 | `aarch64-unknown-linux-gnu` |
| macOS | Intel | `x86_64-apple-darwin` |
| macOS | Apple Silicon | `aarch64-apple-darwin` |

**Install methods generated:**
- `curl -sSf https://...install.sh | sh` (Linux/macOS)
- `irm https://...install.ps1 | iex` (Windows, optional)
- Homebrew tap (optional)

### release-please

Automated version bump + CHANGELOG generation.

**Flow:**
1. PR merge to main
2. release-please analyzes commits (Conventional Commits)
3. Creates/updates a "Release PR" with version bump + CHANGELOG
4. Merge Release PR → tag created → cargo-dist triggers

**Conventional Commits required:**
```
feat: add proj sync --all        → minor bump (0.1.0 → 0.2.0)
fix: correct auto-detect for TS  → patch bump (0.1.0 → 0.1.1)
feat!: redesign init flow        → major bump (0.1.0 → 1.0.0)
```

### Distribution by Repo Visibility

**Public repo:**
- cargo-dist가 install.sh 자동 생성
- `curl -sSf https://...install.sh | sh` 로 설치
- Homebrew tap, crates.io publish 가능

**Private repo:**
- install.sh 별도 제작 안 함
- `gh release download`로 설치 (gh 로그인 상태면 인증 자동 처리)
```bash
gh release download latest -R owner/proj -p "proj-*-$(uname -m)*" -D /tmp
tar -xzf /tmp/proj-*.tar.gz -C /tmp
mv /tmp/proj ~/.local/bin/proj
```

## 개발 로드맵

### Phase 1 — MVP
- [ ] `proj init` — interactive + non-interactive (Rust/Axum만 우선)
- [ ] Auto-detect (언어/프레임워크 마커 감지)
- [ ] `.proj.yaml` 생성
- [ ] config repo clone/캐시
- [ ] `proj sync` (단일 프로젝트)
- [ ] `proj check` (단일 프로젝트)
- [ ] cargo-dist 설정 (GitHub Release 자동화)
- [ ] release-please 설정 (자동 버전 범프 + CHANGELOG)

### Phase 2 — 확장
- [ ] 나머지 언어/프레임워크 추가 (TS, Python, Dart, C)
- [ ] `proj sync --all`, `proj check --all`
- [ ] `proj list`, `proj add`
- [ ] managed/merged/template 파일 전략

### Phase 3 — 자동화 연동
- [ ] `proj diff` 명령
- [ ] GitHub Actions 연동 (config 변경 시 각 레포에 자동 PR)
- [ ] Claude Code skill 래퍼 (`/proj-init`, `/proj-sync`)

## 미결정 사항

- C 프레임워크/빌드 시스템 (Makefile? CMake?)
- config repo를 public으로 할지 private으로 할지
- 프로젝트별 override 허용 범위

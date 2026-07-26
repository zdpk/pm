# PM (Project Manager) CLI Specification

> Note: the current implementation now uses `config.json` + `manifest.json`, workspace roots, `pm sync`, and lazy restore on `pm sw`. Treat `README.md` as the up-to-date user-facing reference until this spec is fully refreshed.

Git 프로젝트 디렉토리들을 등록하고 관리하는 Rust CLI 도구

## 설계 철학

PM은 **git 프로젝트와 디렉토리 관리**에만 집중하는 최소한의 도구입니다.

### 핵심 기능 (Core)
- 프로젝트 등록/삭제/목록
- Workspace 관리
- 프로젝트 간 이동
- 설정 동기화 (load/save)

### 확장 기능 (Plugin/Hook)
다음은 모두 플러그인이나 훅으로 분리:
- Git hooks (pre-commit, commit-msg 등)
- 프로젝트 템플릿 (언어별 설정 파일)
- 공통 파일 (.editorconfig, CLAUDE.md 등)

### 통합하지 않는 것
- 터미널 에뮬레이터 (Ghostty, iTerm2 등)
- 터미널 멀티플렉서 (tmux, zellij 등)
- IDE/에디터 통합

이러한 도구들은 사용자 선택에 맡기며, PM은 프로젝트 관리 본연의 기능에 충실합니다.

### 선택적 로컬 개발 오케스트레이션

`.project.yaml`에 `services:`가 선언된 프로젝트의 `pm run`은 선택적으로
로컬 앱 실행과 hostname 라우팅까지 관리합니다. 이는 Vercel Portless를
호출하거나 앱마다 프록시를 띄우는 구조가 아니라, Portless와 같은 원리를
PM의 공용 daemon에 직접 적용한 기능입니다.

#### hostname이 포트를 대신하는 원리

PM은 실행된 앱이 선택한 포트를 나중에 탐지하지 않습니다. 먼저 사용 가능한
upstream 포트를 할당하고, service의 `port_kind`에 대응하는 환경변수
(`FRONTEND_PORT`, `APP_PORT` 등)로 앱에 주입한 다음 해당 포트를 hostname
route에 등록합니다. 앱 또는 프로젝트 설정은 주입된 변수를 실제 listen
port로 사용해야 합니다. PM은 `dev_cmd`를 shell interpolation 없이 직접
실행하므로, 해당 변수를 기본 지원하지 않는 framework는 프로젝트 쪽
설정에서 변수를 읽도록 구성해야 합니다.

```text
pm run
  1. 서비스용 빈 포트 선택 (예: 26918)
  2. 앱에 포트를 주입하고 127.0.0.1:26918에서 실행
  3. services.json에 PM이 관리하는 PID와 포트 기록
  4. routes.json에 back.api.work.localhost -> 127.0.0.1:26918 기록
  5. 사용자별 공용 reverse proxy가 없으면 자동 실행

browser
  http://back.api.work.localhost:7100
      -> DNS가 *.localhost를 loopback IP로 해석
      -> 공용 proxy가 HTTP Host header로 route 조회
      -> 127.0.0.1:26918로 요청 전달
```

DNS는 hostname을 loopback IP로 해석할 뿐 포트를 선택하지 않습니다.
실제 hostname-to-port 선택은 `dev.proxy_port`에서 실행되는 공용 HTTP
reverse proxy가 담당합니다. 따라서 기본 포트가 7100이면 URL에도 `:7100`이
필요합니다. 이후 proxy를 80/443에 설치하는 기능이 추가될 경우에만 주소에서
포트를 생략할 수 있습니다.

상태의 역할은 다음과 같이 구분합니다.

| 파일 | 역할 |
|------|------|
| `ports.json` | 서비스에 할당한 upstream 포트 |
| `services.json` | PM이 실행하고 관리하는 process registry |
| `foreground-processes.json` | `pm run --`으로 실행 중인 foreground process registry |
| `routes.json` | live service로부터 파생한 hostname URL projection |

`pm stop`은 service record와 그 서비스의 canonical/alias route를 모두
제거합니다. 동일 서비스를 다시 등록할 때도 기존 alias를 서비스 단위로
먼저 제거하여 이전 포트가 남지 않게 합니다. 이미 실행 중인 서비스를
`pm run`으로 다시 확인할 때도 route projection을 재등록해 유실된 route를
복구합니다. 대시보드 조회는 PID 생존 여부만 확인하며 상태 파일을 수정하지
않습니다.

이 라우팅은 HTTP `Host` header를 사용하는 application-layer reverse
proxy이며 OS NAT나 범용 TCP port mapping이 아닙니다. PostgreSQL, Redis와
같은 raw TCP 서비스는 hostname으로 분기하지 않고 공유 포트, database
이름, key prefix 및 주입된 connection URL로 분리합니다.

상세한 normative requirement와 scenario는
[`local-dev-orchestrator` spec](openspec/specs/local-dev-orchestrator/spec.md)을
따릅니다.

#### 실행 중인 앱 대시보드

공용 proxy는 다음 주소에 PM이 관리하는 모든 앱을 보여주는 read-only HTML
페이지를 제공합니다.

```text
http://pm.localhost:<proxy_port>/
http://localhost:<proxy_port>/       # 편의 alias
http://127.0.0.1:<proxy_port>/       # 편의 alias
```

페이지는 `services.json`의 detached service를 Background로,
`foreground-processes.json`의 실행 중인 `pm run --` child를 Foreground로
표시하고 `ports.json`의 port kind로 frontend와 나머지 process를 구분하며,
`routes.json`에서 로컬 URL을 결합합니다. frontend는 카드 전체가 링크이며
Tailnet URL이 있으면 이를 우선하고, 없으면 로컬 URL로 이동합니다. 알려진
frontend workspace/project 조합은 버전이 고정된 평면 리소그래프 스타일의 내장
일러스트와 한 줄 제품 설명을 종이·잉크 기반의 절제된 editorial layout으로 함께
표시하고, 별도 presentation이 없는 project는 중립적인 fallback visual을
사용합니다. backend, infra와 Foreground process는 하단의 이동 주소 없는 요약
목록으로 표시합니다. 모든 항목은 workspace/project/service, PID, upstream
port와 시작 시각을 표시하고 frontend에는 선호 URL도 표시합니다. canonical
hostname과 짧은 alias가 함께 있어도 서비스는 한 번만 표시합니다. route가
없는 live frontend service도 목록에서 숨기지 않고 URL을 unavailable로
표시합니다. 비정상 종료 뒤 registry에 남은 PID는 실행 중으로 계산하지 않고
stopped 상태로 표시합니다. 페이지는 idle 상태에서 5초마다 갱신하되
interactive control에 focus가 있으면 갱신을 미루며, 사용자가 자동 갱신을
pause/resume할 수 있어야 합니다.

Tailscale Serve가 설치되어 있으면 현재 loopback proxy mapping을 읽어
service port와 일치하는 Tailnet URL을 함께 표시하며 frontend 카드의 우선
이동 주소로 사용합니다. PM proxy는 계속
loopback에만 bind하며, `dev.dashboard_hosts`에 등록한 정확한 hostname만
Tailscale 같은 로컬 reverse proxy를 통한 dashboard Host로 허용합니다.

대시보드는 조회 전용입니다. 앱 시작·중지·재시작, route 변경, shell 실행을
제공하지 않으며 환경변수, credential, 로그 내용이나 foreground command
argument도 노출하지 않습니다.

## 파일 구조

```
~/.config/pm/
├── config.json      # 전역 설정
├── projects.json    # 프로젝트 데이터
└── workspaces.json  # 워크스페이스 데이터
```

## 환경변수

| 변수 | 설명 | 기본값 |
|------|------|--------|
| `PM_CONFIG_DIR` | 설정 디렉토리 경로 (수동 override) | 자동 감지 |

### 개발 환경 자동 분리

바이너리 이름에 따라 설정 디렉토리가 자동으로 분리됨:

| 바이너리 | 설정 디렉토리 | 용도 |
|----------|---------------|------|
| `pm` | `~/.config/pm/` | 상용 |
| `pmd` | `~/.config/pm-dev/` | 개발/테스트 |

### 설치

```bash
# 상용 버전
cargo install --path . --bin pm

# 개발 버전 (전역 설치)
cargo install --path . --bin pmd
```

이를 통해 `pmd ls` 실행 시 상용 설정과 충돌 없이 테스트 가능.

## 경로 규칙

- **저장**: 항상 `~/` 형식으로 저장
- **입력**: 절대 경로, 상대 경로, `~` 모두 허용
- **내부 처리**: 사용 시 절대 경로로 expand

```rust
// 저장 시: 절대 경로 → ~ 형식으로 변환
fn collapse_path(path: &Path) -> String;

// 사용 시: ~ → 절대 경로로 변환
fn expand_path(path: &str) -> PathBuf;
```

---

## 데이터 스키마

### `config.json`

```json
{
  "editor": "cursor",
  "git_host": "https://github.com",
  "display": {
    "show_full_path": false,
    "color": true
  },
  "git": {
    "auto_fetch": false,
    "fetch_interval": 3600
  }
}
```

- `git_host`: `pm load/save`와 `pm restore` 시 사용되는 기본 git 호스트. GitLab, Bitbucket 등으로 변경 가능.

### `projects.json`

```json
{
  "version": 1,
  "projects": [
    {
      "name": "my-app",
      "path": "~/github/my-app",
      "remote": "git@github.com:user/my-app.git",
      "tags": ["rust", "cli"],
      "note": "Main project",
      "added_at": "2025-01-08T10:00:00Z",
      "last_accessed": "2025-01-08T12:00:00Z"
    }
  ]
}
```

- `remote`: git origin URL (등록 시 자동 추출, 복구에 활용)

### `workspaces.json`

```json
{
  "version": 1,
  "current": "default",
  "workspaces": [
    {
      "name": "default",
      "projects": ["my-app"],
      "created_at": "2025-01-08T10:00:00Z"
    },
    {
      "name": "work",
      "projects": ["company-api"],
      "created_at": "2025-01-08T10:00:00Z",
      "git": {
        "user.name": "John Doe",
        "user.email": "john@company.com",
        "core.sshCommand": "ssh -i ~/.ssh/work_key"
      }
    },
    {
      "name": ".trash",
      "projects": [],
      "created_at": "2025-01-08T10:00:00Z"
    }
  ]
}
```

- `git`: Workspace별 git 설정 (선택). 프로젝트 등록 시 `.git/config`에 자동 적용

---

## Workspace 시스템

### 규칙

| 항목 | 규칙 |
|------|------|
| 초기 상태 | `default` workspace 자동 생성 |
| 시스템 예약 | `.`으로 시작 (예: `.trash`) |
| 사용자 생성 | 알파벳으로 시작 (`[a-zA-Z][a-zA-Z0-9_-]*`) |
| `.trash` | 숨김, 삭제된 workspace의 고아 프로젝트 보관 |

### 삭제 동작

```bash
pm ws rm work
# → 프로젝트들은 .trash로 이동
```

### Workspace 자동 감지 (Path 기반)

현재 디렉토리가 등록된 프로젝트 내부면 해당 workspace 자동 선택.

```bash
$ cd ~/work/company-api
$ pm ls
# company-api가 work workspace 소속 → work 자동 선택

$ cd ~/random/place
$ pm ls
# 매칭 없음 → JSON의 current 사용
```

**우선순위**:
1. 현재 경로가 프로젝트 내부 → 해당 프로젝트의 workspace
2. 매칭 없음 → `workspaces.json`의 `current`

**장점**:
- 터미널 충돌 없음 (경로 기반 판단)
- 명시적 `switch` 없이 자연스러운 전환
- 기존 `current` 방식과 호환

---

## 명령어

### 별칭

| 전체 | 단축 |
|------|------|
| `workspace` | `ws` |
| `remove` | `rm` |
| `list` | `ls` |
| `switch` | `sw` |

---

## `pm add`

Git URL 또는 로컬 경로를 현재 workspace의 관리 경로에 materialize한 뒤
프로젝트로 등록한다. Git URL은 clone하고, 존재하는 로컬 디렉토리는
이동하며, 존재하지 않는 로컬 경로는 관리 경로에 빈 프로젝트
디렉토리를 생성한다.

### 사용법

```bash
pm add [path-or-url] [options]
```

### 인자

| 인자 | 설명 | 기본값 |
|------|------|--------|
| `path-or-url` | 로컬 프로젝트 경로 또는 Git URL | `.` (현재 디렉토리) |

### 옵션

| 옵션 | 단축 | 설명 |
|------|------|------|
| `--name <name>` | `-n` | 프로젝트 이름 지정 (기본: 디렉토리명) |
| `--tags <tags>` | `-t` | 태그 추가 (쉼표 구분) |
| `--note <note>` | | 메모 추가 |
| `--force` | `-f` | 충돌하는 manifest 항목 교체 (기존 목적지는 덮어쓰지 않음) |

### 동작 흐름

1. 입력을 Git URL 또는 로컬 경로로 분류
2. 이름 결정 (`--name`, URL의 저장소명 또는 로컬 경로의 마지막 요소)
3. 이름과 manifest 충돌을 검증하고 목적지를
   `<base_root>/<current_workspace>/<name>`으로 결정
4. 입력을 목적지에 materialize
   - Git URL: 목적지로 직접 clone
   - 존재하는 로컬 디렉토리: 목적지로 이동
   - 목적지 자체를 가리키는 로컬 디렉토리: 제자리에서 등록
   - 존재하지 않는 로컬 경로: 원래 소스 경로는 만들지 않고 목적지에
     빈 디렉토리 생성
5. 로컬 Git 저장소이면 `origin`을 remote로 저장하고, Git 저장소가
   아니면 경고 후 계속 진행
6. materialize가 성공한 뒤 manifest에 등록하고 현재 프로젝트로 설정
7. 결과 출력

### 출력 예시

```bash
$ pm add ~/github/my-app
✓ Moved ~/github/my-app → ~/ws/default/my-app
✓ Added 'my-app' to workspace 'default'

$ pm add ~/github/new-app
⚠ Source directory does not exist; creating an empty project directory
✓ Created and registered ~/ws/default/new-app
✓ Added 'new-app' to workspace 'default'

$ pm add https://github.com/acme/company-api.git
✓ Cloned https://github.com/acme/company-api.git → ~/ws/default/company-api
✓ Added 'company-api' to workspace 'default'
```

### 검증 규칙

| 항목 | 규칙 | 실패 시 |
|------|------|---------|
| 로컬 소스 | 존재하면 디렉토리여야 함. 존재하지 않으면 빈 관리 디렉토리 생성 | 파일 등 디렉토리가 아닌 항목이면 에러 |
| Git 저장소 | 로컬 디렉토리에 `.git`이 없어도 등록 가능 | 경고 후 진행 |
| Git URL | 목적지로 clone할 수 있어야 함 | clone 실패 시 에러, manifest 변경 없음 |
| 목적지 | 제자리 등록을 제외하고 어떤 파일시스템 항목도 없어야 함 | 에러, `--force`로도 덮어쓰지 않음 |
| 이름 중복 | 기존 manifest 이름과 충돌하지 않아야 함 | 에러 (`--force`는 충돌 항목 교체) |
| 경로 중복 | 같은 workspace의 `dir`과 충돌하지 않아야 함 | 에러 (`--force`는 충돌 항목 교체) |
| 이름 형식 | 영문자로 시작하고 영문, 숫자, `-`, `_`만 사용 | 에러 |
| 이동/생성 | 목적지 materialize가 성공해야 함 | 에러, manifest 변경 없음 |

---

## `pm new`

새 디렉토리를 생성하고 프로젝트로 등록.

### 사용법

```bash
pm new <path> [options]
```

### 옵션

`pm add`와 동일 + 추가 옵션 (추후 확장):
- `--template <name>`: 템플릿 사용
- `--clone <url>`: git clone 후 등록

### 동작

1. 디렉토리 생성
2. `git init`
3. 프로젝트 등록 (`pm add`와 동일)

---

## `pm rm`

프로젝트 삭제.

### 사용법

```bash
pm rm <project>
pm rm -f <project>
pm rm -rf <project>
```

### 옵션

| 옵션 | 설명 | 파일 | 복구 |
|------|------|------|------|
| (없음) | 등록 해제 | 유지 | `pm add`로 재등록 |
| `-f` | trash로 이동 | 유지 | `pm trash restore` |
| `-rf` | 파일까지 삭제 | 삭제 | 불가 (remote에서 clone) |

### 출력 예시

```bash
$ pm rm my-app
✓ Unregistered 'my-app' (files kept at ~/github/my-app)

$ pm rm -f my-app
✓ Moved 'my-app' to trash

$ pm rm -rf my-app
⚠ This will permanently delete:
  ~/github/my-app (23 files, 1.2MB)

Type project name to confirm: my-app
✓ Deleted 'my-app' and its files
```

---

## `pm ls`

프로젝트 목록 표시.

### 사용법

```bash
pm list [options]
pm ls [options]
```

### 옵션

| 옵션 | 단축 | 설명 |
|------|------|------|
| `--all` | `-a` | 모든 workspace의 프로젝트 표시 |
| `--tags <tags>` | `-t` | 특정 태그로 필터링 |
| `--path` | `-p` | 전체 경로 표시 (`~` 대신 절대 경로) |
| `--no-status` | | git 상태 생략 (빠른 출력) |
| `--sort <field>` | `-s` | 정렬 기준 (기본: accessed) |
| `--reverse` | `-r` | 역순 정렬 |
| `--filter <type>` | `-f` | 필터링 (git, non-git, orphan) |

### 정렬 필드

| 필드 | 설명 |
|------|------|
| `accessed` | 최근 접근순 (기본값) |
| `name` | 이름순 |
| `path` | 경로순 |
| `added` | 등록일순 |
| `frequency` | 접근 빈도순 (자주 사용하는 것 먼저) |
| `status` | git 상태순 (변경 있는 것 먼저) |

### 필터

| 필터 | 설명 |
|------|------|
| `git` | git 저장소만 |
| `non-git` | git 아닌 것만 |
| `orphan` | 경로가 유효하지 않은 것 |

### 출력 형식

```bash
$ pm ls
Workspace: work

NAME           PATH                   BRANCH     STATUS      TAGS
company-api    ~/work/company-api     main       ✓ clean     rust, api
company-web    ~/work/company-web     develop    2↑ 1↓       typescript
internal-tool  ~/work/internal-tool   feature    3 changed   rust
```

### 상태 표시 규칙

| 상태 | 표시 | 의미 |
|------|------|------|
| clean | `✓ clean` | 변경 없음, 동기화됨 |
| ahead | `2↑` | 2 commits ahead |
| behind | `1↓` | 1 commit behind |
| diverged | `2↑ 1↓` | ahead + behind |
| changed | `3 changed` | uncommitted 변경 |
| conflict | `⚠ conflict` | merge conflict |
| error | `✗ error` | git 오류 또는 경로 없음 |

---

## `pm switch` / `pm sw`

프로젝트 디렉토리로 이동.

### 사용법

```bash
pm switch <project>
pm sw <project>              # 단축
pm sw @<workspace>/<project> # workspace 전환 + 프로젝트 이동
```

### 문법

| 형식 | 동작 |
|------|------|
| `pm sw my-app` | 현재 workspace에서 `my-app`으로 이동 |
| `pm sw @work/my-app` | `work` workspace로 전환 후 `my-app`으로 이동 |

### Shell Integration 필요

```bash
# .bashrc / .zshrc
pm() {
    if [[ "$1" == "sw" || "$1" == "switch" ]] && [[ -n "$2" ]]; then
        cd "$(command pm path "$2")"
    else
        command pm "$@"
    fi
}
```

### 보조 명령어

```bash
pm path <project>              # 경로만 출력 (shell integration용)
pm path @<workspace>/<project> # workspace 포함 경로 출력
```

### 자동완성

```bash
$ pm sw my<TAB>
my-app  my-lib  my-tool

$ pm sw @w<TAB>
@work/

$ pm sw @work/<TAB>
@work/company-api  @work/company-web  @work/internal-tool
```

---

## `pm ws` (Workspace)

### `pm ws list`

```bash
$ pm ws list
  NAME       PROJECTS
* default    1
  work       3
  personal   2
```

### `pm ws new <name>`

```bash
$ pm ws new work
✓ Created workspace 'work'
✓ Switched to 'work'
```

### `pm ws rm <name>`

```bash
pm ws rm <workspace>
pm ws rm -f <workspace>
pm ws rm -rf <workspace>
```

| 옵션 | 설명 | 프로젝트 | 파일 |
|------|------|----------|------|
| (없음) | workspace 삭제 | trash로 이동 | 유지 |
| `-f` | workspace 삭제 | 등록 해제 | 유지 |
| `-rf` | workspace 삭제 | 삭제 | 삭제 |

```bash
$ pm ws rm work
⚠ 3 projects will be moved to trash.
Continue? [y/N] y
✓ Removed workspace 'work'

$ pm ws rm -f work
⚠ 3 projects will be unregistered (files kept).
Continue? [y/N] y
✓ Removed workspace 'work'

$ pm ws rm -rf work
⚠ This will permanently delete workspace 'work' and all files:
  - company-api (~/work/company-api)
  - company-web (~/work/company-web)
  - internal-tool (~/work/internal-tool)

Type workspace name to confirm: work
✓ Deleted workspace 'work' and all project files
```

### `pm use <name>`

Workspace 전환 (top-level 명령어).

```bash
$ pm use work
✓ Switched to workspace 'work'
```

### `pm ws mv <project> <workspace>`

```bash
$ pm ws mv company-api personal
✓ Moved 'company-api' to 'personal'

$ pm ws mv company-api company-web personal
✓ Moved 2 projects to 'personal'
```

---

## `pm trash`

### `pm trash list`

```bash
$ pm trash list
NAME          PATH                    MOVED_AT
company-api   ~/work/company-api      2025-01-08
company-web   ~/work/company-web      2025-01-08
```

### `pm trash restore <project> <workspace>`

```bash
$ pm trash restore company-api work
✓ Restored 'company-api' to 'work'
```

### `pm trash clear`

```bash
$ pm trash clear
⚠ 2 projects will be permanently unregistered:
  - company-api
  - company-web
Continue? [y/N]
```

---

## `pm tag`

태그는 프로젝트에 직접 귀속 (전역, workspace 무관).

### `pm tag add <project> <tags...>`

```bash
$ pm tag add my-app rust cli api
✓ Added tags to 'my-app': rust, cli, api
```

### `pm tag rm <project> <tags...>`

```bash
$ pm tag rm my-app cli api
✓ Removed tags from 'my-app': cli, api
```

### `pm tag ls [project]`

```bash
# 특정 프로젝트
$ pm tag ls my-app
rust, api

# 전체 태그 목록
$ pm tag ls
TAG          COUNT
rust         3
typescript   2
api          1
```

---

## `pm completion`

Shell 자동 완성 스크립트 생성.

### 사용법

```bash
pm completion bash > ~/.bash_completion.d/pm
pm completion zsh > ~/.zfunc/_pm
pm completion fish > ~/.config/fish/completions/pm.fish
```

### 또는

```bash
# .bashrc / .zshrc
eval "$(pm completion bash)"
eval "$(pm completion zsh)"
```

### 자동 완성 대상

| 대상 | 예시 |
|------|------|
| 서브커맨드 | `pm w<tab>` → `pm ws` |
| 옵션 | `pm add -<tab>` → `--name`, `--tags` |
| 프로젝트 이름 | `pm rm my<tab>` → `pm rm my-app` |
| Workspace 이름 | `pm ws switch w<tab>` → `pm ws switch work` |

---

## `pm check`

모든 프로젝트 경로 유효성 검사.

### 사용법

```bash
pm check
```

### 출력 예시

```bash
$ pm check
✓ company-api    ~/work/company-api
✓ company-web    ~/work/company-web
✗ my-app         ~/github/my-app (not found)
✗ old-project    ~/old/project (not found)

2 projects have invalid paths.
```

---

## `pm update`

프로젝트 경로 업데이트.

### 사용법

```bash
pm update <project> <new-path>
```

### 출력 예시

```bash
$ pm update my-app ~/new-location/my-app
✓ Updated 'my-app' path: ~/github/my-app → ~/new-location/my-app
```

---

## `pm restore`

삭제된 프로젝트를 remote에서 복구 (저장된 origin URL 사용).

### 사용법

```bash
pm restore <project> [path]
```

### 동작

1. `projects.json`에서 저장된 `remote` URL 확인
2. 지정된 경로 (또는 원래 경로)에 `git clone`
3. 프로젝트 경로 업데이트

### 출력 예시

```bash
$ pm restore my-app
✓ Cloning from git@github.com:user/my-app.git
✓ Restored 'my-app' to ~/github/my-app

$ pm restore my-app ~/new-location/my-app
✓ Cloning from git@github.com:user/my-app.git
✓ Restored 'my-app' to ~/new-location/my-app
```

### 에러

```bash
$ pm restore my-app
✗ No remote URL saved for 'my-app'
```

---

## `pm ws config`

Workspace별 git 설정 관리.

### 사용법

```bash
pm ws config <workspace> <key> <value>   # 설정
pm ws config <workspace> --list          # 목록
pm ws config <workspace> --unset <key>   # 삭제
```

### 예시

```bash
$ pm ws config work git.user.email "john@company.com"
✓ Set git.user.email for workspace 'work'

$ pm ws config work git.user.name "John Doe"
✓ Set git.user.name for workspace 'work'

$ pm ws config work --list
git.user.email = john@company.com
git.user.name = John Doe

$ pm ws config work --unset git.user.name
✓ Unset git.user.name for workspace 'work'
```

---

## `pm ws apply-git`

Workspace의 git 설정을 모든 프로젝트에 적용.

### 사용법

```bash
pm ws apply-git <workspace>
```

### 출력 예시

```bash
$ pm ws apply-git work
✓ Applied git config to 'company-api'
✓ Applied git config to 'company-web'
✓ Applied git config to 'internal-tool'

Applied to 3 projects.
```

---

## `pm init`

PM 설정 초기화.

### 사용법

```bash
pm init [options]
```

### 옵션

| 옵션 | 단축 | 설명 |
|------|------|------|
| `--force` | `-f` | 기존 설정 덮어쓰기 |

### 동작

1. `~/.config/pm/` 디렉토리 생성
2. 기본 `config.json` 생성
3. 빈 `projects.json` 생성
4. `default` workspace로 `workspaces.json` 생성

### 출력 예시

```bash
$ pm init
✓ Created ~/.config/pm/
✓ Created config.json
✓ Created projects.json
✓ Created workspaces.json

PM initialized successfully!

$ pm init
✗ PM already initialized. Use --force to overwrite.

$ pm init --force
⚠ Existing configuration will be overwritten.
Continue? [y/N] y
✓ Reinitialized PM configuration.
```

---

## `pm load`

GitHub에서 PM 설정 불러오기.

### 사용법

```bash
pm load <username>/<repo>
```

### 동작

1. GitHub에서 설정 저장소 clone (임시 위치)
2. 설정 파일들을 `~/.config/pm/`으로 복사
3. 누락된 프로젝트 감지
4. 복구 워크플로우 시작 (interactive)
5. 임시 clone 삭제

### 출력 예시

```bash
$ pm load myuser/pm-config
✓ Cloning from github.com/myuser/pm-config...
✓ Loaded configuration

Found 5 projects, 2 missing:
  ✓ company-api     ~/work/company-api
  ✓ company-web     ~/work/company-web
  ✓ my-app          ~/github/my-app
  ✗ old-project     ~/github/old-project (not found)
  ✗ archived        ~/github/archived (not found)

Restore missing projects? [Y/n] y

[1/2] old-project
  Remote: git@github.com:myuser/old-project.git
  Original path: ~/github/old-project

  (r) Restore to original path
  (c) Clone to different path
  (s) Skip
  (d) Delete from config
  > r

✓ Cloned 'old-project' to ~/github/old-project

[2/2] archived
  Remote: (none)
  Original path: ~/github/archived

  (u) Update path manually
  (s) Skip
  (d) Delete from config
  > d

✓ Removed 'archived' from config

Summary:
  Restored: 1
  Skipped: 0
  Deleted: 1
```

### 에러

```bash
$ pm load myuser/pm-config
✗ Repository not found: github.com/myuser/pm-config

$ pm load myuser/pm-config
✗ PM already initialized. Use 'pm load --force' to overwrite.
```

---

## `pm save`

PM 설정을 GitHub에 저장.

### 사용법

```bash
pm save <username>/<repo>
```

### 동작

1. 저장소 존재 확인
   - 없으면: GitHub에 새 private 저장소 생성
   - 있으면: 기존 저장소 사용
2. 설정 파일들 복사
3. commit & push

### 출력 예시

```bash
$ pm save myuser/pm-config
✓ Repository exists: github.com/myuser/pm-config
✓ Copied configuration files
✓ Committed changes
✓ Pushed to origin

Configuration saved!

$ pm save myuser/pm-config
✓ Creating new private repository...
✓ Created github.com/myuser/pm-config
✓ Copied configuration files
✓ Initial commit
✓ Pushed to origin

Configuration saved!
```

### 저장되는 파일

```
pm-config/
├── config.json
├── projects.json
└── workspaces.json
```

### 주의사항

- GitHub CLI (`gh`) 또는 SSH 키 설정 필요
- 민감한 정보 (SSH 키 경로 등)는 상대 경로로 저장 권장

---

## 플러그인 시스템

### 파일 구조

```
~/.config/pm/
├── plugins/
│   ├── hooks/              # PM 이벤트 훅
│   │   ├── pre-switch.sh
│   │   ├── post-switch.sh
│   │   ├── pre-add.sh
│   │   └── post-add.sh
│   └── commands/           # 커스텀 명령어
│       ├── deploy/
│       │   ├── plugin.toml
│       │   └── main.sh
│       └── sync/
│           ├── plugin.toml
│           └── main.py
├── git-hooks/              # Git hooks 템플릿
│   ├── pre-commit
│   ├── commit-msg
│   └── pre-push
└── ...
```

### PM 이벤트 훅

pm 명령어 전/후 자동 실행되는 스크립트.

#### 지원 훅

| 훅 | 실행 시점 |
|-----|----------|
| `pre-add` | `pm add` 전 |
| `post-add` | `pm add` 후 |
| `pre-switch` | `pm sw` 전 |
| `post-switch` | `pm sw` 후 |
| `pre-remove` | `pm rm` 전 |
| `post-remove` | `pm rm` 후 |

#### 환경 변수

```bash
PM_PROJECT      # 프로젝트 이름
PM_PATH         # 프로젝트 경로
PM_WORKSPACE    # 현재 workspace
PM_COMMAND      # 실행된 명령어
```

#### 예시

```bash
# ~/.config/pm/plugins/hooks/post-switch.sh
#!/bin/bash

# .nvmrc 있으면 자동으로 nvm use
if [[ -f "$PM_PATH/.nvmrc" ]]; then
    nvm use
fi

# Python 프로젝트면 venv 활성화
if [[ -f "$PM_PATH/.venv/bin/activate" ]]; then
    source "$PM_PATH/.venv/bin/activate"
fi
```

---

### 커스텀 명령어 (Plugin)

사용자 정의 명령어를 추가.

#### plugin.toml

```toml
[plugin]
name = "deploy"
version = "0.1.0"
description = "Deploy project to server"
language = "sh"  # sh 또는 py

[command]
usage = "pm deploy [environment]"
```

#### 실행

```bash
$ pm deploy staging
# → ~/.config/pm/plugins/commands/deploy/main.sh staging 실행
```

#### 플러그인 관리

```bash
pm plugin ls                    # 설치된 플러그인 목록
pm plugin enable <name>         # 플러그인 활성화
pm plugin disable <name>        # 플러그인 비활성화
```

---

## Git Hooks 동기화

pm이 관리하는 git hooks를 프로젝트에 자동 설치.

### 설정

```json
// config.json
{
  "git_hooks": {
    "enabled": true,
    "auto_install": true
  }
}
```

### Git Hooks 템플릿

```
~/.config/pm/git-hooks/
├── pre-commit
├── commit-msg
└── pre-push
```

### 자동 설치

`auto_install: true`일 때:

```bash
$ pm add .
✓ Added 'my-app' to workspace 'default'
✓ Installed git hooks (pre-commit, commit-msg)

$ pm load myuser/pm-config
...
✓ Installed git hooks to 5 projects
```

### 수동 관리

```bash
pm hooks install [project]      # git hooks 설치 (기본: 현재 프로젝트)
pm hooks install --all          # 모든 프로젝트에 설치
pm hooks uninstall [project]    # git hooks 제거
pm hooks ls                     # 설치 상태 확인
pm hooks sync                   # 템플릿 변경사항 동기화
```

### 출력 예시

```bash
$ pm hooks ls
PROJECT         PRE-COMMIT  COMMIT-MSG  PRE-PUSH
company-api     ✓           ✓           ✓
company-web     ✓           ✓           ✗
my-app          ✗           ✗           ✗

$ pm hooks install my-app
✓ Installed git hooks to 'my-app'

$ pm hooks sync
✓ Updated pre-commit in 2 projects
✓ Updated commit-msg in 2 projects
```

---

## 기술 스택

### Dependencies

```toml
[package]
name = "pm"
version = "0.1.0"
edition = "2024"

[dependencies]
# CLI Framework
clap = { version = "4.5", features = ["derive"] }
clap_complete = "4.5"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Git Operations
git2 = "0.20"

# Error Handling
anyhow = "1.0"
thiserror = "2.0"

# Directories
dirs = "6.0"

# Output & Formatting
colored = "2.0"
tabled = "0.20"

# Date/Time
chrono = { version = "0.4", features = ["serde"] }
```

### 라이브러리 선택 이유

| 크레이트 | 버전 | 선택 이유 |
|----------|------|-----------|
| `clap` | 4.5 | derive 매크로로 선언적 CLI 정의, 자동 help 생성 |
| `clap_complete` | 4.5 | bash/zsh/fish 자동 완성 스크립트 생성 |
| `serde` + `serde_json` | 1.0 | Rust 표준 직렬화, JSON 설정 파일 |
| `git2` | 0.20 | libgit2 바인딩, 순수 Rust git 작업 |
| `anyhow` | 1.0 | 애플리케이션 레벨 에러 처리 |
| `thiserror` | 2.0 | 커스텀 에러 타입 정의 |
| `dirs` | 6.0 | 크로스 플랫폼 디렉토리 경로 (~/.config 등) |
| `colored` | 2.0 | 터미널 색상 출력 |
| `tabled` | 0.20 | 테이블 형식 출력 |
| `chrono` | 0.4 | 날짜/시간 처리 및 직렬화 |

---

## 미정 사항

- [ ] `pm info <project>` - 프로젝트 상세 정보
- [ ] `pm open <project>` - 에디터에서 열기
- [ ] `pm status [project]` - git 상태 요약
- [ ] `pm find <query>` - 프로젝트 검색
- [ ] `pm fetch` / `pm pull` - 전체 프로젝트 git 작업

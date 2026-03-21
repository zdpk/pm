# PM Plugin System 구현 프롬프트

## 배경

PM은 Git 프로젝트 디렉토리를 관리하는 Rust CLI 도구다 (`~/.config/pm/`).
현재 프로젝트 등록, workspace, 이동, 태그 등 핵심 기능은 구현되어 있다.

SPEC.md에 플러그인 시스템이 설계되어 있지만 아직 미구현이다.
이번 작업은 **플러그인 시스템을 구현**하고, 첫 번째 플러그인으로 **skill manager(sc)**를 연동하는 것이다.

---

## 1. 플러그인 시스템 구현

### 1.1 디렉토리 구조

```
~/.config/pm/
├── config.json
├── projects.json
├── workspaces.json
└── plugins/
    └── commands/           # 커스텀 명령어 플러그인
        └── skills/
            ├── plugin.toml
            └── main.py
```

### 1.2 plugin.toml 스키마

```toml
[plugin]
name = "skills"
version = "0.1.0"
description = "AI agent skill manager — registry, deploy, sync"
language = "py"        # "sh" | "py"
entry = "main.py"      # 기본값: main.{language확장자}

[command]
usage = "pm skills <subcommand> [options]"
aliases = ["sk"]       # pm sk list 등 단축 가능
```

### 1.3 PM 측 구현 요구사항

#### CLI 변경 (cli.rs)

```rust
// 기존 Commands enum에 추가:
/// Run a plugin command
Plugin {
    /// Plugin name
    name: String,
    /// Arguments to pass to the plugin
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
},
```

단, `pm skills list`처럼 자연스럽게 쓰려면 **알려지지 않은 서브커맨드를 plugin으로 라우팅**하는 방식이 더 좋다:

```rust
// main.rs에서 clap 파싱 실패 시 → plugin fallback
fn main() -> Result<()> {
    match Cli::try_parse() {
        Ok(cli) => dispatch(cli),
        Err(e) => {
            // 알려지지 않은 서브커맨드면 plugin에서 찾기
            let args: Vec<String> = std::env::args().collect();
            if args.len() > 1 {
                if let Some(plugin) = find_plugin(&args[1]) {
                    return run_plugin(plugin, &args[2..]);
                }
            }
            e.exit();
        }
    }
}
```

#### Plugin Discovery (새 모듈: src/plugin.rs)

```rust
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub language: PluginLanguage,
    pub entry: PathBuf,       // 절대 경로
    pub aliases: Vec<String>,
    pub dir: PathBuf,         // plugin 디렉토리
}

pub enum PluginLanguage {
    Python,
    Shell,
}

/// ~/.config/pm/plugins/commands/ 스캔하여 plugin 목록 반환
pub fn discover_plugins() -> Vec<Plugin>;

/// 이름 또는 alias로 plugin 찾기
pub fn find_plugin(name: &str) -> Option<Plugin>;

/// plugin 실행 — subprocess 생성
pub fn run_plugin(plugin: &Plugin, args: &[String]) -> Result<()>;
```

#### Plugin 실행 시 환경변수

plugin 프로세스에 다음 환경변수를 주입:

```
PM_CONFIG_DIR     = ~/.config/pm
PM_PROJECTS_FILE  = ~/.config/pm/projects.json
PM_WORKSPACES_FILE = ~/.config/pm/workspaces.json
PM_PLUGIN_DIR     = ~/.config/pm/plugins/commands/skills
PM_PROJECT        = maple          (현재 dir이 등록된 프로젝트면)
PM_PROJECT_PATH   = ~/workspace/maple
PM_WORKSPACE      = personal       (현재 workspace)
PM_PROJECT_TAGS   = rust,cli,maple (프로젝트 태그, 쉼표 구분)
```

`PM_PROJECT`가 가장 중요하다. 이 값이 있으면 plugin은 "현재 어떤 프로젝트에서 실행되는지" 안다.

#### 현재 프로젝트 감지 로직

pm은 이미 workspace 자동 감지가 있다 (SPEC.md "Workspace 자동 감지" 참조).
동일한 로직으로 현재 디렉토리가 등록된 프로젝트인지 판별:

```rust
/// 현재 디렉토리가 등록된 프로젝트 내부인지 확인
pub fn detect_current_project(projects: &[Project]) -> Option<&Project> {
    let cwd = std::env::current_dir().ok()?;
    let cwd_str = collapse_path(&cwd);

    for project in projects {
        let project_path = expand_path(&project.path);
        if cwd.starts_with(&project_path) {
            return Some(project);
        }
    }
    None
}
```

#### Plugin 관리 명령어

```bash
pm plugin list              # 설치된 플러그인 목록
pm plugin enable <name>     # 활성화 (기본값)
pm plugin disable <name>    # 비활성화
```

이건 최소한으로 — `plugin.toml`에 `enabled = false` 필드 추가하는 정도.
초기 구현에서는 `pm plugin list`만 있으면 충분하다.

### 1.4 Plugin 설치 흐름

플러그인 설치는 수동이다 (Phase 1):

```bash
# sc 플러그인 설치 예시
mkdir -p ~/.config/pm/plugins/commands/skills
cp plugin.toml main.py ~/.config/pm/plugins/commands/skills/
```

향후 `pm plugin install <source>` 추가 가능하지만 지금은 scope 밖.

---

## 2. sc (Skill Manager) — 첫 번째 플러그인

### 2.1 sc란 무엇인가

AI 에이전트(Claude Code, Codex, Gemini, OpenCode)가 사용하는 **skill 파일들의 중앙 레지스트리 + 배포 도구**.

- Skill = `SKILL.md` 파일을 포함하는 디렉토리. AI agent에게 특정 능력을 부여하는 프롬프트.
- Claude Code는 `.claude/skills/`, Codex는 `.codex/skills/`에서 skill을 로드한다.
- sc는 skill들을 중앙(`~/.config/pm/skills/`)에서 관리하고, 프로젝트에 symlink로 배포한다.

### 2.2 현재 sc 아키텍처 (이관 대상)

**기존**: 독립 Python CLI (`sc`), 자체 데이터 저장소 (`skills/` 하위).
**이관 후**: pm plugin으로 동작, 데이터를 `~/.config/pm/skills/`에 저장.

#### 데이터 구조 (이관 후)

```
~/.config/pm/
├── skills/                         ← skill store (new)
│   ├── registry.yaml               ← skill 목록 + 메타데이터
│   ├── custom/                     ← 직접 만든 skill
│   │   ├── dev/
│   │   │   └── vitest/
│   │   │       └── SKILL.md
│   │   ├── meta/
│   │   │   └── sc/
│   │   │       └── SKILL.md
│   │   └── ...
│   └── downloaded/                 ← 커뮤니티에서 받은 skill
│       ├── shadcn-ui/
│       │   └── SKILL.md
│       └── ...
└── plugins/
    └── commands/
        └── skills/
            ├── plugin.toml
            └── main.py             ← sc 로직
```

#### registry.yaml 포맷

```yaml
version: 1

profiles:
  dev:
    description: 개발 프로젝트 공통
    tags: [dev, openspec, util, meta]
  maple:
    description: 메이플스토리 BGM 프로젝트
    tags: [maple, content, imagegen, youtube]
  full:
    description: 모든 스킬
    tags: ['*']

skills:
  # ── Custom (43) ─────────────────────────────────────────
  - name: vitest
    source: custom
    agent: universal
    tags: [dev, testing]
    path: skills/custom/dev/vitest
    description: Vitest testing patterns

  # ── Downloaded (42) ─────────────────────────────────────
  - name: shadcn-ui
    source: downloaded
    agent: claude
    tags: [dev, frontend]
    path: skills/downloaded/shadcn-ui
    description: shadcn/ui component patterns
    install_source: "github:user/repo/path"

  # ── Builtin (3) ────────────────────────────────────────
  - name: memory
    source: builtin
    agent: claude
    tags: [meta]
    description: Claude built-in memory
```

필드 설명:
- `name`: 고유 식별자
- `source`: `custom` | `downloaded` | `builtin`
- `agent`: `claude` | `codex` | `gemini` | `opencode` | `universal`
- `tags`: skill 분류 태그 (배포 필터링에 사용)
- `path`: SKILL.md가 있는 디렉토리 (STORE_DIR 기준 상대경로)
- `description`: 한 줄 설명
- `install_source` (optional): downloaded skill의 원본 (`github:owner/repo/path`, `url:...`)
- `disabled` (optional): true면 배포 제외

#### 서브커맨드

```bash
pm skills list [--source X] [--agent X] [--tag X] [--format table|json|yaml]
pm skills info <name>
pm skills add <name> --source X --agent X --tags X [--path X] [--description X]
pm skills remove <name> [--delete-files] [--force]
pm skills update <name> [--add-tags X] [--remove-tags X] [--agent X] [--description X]
pm skills deploy [profile] [--tags X] [--agent X] [--dry-run] [--clean] [--force]
pm skills scan [--register]
pm skills verify
pm skills diff [--format table|json]
pm skills import <path> [--name X] [--category X] [--agent X] --tags X
pm skills install [--only X] [--force]
```

### 2.3 deploy 흐름 (pm 연동의 핵심)

**기존 sc**: `sc deploy dev --target ~/project-a` (target 필수, 수동 지정)

**pm plugin으로**: pm이 프로젝트 컨텍스트를 제공하므로 더 자연스러워진다.

```bash
cd ~/workspace/maple
pm skills deploy
```

내부 동작:

```
1. PM이 환경변수 주입
   PM_PROJECT=maple
   PM_PROJECT_PATH=~/workspace/maple
   PM_PROJECT_TAGS=rust,maple,youtube

2. skills plugin이 profile 결정
   a. 명시적 인자: pm skills deploy maple → maple profile
   b. PM_PROJECT_TAGS 기반 자동 매칭:
      project tags [maple, youtube] → profiles 중 maple이 가장 매칭
   c. default_profile fallback

3. profile → tags → skill 필터링
   maple profile → tags: [maple, content, imagegen, youtube]
   → 해당 tag가 있는 skill만 선택

4. agent 자동 감지
   .claude/ 있으면 → claude agent
   .codex/ 있으면 → codex agent
   → agent 호환 skill만 선택

5. symlink 생성
   ~/.config/pm/skills/custom/dev/vitest → ~/workspace/maple/.claude/skills/vitest
```

#### Profile-Project 매핑 전략

가장 깔끔한 접근: **pm의 project tags가 profile 선택 힌트**가 된다.

```bash
# pm에서 프로젝트에 tag가 이미 있다
pm add ~/workspace/maple --tags maple,youtube

# skills deploy 시 자동 매칭:
# project tags: [maple, youtube]
# profiles:
#   dev: [dev, openspec]        → 교집합 0개
#   maple: [maple, youtube, ...] → 교집합 2개 ← 최다 매칭
# → maple profile 자동 선택
```

명시적 지정도 가능:
```bash
pm skills deploy dev     # 강제로 dev profile 사용
pm skills deploy --tags dev,maple  # profile 대신 직접 tags 지정
```

### 2.4 sc 코드 이관 시 변경사항

현재 sc에서 바뀌는 것:

| 현재 | 이관 후 |
|------|---------|
| `REPO_ROOT = SCRIPT_DIR.parent` | `STORE_DIR = Path(os.environ.get("PM_CONFIG_DIR", "~/.config/pm"))` |
| `REGISTRY_PATH = REPO_ROOT / "skills" / "registry.yaml"` | `REGISTRY_PATH = STORE_DIR / "skills" / "registry.yaml"` |
| `expand_path`: REPO_ROOT 기준 상대경로 | STORE_DIR 기준 상대경로 |
| `cmd_deploy --target` 필수 | `--target` 생략 시 `PM_PROJECT_PATH` 사용 |
| argparse로 `sc deploy ...` | `pm skills deploy ...` (args는 PM이 전달) |
| 독립 실행 `sc` | `pm skills` 로 호출. 단독 `sc`도 유지 가능 (호환) |

main.py의 엔트리포인트:

```python
#!/usr/bin/env python3
"""PM skills plugin — AI agent skill registry manager."""

import sys
import os
from pathlib import Path

# Store 경로 결정
STORE_DIR = Path(os.environ.get("PM_CONFIG_DIR", str(Path.home() / ".config" / "pm")))
SKILLS_DIR = STORE_DIR / "skills"
REGISTRY_PATH = SKILLS_DIR / "registry.yaml"

def main():
    # sys.argv[0] = "pm skills"
    # sys.argv[1:] = plugin에 전달된 args
    # 예: pm skills list --tag dev → args = ["list", "--tag", "dev"]
    ...

if __name__ == "__main__":
    main()
```

---

## 3. 구현 순서

### Phase 1: PM Plugin 인프라

1. `src/plugin.rs` 모듈 생성
   - `Plugin` struct, `PluginLanguage` enum
   - `discover_plugins()`: `~/.config/pm/plugins/commands/` 스캔
   - `find_plugin()`: 이름/alias 매칭
   - `run_plugin()`: subprocess 실행 + 환경변수 주입
   - TOML 파싱은 `toml` crate 사용

2. `src/main.rs` 수정
   - clap 파싱 실패 시 plugin fallback 로직
   - 또는 `Commands::Plugin` variant 추가

3. `src/commands/plugin.rs` 생성
   - `pm plugin list` 구현

4. 프로젝트 감지 함수
   - `detect_current_project()`: cwd → Project 매핑
   - 환경변수 세팅 로직

5. `Cargo.toml` 의존성 추가
   - `toml = "0.8"` (plugin.toml 파싱)

### Phase 2: skills 플러그인 작성

1. `~/.config/pm/plugins/commands/skills/plugin.toml` 생성
2. `main.py` 작성 (기존 sc 로직 이관)
3. PM 환경변수 활용하여 deploy 자동화
4. 테스트: `pm skills list`, `pm skills deploy` 등

### Phase 3: 데이터 마이그레이션

1. 기존 `~/skills/dotfiles/skills/` → `~/.config/pm/skills/` 이동
2. registry.yaml의 path 필드 확인 (상대경로 기준 변경)
3. 기존 symlink 정리 및 재배포

---

## 4. 기술 제약사항

- pm은 Rust (edition 2024), plugin 시스템도 Rust로 구현
- plugin 자체는 Python 또는 Shell (subprocess로 실행)
- plugin과 pm 간 통신은 **환경변수 (입력) + stdout/stderr (출력) + exit code** 만 사용
- JSON RPC 등 복잡한 IPC는 사용하지 않는다
- plugin.toml 파싱에 `toml` crate 사용
- plugin 실행 시 pm은 단순히 subprocess를 spawn하고 stdio를 패스스루한다

---

## 5. 테스트 시나리오

```bash
# Plugin discovery
pm plugin list
# NAME     VERSION  LANGUAGE  DESCRIPTION
# skills   0.1.0    python    AI agent skill manager

# 기본 skill 조회
pm skills list
pm skills list --tag dev --format json
pm skills info vitest

# 프로젝트에서 deploy
cd ~/workspace/maple        # pm에 maple로 등록되어 있음
pm skills deploy             # 자동 profile 매칭 → deploy
pm skills deploy --dry-run   # 미리보기
pm skills deploy dev         # 명시적 profile

# Skill 관리
pm skills add my-tool --source custom --agent universal --tags dev,util
pm skills scan
pm skills verify
pm skills diff
```


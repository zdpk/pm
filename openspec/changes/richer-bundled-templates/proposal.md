## Why

`pm project init` 의 bundled `.gitignore` 템플릿은 stack 별로 4–10줄 짜리 미니 파일이다 (예: `rust/common`은 `/target`, `*.rs.bk`, `*.pdb` 단 3줄). 실세계 프로젝트의 ignore 룰 — IDE 잡파일 (`.idea/`, `.vscode/*` 외 많음), OS 메타 (`.DS_Store`, `Thumbs.db`), 빌드 산출물 변형, 테스트 커버리지, 캐시 디렉토리 등 — 을 전혀 못 커버한다. 사용자는 매번 검색해서 추가 룰을 직접 채워 넣어야 한다.

산업 표준은 [`github/gitignore`](https://github.com/github/gitignore) (CC0 라이선스, GitHub 의 "Add .gitignore" 버튼이 사용하는 그 저장소) 다. 이 템플릿을 빌드 타임에 pm 바이너리에 임베딩해 풍부한 ignore 룰을 기본 제공하면, 사용자는 `pm project init` 한 번으로 production-quality `.gitignore` 를 얻는다. 기존 `merged` strategy 가 그대로 사용자 추가분을 보존하므로 호환성 문제 없음.

## What Changes

### 신규: `vendor/github-gitignore/` git submodule
- `github/gitignore` 저장소를 pinned commit 으로 vendor 한다.
- 메인테이너가 주기적으로 `git submodule update --remote` 로 갱신.
- CI 빌드는 `git submodule update --init` 후 진행.

### `build.rs` 빌드 스크립트 신설
- `vendor/github-gitignore/` 에서 stack-별 템플릿 파일을 읽어 `OUT_DIR` 의 Rust 파일로 `include_str!` 친화적 형태로 export.
- 빌드 시 vendor 디렉토리 변경 감지 (`cargo:rerun-if-changed=vendor/github-gitignore`).
- 임베딩 대상 (10개): `Rust.gitignore`, `Node.gitignore`, `Python.gitignore`, `Dart.gitignore`, `Go.gitignore`, `Global/macOS.gitignore`, `Global/Linux.gitignore`, `Global/Windows.gitignore`, `Global/VisualStudioCode.gitignore`, `Global/JetBrains.gitignore`.

### 합성 (composition) 로직 — `pm project init` / `sync` 시점
순서대로 라인을 합쳐 `.gitignore` 생성 (`merged` strategy 가 사용자 영역 보존):
1. **Global OS**: `macOS.gitignore` + `Linux.gitignore` (Windows 는 옵션 — pm 자체가 Unix only 이지만 팀원이 있을 수 있음)
2. **Global IDE**: `VisualStudioCode.gitignore` + `JetBrains.gitignore`
3. **Language**: `Rust.gitignore` / `Node.gitignore` / 등
4. **Framework-specific**: `configs/<lang>/<fw>/.gitignore.extra` (pm 이 직접 관리, 짧음 — 예: nextjs 의 `.next/`, `out/`)
5. **pm 마커**: `# >>> pm managed >>>` … `# <<< pm managed <<<` 영역 안에 위 1–4 의 합성 결과. 사용자가 마커 밖에 추가한 라인은 절대 수정 안 함.

### 신규 명령 `pm project gitignore`
- `pm project gitignore` — 현재 프로젝트의 `.gitignore` 를 합성한 결과로 갱신 (init/sync 와 별개로 호출 가능)
- `pm project gitignore --diff` — 현재 합성 결과와 디스크의 차이만 출력
- `pm project gitignore --categories <list>` — 사용자가 직접 카테고리 조합 선택 (`macos,vscode,rust,jetbrains`)

### `configs/<lang>/common/.gitignore` 의 역할 변경
- 기존: 자체 라인 (예: `node_modules/`)
- 변경: pm 관리 영역의 출처를 명시한 placeholder 만 두고, 실제 내용은 합성 시점에 채움
- (선택) `configs/<lang>/<fw>/.gitignore.extra` 신설 — framework 특화 추가 라인 (예: `ts/nextjs/.gitignore.extra` 에 `.next/` 등)

### 라이선스
- `LICENSES/github-gitignore-CC0.txt` 추가 (github/gitignore 의 CC0 라이선스 명시)
- README 의 "Acknowledgements" 섹션에 출처 표시

### Non-changes (이번 change 범위 밖)
- gitignore.io API 온라인 모드 — 차후 별도 change `online-gitignore-source`
- `.editorconfig` 분리 — 단일 파일 형식이 충분히 표현력 있어 분리 실익 작음
- `merged` strategy 자체 변경 — 이미 v0.4.0 에서 사용 중

## Capabilities

### New Capabilities
- `bundled-templates`: `pm project init/sync/gitignore` 가 어떤 출처에서 어떤 순서로 ignore/config 템플릿을 합성하는지 정의. github/gitignore vendoring, 합성 순서, pm 마커 블록, framework 별 extra 라인.

### Modified Capabilities
<!-- 비어 있음. 기존 spec 에는 .gitignore 합성/marker 관련 Requirement 가 없음. -->

## Impact

### Code
- `build.rs` 신설 — `vendor/github-gitignore/` → `OUT_DIR/embedded_gitignore.rs` 변환
- `Cargo.toml` — `build = "build.rs"` 또는 default 사용
- `src/templates/mod.rs` 신설 (또는 `project.rs` 확장) — 합성 함수, marker block 처리, framework-extra 머지
- `src/cli.rs` — `ProjectCommand::Gitignore { diff, categories }` 추가
- `src/commands/project.rs` — `cmd_gitignore` 핸들러, `cmd_init` / `cmd_sync` 가 새 합성 경로 사용

### Repository
- `vendor/github-gitignore/` git submodule 추가 (`.gitmodules` 신설)
- `configs/<lang>/common/.gitignore` 의 컨텐츠 축소 — placeholder 만, 실내용은 vendoring 에서
- `configs/<lang>/<fw>/.gitignore.extra` 신설 (framework 별 추가 라인)
- `LICENSES/github-gitignore-CC0.txt` 추가
- README "Acknowledgements" 섹션

### Build / CI
- `release.yml` 에 `submodules: recursive` 옵션 추가 (`actions/checkout@v4` 의 `submodules: true`)
- 첫 빌드 시 약 +수 MB 디스크 (vendor/github-gitignore is small, ~수백 KB)
- 임베딩으로 인한 바이너리 크기 ↑ 추정 +50–100 KB (10개 파일, 각 1–2 KB)

### User-facing
- 기존 사용자 영향: `pm project sync` 시점에 `.gitignore` 가 풍부해짐. `merged` strategy 라 사용자 추가 라인은 보존되지만, **이전에 pm 이 추가한 4–10줄도 그대로 남음** (사용자가 수정한 것으로 보일 수 있음). pm 마커 도입 시 첫 sync 후 깔끔히 정리됨.
- 신규 사용자: `pm project init` 한 번으로 production-quality `.gitignore`.

### Versioning
- v0.5.0 — minor bump (BREAKING 없음, 신규 기능 + 풍부화)

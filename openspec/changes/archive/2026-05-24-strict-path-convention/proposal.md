## Why

`pm`은 현재 워크스페이스와 프로젝트 경로를 자유롭게 지정할 수 있다. `Workspace.root`로 임의 절대경로를 지정할 수 있고, `Project.dir`도 워크스페이스 루트 아래라면 어디든 들어갈 수 있어 경로가 결정적이지 않다. 결과적으로 같은 매니페스트가 머신마다 다른 위치를 가리키고, 백업·동기화·이식성이 어려워진다. 하위 호환을 고려하지 않을 수 있는 지금 시점에 단순한 컨벤션으로 리셋해 경로를 이름의 결정적 함수로 만든다.

## What Changes

- **BREAKING** `Workspace.root` 필드를 제거한다. 워크스페이스 디렉토리는 항상 `base_root/<workspace>/`이다.
- **BREAKING** `Project.dir`는 단일 디렉토리 이름만 허용한다. 절대경로, `..`, `/`, `\` 금지. 경로 분리자 검증을 매니페스트 로딩과 명령어 입력 양쪽에서 수행한다.
- **BREAKING** 프로젝트 경로 계산을 `base_root/<workspace>/<project.dir>`로 결정적으로 단순화한다. `workspace_root()`는 컨벤션 한 줄로 축소된다.
- **BREAKING** `pm add`의 의미를 "외부 경로 등록"에서 "이 위치로 clone/move"로 바꾼다. 이미 다른 곳에 있는 git 저장소를 추가하면 `base_root/<workspace>/<project>`로 이동시킨다.
- **BREAKING** `normalized_workspace_root` 및 워크스페이스 루트 정규화 로직 제거.
- **BREAKING** 외부 경로 사용처는 본 change에서 모두 차단한다. 외부 디렉토리를 1급 시민으로 다루는 `pm link` 같은 별도 개념은 후속 change로 분리한다.
- `base_root`는 사용자 설정 가능하게 유지한다 (기본값 `~/projects`). 단 한 곳에서만 자유도를 둔다.

## Capabilities

### New Capabilities

- `path-convention`: 워크스페이스/프로젝트 디렉토리 결정 규칙, 검증, 그리고 매니페스트와 파일시스템 간 결정적 매핑을 정의한다.

### Modified Capabilities

(없음 — path 컨벤션을 다루는 기존 spec이 없다.)

## Impact

- **모델**: `src/project.rs`의 `Workspace`(`root` 필드 제거), `Project`(`dir` 검증 강화)
- **경로 함수**: `src/state.rs`의 `workspace_root`, `project_path`, `relative_dir`, `normalized_workspace_root`, `detect_current_project`
- **설정/검증**: `src/config.rs`의 `projects_path`, `workspaces_path`, `relative_or_basename`
- **명령어**: `pm add`, `pm sync`, `pm restore`, 워크스페이스 생성·열거 관련 모든 커맨드 (`src/commands/`)
- **마이그레이션**: 본 change는 하위 호환을 고려하지 않으므로 기존 매니페스트가 비표준 경로를 담고 있으면 로딩 단계에서 명확한 에러로 실패해야 한다. 마이그레이션 도구는 별도 change.
- **문서**: `README.md`, `README-ko.md`의 경로 설명, `pm init` 출력

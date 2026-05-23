## Why

`strict-path-convention`(v4)에서 프로젝트 경로는 `base_root/<workspace>/<project.dir>`로 결정적이고, 매니페스트가 desired state·파일시스템이 observed state라는 모델(D8)이 확립된다. 결정적 경로 덕분에 처음으로 `base_root/*/*`를 기계적으로 스캔해 매니페스트와 양방향 비교할 수 있다 — 전엔 워크스페이스 루트가 임의 경로라 "어디를 스캔해야 하는지"조차 알 수 없었다.

현재 `pm check`는 매니페스트→파일시스템 단방향 진단(경로 없음/비-git/remote 불일치 보고)만 하고, `pm sync`는 매니페스트에 있는데 사라진 것을 remote에서 복원만 한다. **반대 방향**(디스크엔 있는데 매니페스트엔 없는 untracked 디렉토리 탐지)과 **prune**(사라진 항목을 확인 후 정리)이 없어, 사용자가 `pm` 밖에서 `mkdir`/`git clone`/`rm`/`mv`로 만든 drift를 해소할 길이 없다.

이 change는 D8 모델 위에서 양방향 reconcile을 완성한다.

## What Changes

- `pm check`(진단, 읽기 전용)에 **untracked 디렉토리 탐지**를 추가한다. `base_root/<workspace>/<dir>` 중 매니페스트에 없는 git 디렉토리를 `untracked`로 보고한다. 매니페스트는 변경하지 않는다.
- `pm sync`(수리)에 **양방향 reconcile**을 추가한다:
  - **adopt**: untracked git 디렉토리를 확인 후 매니페스트에 등록(메타데이터는 디렉토리명/remote에서 추론).
  - **prune**: 디스크에서 사라진 매니페스트 항목을 확인 후 제거하고 history(`Unregistered`)에 기록.
  - 기존 동작(사라진 항목을 remote에서 re-clone)은 유지.
- 멤버십 SoT는 매니페스트이므로 adopt/prune 모두 **자동 수행하지 않고 명시적 확인**을 받는다. prune은 데이터 손실 위험이라 특히 그렇다(D8). `--yes`로 비대화형 일괄 처리, `--dry-run`으로 변경 미리보기를 제공한다.
- reconcile 대상 분류와 처리 정책을 한 곳(스캔→분류→액션)으로 모은다.

## Capabilities

### New Capabilities

- `path-reconcile`: `base_root` 스캔으로 매니페스트(desired)와 파일시스템(observed)의 drift를 분류하고, 확인 기반으로 adopt/prune/restore를 적용하는 규칙을 정의한다.

### Modified Capabilities

- `path-convention`: D8(desired/observed) 모델을 전제로 참조하되, 본 change는 그 위에서 동작하는 reconcile 동작만 추가한다. (path-convention 자체 규칙은 변경하지 않음.)

## Impact

- **명령어**: `pm check`(`src/commands/check.rs`) — untracked 탐지 추가, `pm sync`(`src/commands/sync.rs`) — adopt/prune 추가, `--dry-run` 옵션.
- **스캔**: `base_root/*/*` 열거 유틸 신규 (워크스페이스 디렉토리 → 프로젝트 디렉토리), 시스템 워크스페이스(`.`-prefix) 포함 여부 정책.
- **모델/상태**: drift 분류 타입(orphan/untracked/conflict), adopt 시 `Project` 메타데이터 추론(remote→repo_slug), prune 시 history 기록(`src/history.rs`, `HistoryAction::Unregistered`).
- **CLI**: `src/cli.rs`의 `Sync`/`Check`에 옵션 추가, 셸 완성.
- **의존성**: `strict-path-convention`(v4, D8) 위에서만 의미가 성립 — 그 change가 선행되어야 한다.
- **문서**: `README.md`, `README-ko.md`에 reconcile 워크플로우 설명.

## Non-Goals

- 외부 경로를 1급 시민으로 두는 `pm link`. 별도 개념.
- `base_root` 변경 시 기존 디렉토리 자동 이동.
- 비-git untracked 디렉토리의 자동 처리(보고만).

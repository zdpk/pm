## Context

`pm`은 워크스페이스와 프로젝트의 물리 경로를 두 단계 자유 변수로 다룬다.

- `Workspace.root: Option<String>` — 임의 절대경로로 워크스페이스 디렉토리 위치를 덮어쓸 수 있다.
- `Project.dir: String` — 워크스페이스 루트를 기준으로 하지만 중첩 경로(`a/b/c`)도 들어갈 수 있다.

결과적으로 매니페스트만 보고는 "이 프로젝트가 어디에 있어야 하는지"를 알 수 없고, 같은 매니페스트가 머신마다 다른 위치를 가리킨다. `src/state.rs:58-69`의 `workspace_root`/`project_path`, `src/state.rs:122-`의 `normalized_workspace_root`, `src/state.rs:106-119`의 `relative_dir`이 이 자유도를 흡수하는 데 쓰이고 있다.

하위 호환을 고려하지 않을 수 있는 시점이라, 자유도를 한 군데(`base_root`)만 남기고 모든 경로를 이름으로부터 결정적으로 계산한다.

## Goals / Non-Goals

**Goals:**

- 매니페스트 + `base_root`만으로 모든 워크스페이스/프로젝트의 절대경로를 결정적으로 계산할 수 있다.
- `Workspace.root` 필드 제거. `Project.dir`는 단일 디렉토리 이름으로 제한.
- `pm add`는 "이 위치로 가져온다"의 의미를 갖는다 (clone/move).
- 매니페스트 로딩 시 새 규칙 위반은 명확한 에러로 실패한다.
- `src/state.rs`의 경로 함수를 컨벤션 한 줄짜리 조합으로 축소한다.

**Non-Goals:**

- 기존 매니페스트의 자동 마이그레이션. 본 change는 호환을 깬다.
- 외부 경로를 1급으로 다루는 기능(`pm link`). 별도 change.
- 시스템 워크스페이스(`.trash` 등) 의미 변경. 컨벤션은 동일하게 적용되되 동작은 유지.
- 멀티 `base_root` 지원.

## Decisions

### D1. 워크스페이스 디렉토리는 `base_root/<workspace>/`로 고정

대안:
- (A) 현행처럼 `workspace.root`로 오버라이드 허용.
- (B) `base_root`도 워크스페이스별로 분리.
- (C) **선택**: 단일 `base_root` + 이름 기반 결정.

**선택 이유**: 매니페스트에서 위치 정보를 분리해야 결정적 매핑이 성립한다. 위치 오버라이드가 필요한 케이스는 후속 `pm link`로 분리.

### D2. `Project.dir`는 단일 디렉토리 이름

검증 규칙:
- 비어있지 않다.
- 경로 분리자 `/`, `\` 미포함.
- `.`, `..`, 절대경로 시작(`/`, `C:\` 등) 금지.
- OS 예약어/널 바이트 등 표준 안전성 체크.
- 길이 상한은 컨벤션 우선이라 두지 않음 (파일시스템이 거절하게).

검증 위치: 매니페스트 로딩(`load_manifest`), `pm add`/`pm rename` 같은 입력 지점 둘 다. 로딩 단에서 거르면 잘못된 매니페스트가 명령어 동작 중에 발견되지 않는다.

대안: dir도 자유롭게 두고 결과 경로만 검증 → 사용자 의도와 결과 사이 거리가 멀어 거부.

### D3. `pm add`는 "물리적으로 그 자리에 둔다"

- 새 git URL을 받으면 `base_root/<workspace>/<project>`에 clone.
- 이미 다른 곳에 클론된 디렉토리를 받으면 그 자리로 이동(move). 이동 실패 시 에러로 중단하고 매니페스트는 건드리지 않는다.
- 대상 경로가 이미 존재하면 에러.

대안: 외부 경로 등록 허용 → Goals와 충돌. 별도 change에서 다룬다.

### D4. 경로 계산 함수 단순화

`workspace_root`/`project_path`는 다음 한 줄로 축소된다.

```
fn workspace_dir(base_root: &Path, workspace: &str) -> PathBuf {
    base_root.join(workspace)
}
fn project_dir(base_root: &Path, workspace: &str, project_dir: &str) -> PathBuf {
    base_root.join(workspace).join(project_dir)
}
```

`Workspace`를 받지 않아도 되므로 시그니처가 간결해진다. 호출부는 `manifest`에서 이름만 조회한다.

`detect_current_project`는 cwd가 `base_root`로 시작하는지 확인 → 그 뒤 두 컴포넌트(`<workspace>`, `<project>`)로 직접 매니페스트 조회로 단순화된다. prefix 최장일치 루프 제거.

### D5. 매니페스트 스키마 버저닝

`pm`은 이미 v1→v2→v3 마이그레이션 체인을 가진다. 이번 변경은 **v4** 스키마로 올린다.

- v3 매니페스트가 로드되면, 본 change는 호환을 깨므로 "v4 strict path convention과 호환 안 됨" 에러로 즉시 실패.
- 클린 리셋이므로 자동 변환·마이그레이션 도구를 제공하지 않는다. 사용자는 새 컨벤션으로 워크스페이스를 다시 만든다.

### D6. 시스템 워크스페이스 (`.`-prefix) 처리

컨벤션은 동일하게 적용. 즉 `.trash`도 `base_root/.trash/`에 존재한다. 숨김 디렉토리 특성을 활용하는 기존 동작은 유지된다.

### D7. 검증 실패 시 매니페스트 보존

매니페스트 로딩에서 검증이 실패하면:
- 명확한 에러 (필드명, 위반 값, 위반 규칙) 출력.
- 매니페스트 자동 수정 안 함.
- 사용자가 `pm check`/`pm rm` 등으로 명시적으로 해결.

**중요**: 여기서의 "검증"은 경로의 *모양*(separator, `..`, 절대경로 등) 검증이다. 디렉토리의 *물리적 존재* 검증이 아니다. 프로젝트 하나가 디스크에서 사라졌다고 매니페스트 로딩 전체가 실패하면 안 된다 (→ D8).

### D8. 매니페스트는 desired state, 파일시스템은 observed state

`pm`은 두 종류의 사실을 분리해 다룬다.

- **의도/멤버십/메타데이터** (어떤 프로젝트를 관리하는가, tags·note·remote·history 등): **매니페스트가 source of truth**. 디스크에서 복원 불가능하고, 결정적 경로 컨벤션(`base_root + 매니페스트`로 모든 위치 계산)의 전제이기도 하다.
- **물리적 실재** (지금 디스크에 디렉토리가 있는가): **파일시스템이 권위**. 매니페스트가 이를 우길 수 없다.

따라서 매니페스트는 *원하는 상태*의 선언이고, 파일시스템은 *관측된 상태*다. 둘의 간극이 drift이며, reconcile은 desired 쪽으로 수렴시킨다 (Kubernetes/Terraform 모델).

drift 처리 원칙:
- **매니페스트엔 있고 디스크엔 없음**: actual이 desired에서 벗어남. 매니페스트가 옳다고 보고 desired로 복원(remote 있으면 re-clone). 매니페스트를 지우는 게 아니라 디스크를 맞춘다.
- **디스크엔 있고 매니페스트엔 없음**: 선언되지 않음. 매니페스트가 멤버십의 SoT이므로 **자동 채택하지 않는다**. 보고만 하고, 사용자가 `pm add`로 명시적으로 선언해야 멤버가 된다.
- **접근 시점**(switch/run)에 desired는 있으나 디스크에 없으면: 없는 척하거나 조용히 생성하지 않고 명확히 "없음 → 복원하라"고 알린다.
- 매니페스트 항목 삭제(prune)는 *SoT 편집*이므로 우발적 drift에 대한 자동 반응이 아니라 의도적·확인된 행위(`pm rm` 또는 reconcile에서 명시적 확인)여야 한다.

본 change는 이 모델을 명문화하고, mv·접근 시점 동작·로딩 검증을 여기서 유도한다. 양방향 reconcile 도구(untracked dir 채택, prune 자동화)는 이 모델 위에서 동작하는 후속 change로 분리한다.

### D9. `pm mv`는 경로(파일) + 워크스페이스 설정을 함께 변경

경로가 `base_root/<workspace>/<dir>`로 결정적이므로, 워크스페이스만 바꾸고 파일을 두면 매니페스트가 가리키는 경로와 실제 위치가 어긋난다 (D8 위반). 따라서 `pm mv`는:
- 매니페스트의 `project.workspace`(및 필요 시 `dir`)를 갱신하고,
- 동시에 디렉토리를 옛 경로에서 새 경로(`base_root/<새workspace>/<dir>`)로 **물리적으로 이동**한다.
- 대상 경로가 이미 존재하면 중단. 이동 실패 시 매니페스트를 갱신하지 않는다 (D3의 add-move와 동일한 비파괴 원칙).

기존 구현(`workspace.rs:167` `move_projects`)은 매니페스트 필드만 바꾸므로 이 change에서 물리 이동을 추가하도록 고친다.

## Risks / Trade-offs

- [기존 사용자의 외부 경로 워크스페이스가 깨진다] → 클린 리셋. 하위 호환을 명시적으로 포기하고 마이그레이션 도구도 제공하지 않는다.
- [중첩 `dir`로 분류하던 케이스(`web/frontend`)가 사라진다] → 단일 이름으로 평탄화하거나 워크스페이스 분할로 표현. 컨벤션 단순화의 이득이 손실보다 크다고 판단.
- [`pm add`가 move를 수반하면 실수로 작업 트리를 옮길 수 있다] → 기본은 dry-run 안내, 실제 이동 전 사용자 확인 요구. 비파괴를 우선.
- [Windows 경로 분리자 검증 누락] → `\` 도 거부. 크로스플랫폼 통일 규칙.

## Migration Plan

클린 리셋이다. 마이그레이션 경로를 제공하지 않는다.

- 본 change는 마이그레이션 없이 호환을 깬다는 점을 README와 CHANGELOG에 명시.
- v3 이하 매니페스트 감지 시: 명확한 호환 불가 에러 + "새 컨벤션으로 워크스페이스를 다시 만들라"는 안내.
- 자동 변환 도구는 만들지 않는다 (기존 사용자가 사실상 없는 v0.x 단계).

## Open Questions

- `base_root` 변경 시 기존 워크스페이스 디렉토리를 자동 이동할지, 아니면 매니페스트만 갱신하고 사용자에게 수동 이동을 요구할지. → 본 change 범위 밖, 후속에서 결정.
- `pm add`/`pm mv`가 move를 수행할 때 git 작업 트리 안전성 검증(uncommitted changes 등) 수준. 보수적 기본값으로 가되 옵션 노출 여부.
- 양방향 reconcile(`pm check`의 untracked dir 탐지, `pm sync`의 adopt/prune)은 D8 모델 위에서 동작하는 별도 후속 change로 분리. prune은 데이터 손실 위험이라 항상 확인을 받는다(D8).

## Context

v0.4.0 의 `pm project init` / `sync` 는 `configs/<lang>/common/.gitignore` 의 4–10줄 짜리 정적 파일을 그대로 적용한다. 이 컨텐츠는 pm 메인테이너가 수동으로 작성/유지해야 하고, 실세계의 풍부한 ignore 룰 (IDE 잡파일, OS 메타, 빌드/캐시 변형) 은 거의 다 빠져 있다.

`github/gitignore` 는 GitHub 가 직접 유지하는 200+ 카테고리의 표준 템플릿 저장소다. CC0 라이선스라 임베딩에 제약이 없다. pm 이 빌드 타임에 이를 vendoring + 임베딩하면, 사용자는 별도 작업 없이 production-quality `.gitignore` 를 얻을 수 있다.

기술적 제약:
- **단일 정적 바이너리** 정체성 유지 — 런타임에 외부 저장소나 네트워크 의존 금지
- **결정적 빌드** — 같은 입력 → 같은 바이너리 (CI 재현성)
- **임베딩 결과 크기 제한** — 10개 템플릿, 각 1–3 KB → 총 ~30 KB 가산은 수용 가능
- **사용자 마커 블록** 의 안정성 — pm 영역과 사용자 영역이 안전하게 공존해야 함 (이전 `dotenv-sync` 검토에서 도출된 패턴)

## Goals / Non-Goals

**Goals:**
- `pm project init` 한 번으로 일반적 `.gitignore` 룰의 90% 를 자동 적용
- pm 메인테이너가 `git submodule update --remote` 한 줄로 모든 템플릿 갱신
- 사용자가 `.gitignore` 를 자유롭게 수동 편집해도 `pm project sync` 가 망가뜨리지 않음
- 사용자가 카테고리 조합을 직접 선택할 수 있는 escape hatch 제공 (`--categories rust,vscode,macos`)
- `merged` strategy 가 보존되고, 그 위에 마커 블록을 도입해 의미를 명확화

**Non-Goals:**
- 온라인 모드 (`gitignore.io` 호출) — 차후 별도 change `online-gitignore-source`
- `.editorconfig` 분리 — 단일 파일이 표현력 충분
- `github/gitignore` 자동 갱신 / dependabot 통합 — 메인테이너 수동 갱신 정책
- 사용자 정의 템플릿 추가 (예: `~/.config/pm/gitignore-categories/myteam.gitignore`) — 후속 change 검토
- pm 마커 블록 기반 `.editorconfig` / `.npmrc` 등 다른 파일 — 본 change 는 `.gitignore` 만

## Decisions

### D1. Vendoring: git submodule
`vendor/github-gitignore/` 에 `github/gitignore` 저장소를 submodule 로 둔다. pinned commit 으로 결정성 확보; 메인테이너가 `git submodule update --remote && git commit -m "Refresh gitignore templates"` 로 주기적 갱신.

**Why**:
- 빌드 시 네트워크 불필요 — submodule 은 git clone 시점에 fetch 됨, CI 의 `actions/checkout@v4` 에 `submodules: true` 옵션만 추가
- 라이선스 명시성 — 디스크에 실제 저장소가 보존되므로 CC0 라이선스 텍스트도 함께
- 결정성 — pinned commit 이 lockfile 역할

**Alternatives 거부**:
- `build.rs` 의 HTTP fetch — 네트워크 의존, 빌드 재현성 저하, CI 캐시 복잡도
- `scripts/refresh-templates.sh` 로 raw 파일을 repo 에 직접 커밋 — 매뉴얼 작업이 손쉽게 stale 됨, 라이선스 source attribution 약화
- crates.io 의 `gitignores` crate (예: `gitignore-template`) — 안정성·라이선스 검증 필요, 외부 의존성 증가

### D2. 임베딩 메커니즘: `include_str!` + `build.rs` 생성 모듈
`build.rs` 가 `vendor/github-gitignore/` 의 선택된 파일들을 `OUT_DIR` 의 단일 Rust 모듈로 합성:

```rust
// $OUT_DIR/embedded_gitignore.rs
pub static RUST: &str = include_str!("../../../vendor/github-gitignore/Rust.gitignore");
pub static NODE: &str = include_str!("../../../vendor/github-gitignore/Node.gitignore");
pub static MACOS: &str = include_str!("../../../vendor/github-gitignore/Global/macOS.gitignore");
// ...
pub static ALL_CATEGORIES: &[(&str, &str)] = &[
    ("rust", RUST),
    ("node", NODE),
    ("macos", MACOS),
    // ...
];
```

`src/templates/embedded.rs` 가 `include!(concat!(env!("OUT_DIR"), "/embedded_gitignore.rs"));` 한 줄로 가져옴.

**Why**: `include_str!` 은 std 빌트인, 추가 의존성 0. 컴파일 타임에 바이너리에 통째로 embed.
**Alternatives 거부**: `rust-embed` — 다수 파일 자동 임베딩에 유용하지만 10개 파일에는 overkill, 의존성 +1.

### D3. 합성 순서 (composition order)
`pm` 이 만드는 `.gitignore` 의 pm 마커 블록 안 라인 순서:

```
# === OS metadata ===
{macOS.gitignore}
{Linux.gitignore}
{Windows.gitignore}     # 팀 협업 가능성, 라인 수 적음

# === IDE / Editor ===
{VisualStudioCode.gitignore}
{JetBrains.gitignore}

# === Language: rust ===
{Rust.gitignore}

# === Framework: axum ===
{configs/rust/axum/.gitignore.extra}    # 짧음, pm 직접 관리
```

각 섹션은 `#` 주석 헤더로 구분. 카테고리 선택 (`--categories`) 시 헤더 + 내용을 통째로 포함/제외.

**Why**: 일반→특수 순서. 헤더가 있어 사용자가 봤을 때 출처를 알 수 있음. `merged` 가 라인 단위라 순서 보존이 약하지만, **마커 블록 안에서는 pm 이 통째로 교체** 하므로 순서가 정확히 유지됨.

### D4. pm 마커 블록 (managed region)
`.gitignore` 안에서 pm 영역과 사용자 영역을 텍스트 마커로 분리:

```
# 사용자 영역 (pm 건드리지 않음)
my-secret-stuff
*.local

# >>> pm managed (do not edit; run `pm project gitignore` to refresh) >>>
# === OS metadata ===
.DS_Store
...
# <<< pm managed <<<

# 사용자 영역 (pm 건드리지 않음)
extra_user_lines
```

**합성 알고리즘**:
1. 디스크의 `.gitignore` 읽기 (없으면 빈 string).
2. 마커 블록을 정규식으로 찾아 추출 (없으면 빈 블록).
3. 마커 블록 외부 = 사용자 영역, 그대로 보존.
4. 새 pm 영역 = 합성 결과로 대체.
5. 마커 블록이 처음이면 파일 끝에 append, 있었으면 in-place 교체.

**Why**:
- 사용자가 자유롭게 외부 영역 편집 가능
- pm 이 영역 안 내용은 통째로 갱신 — 라인 단위 merge 의 모호성 (사용자가 pm 라인을 수정한 건지 새 사용자 라인을 추가한 건지) 제거
- 마커 텍스트는 영구 (마이그레이션 단순)

**Trade-off**: `merged` strategy 의 기존 의미와 약간 다름. v0.4.0 까지의 `.gitignore` 는 `merged` (라인 단위 union) 였고, 이번에는 `pm-managed-block` (블록 단위 교체) 로 바뀜. 첫 sync 시 한 번 텍스트 정리가 필요 — 이전 pm 이 추가한 라인이 사용자 영역에 흩어져 있음. 처리:
- 첫 sync 시 알려진 v0.4.0 이전 pm 라인 패턴 (`/target`, `node_modules/`, `*.tsbuildinfo` 등) 을 자동 검출해 마커 블록으로 옮김
- 검출 못 한 라인은 사용자 영역에 남음 (안전한 fallback)

### D5. CLI 표면
신규 서브커맨드:

```bash
pm project gitignore [--diff] [--categories <list>]
```

- `--categories` 미지정 시 default = `os + ide + language + framework`
- `--diff` 는 디스크 파일과 합성 결과 비교만 출력 (no write)
- 예: `pm project gitignore --categories rust,macos,vscode` 만으로 합성

`pm project init` 도 동일 합성 로직 사용. `pm project sync` 는 기존처럼 호출되되 `.gitignore` 만은 새 마커 블록 모델로 처리.

**Why**: 별도 명령으로 사용자가 명시적으로 호출 가능. init/sync 자동 호출은 자연스러운 default.

### D6. Framework-extra 의 위치
짧은 framework 특화 라인은 `configs/<lang>/<fw>/.gitignore.extra` 에 둔다:

```
configs/ts/nextjs/.gitignore.extra:
.next/
out/
.vercel
```

`configs/<lang>/<fw>/manifest.yaml` 에는 굳이 추가하지 않음 — `.gitignore.extra` 는 합성 로직이 직접 검출.

**Why**: 일반적 `.gitignore` strategy 와 별개로, pm 의 합성 로직만 인식하는 보조 파일이라 manifest 의 strategy 모델에 맞지 않음.

### D7. 임베딩할 카테고리 — 초기 10개

| 카테고리 | github/gitignore 경로 | pm 사용처 |
|---|---|---|
| `macos` | `Global/macOS.gitignore` | OS, default 포함 |
| `linux` | `Global/Linux.gitignore` | OS, default 포함 |
| `windows` | `Global/Windows.gitignore` | OS, default 포함 (팀 협업) |
| `vscode` | `Global/VisualStudioCode.gitignore` | IDE, default 포함 |
| `jetbrains` | `Global/JetBrains.gitignore` | IDE, default 포함 |
| `rust` | `Rust.gitignore` | language=rust 일 때 |
| `node` | `Node.gitignore` | language=ts 일 때 |
| `python` | `Python.gitignore` | language=python 일 때 |
| `dart` | `Dart.gitignore` | language=dart 일 때 |
| `go` | `Go.gitignore` | language=go 추가 시 (pm 은 아직 미지원이지만 임베딩만 미리) |

**Why**: pm 이 현재 지원하는 4 언어 (rust/ts/python/dart) 의 main + 3 OS + 2 IDE = 9 + 향후 확장 1 (go). 카테고리는 case-insensitive 키 (`rust`, `Rust` 모두 매칭).

### D8. 라이선스 처리
- `vendor/github-gitignore/LICENSE` 가 자동으로 함께 vendor 됨 (CC0)
- pm 의 `Cargo.toml` license 는 그대로 MIT (pm 자체 라이선스)
- README "Acknowledgements" 섹션에 `github/gitignore` 출처 + CC0 표기
- 임베딩된 텍스트 자체에 출처 주석 자동 추가:
  ```
  # === Language: rust ===
  # Source: https://github.com/github/gitignore (CC0)
  {Rust.gitignore content}
  ```

## Risks / Trade-offs

- **Risk**: `github/gitignore` 갱신 빈도가 메인테이너 손에 달림 → 시간이 지나면서 stale.
  → **Mitigation**: `pm project gitignore --refresh-source` 같은 명령은 안 만들지만 (스코프 제한), README 에 "갱신 정책: 메인테이너가 release 마다 submodule update" 명시. 차후 `online-gitignore-source` change 가 동적 fetch 옵션 추가.
- **Risk**: 마커 블록 도입 시 v0.4.0 사용자의 `.gitignore` 가 처음 sync 에서 어색하게 바뀜 (이전 pm 추가 라인이 사용자 영역에 흩어진 채로 남음).
  → **Mitigation**: D4 의 자동 검출 패턴 + 사용자에게 stderr 안내 ("pm has migrated <N> lines to its managed block; review your .gitignore").
- **Risk**: Submodule fetch 실패 시 (`git submodule update --init` 안 하고 `cargo build`) 빌드 에러가 모호함.
  → **Mitigation**: `build.rs` 가 vendor 디렉토리 부재 시 친절한 에러 출력 ("vendor/github-gitignore/ is empty. Run `git submodule update --init --recursive`.").
- **Trade-off**: 바이너리 크기 +30~50 KB. 수용 가능 — 현재 pm 바이너리는 ~5 MB 이라 비율 < 1%.
- **Trade-off**: pm 마커 블록은 텍스트 협의 — 다른 도구 (Renovate, Dependabot, IDE auto-commit) 가 마커를 깨뜨리면 pm 이 동작 못 함. 정규식 detection 의 한계.
- **Trade-off**: `--categories` 가 사용자 자유도 제공하지만 carefully crafted 카테고리 조합이 아니면 충돌 라인 (e.g. `node_modules/` 가 두 카테고리에서 모두 매칭) 가능. 합성 시 dedup 필요.

## Migration Plan

1. **신규 사용자**: `pm project init -l ts -f nextjs` → 풍부한 `.gitignore` (마커 블록 포함, 합성된 카테고리들).
2. **v0.4.0 사용자 — `.gitignore` 가 이미 존재**:
   - 첫 `pm project sync` (또는 `pm project gitignore`) 시점에:
     - 기존 파일을 `pm-managed` 마커 블록 패턴으로 검색
     - 없으면: 알려진 pm-라인 패턴 (위 D4) 을 사용자 영역에서 자동 검출 → 마커 블록으로 이주
     - 사용자에게 stderr 안내
3. **v0.4.0 이전 (.gitignore 미적용 또는 수동 작성)**: `pm project sync` 가 첫 적용 시 사용자 영역 보존 + 새 마커 블록 추가.
4. **Rollback**: v0.5.0 이전 바이너리로 다운그레이드 시, `.gitignore` 의 마커 블록은 그냥 사용자 텍스트로 보임 — 동작 영향 없음, 단지 다음 갱신은 안 됨.

## Resolved Decisions

1. **마커 블록 텍스트 고정**: 정확히 `# >>> pm managed (do not edit; run \`pm project gitignore\` to refresh) >>>` 와 `# <<< pm managed <<<` 사용. 변경은 차후 BREAKING change 에서만 허용.
2. **`--categories` 는 override**: 명시 시 default 무시, 정확히 지정된 카테고리만 합성. Union 패턴 (`--categories +foo`) 은 차후 별도 검토.
3. **Windows OS 카테고리 default 포함**: 팀 협업 안전 + 라인 수 적음 (`Global/Windows.gitignore` 약 15줄).
4. **`go` 카테고리 임베딩 yes, default 미포함**: 임베딩 비용 < 2KB. pm 이 Go 를 정식 지원할 때 default 추가.

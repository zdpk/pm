## 1. Vendoring 인프라

- [x] 1.1 `git submodule add https://github.com/github/gitignore.git vendor/github-gitignore` (pinned commit)
- [x] 1.2 `.gitmodules` 검토 — default `update = checkout` 충분 (변경 없음)
- [x] 1.3 `LICENSES/github-gitignore-CC0.txt` 신규 — CC0 1.0 Universal 텍스트 + github/gitignore 출처 헤더
- [x] 1.4 `.github/workflows/release.yml` 의 `actions/checkout@v4` 에 `submodules: recursive` 추가
- [x] 1.5 `README.md` Acknowledgements 섹션 추가 (github/gitignore + CC0 명시)

## 2. build.rs (빌드 타임 임베딩)

- [x] 2.1 신규 `build.rs` — 프로젝트 root
- [x] 2.2 `Cargo.toml` 에 `build = "build.rs"` 명시
- [x] 2.3 vendor 부재 감지: `vendor/github-gitignore/Rust.gitignore` 없으면 `panic!` with 친절 메시지
- [x] 2.4 10개 카테고리 파일 → `OUT_DIR/embedded_gitignore.rs` (`pub static MACOS/LINUX/...`, `ALL_CATEGORIES` 집계)
- [x] 2.5 `cargo:rerun-if-changed=vendor/github-gitignore` + `build.rs` 출력
- [x] 2.6 50KiB 사이즈 가드 — 총 길이 합산, 초과 시 `panic!` (현재 12,704 bytes / 51,200 limit)
- [x] 2.7 sanity check: 생성 파일 `target/debug/build/.../embedded_gitignore.rs` 확인

## 3. `src/templates/` 모듈

- [x] 3.1 신규 `src/templates/mod.rs` — public API (`Category` enum, `lookup_category`, `default_categories`)
- [x] 3.2 `src/templates/embedded.rs` — `include!(concat!(env!("OUT_DIR"), "/embedded_gitignore.rs"));`
- [x] 3.3 `Category` enum + `key()` / `header()` / `content()` 매핑
- [x] 3.4 `lookup_category(name: &str) -> Option<Category>` (case-insensitive, trim)
- [x] 3.5 `default_categories(language, framework)` — D3 순서 (OS → IDE → Language)
- [x] 3.6 `src/main.rs` 에 `mod templates;` 추가
- [x] 3.7 단위 테스트 7개: case-insensitive lookup, unknown category, every category non-empty, defaults for rust/ts/unknown, Windows in defaults
- [x] 3.8 stub `synthesize.rs` / `marker.rs` 모듈 (Stage 2 채움)

## 4. 합성 (synthesize) 로직

- [x] 4.1 `synthesize_managed_body(categories, framework_name, framework_extra) -> String`
- [x] 4.2 각 카테고리: `# === <Category::header()> ===` + `# Source: github/gitignore (CC0)` attribution
- [x] 4.3 Framework extras: `# === Framework: <name> ===` + `# Source: pm bundled framework extras` attribution. 비어 있으면 skip
- [x] 4.4 line-level dedup — 빈 줄과 `#` 주석은 보존, 패턴 라인은 첫 등장만 유지 (`HashSet<String>`)
- [x] 4.5 단위 테스트 7개: 헤더+attribution, 같은 카테고리 두 번 → dedup, comments preserved across sections, framework extra append, empty extra skipped, framework dedup against language category, empty input

## 5. 마커 블록 (managed region) 로직

- [x] 5.1 `BEGIN_MARKER` / `END_MARKER` 상수 (정확한 텍스트 고정 + regression-guard test)
- [x] 5.2 `parse(content) -> ParsedGitignore { before, managed, after }` — 마커 영역 분리, malformed (BEGIN 만 있음) 은 user content 로 처리
- [x] 5.3 `merge_into_existing(existing, managed_body) -> String` + `render(parsed, body)` — 마커 있으면 in-place, 없으면 끝에 빈 줄 + 새 블록 append
- [x] 5.4 단위 테스트 9개: empty file, user-only, full block, BEGIN-only malformed, append, in-place replace, byte-exact preservation, empty input → block created, marker text fixed-constant guard

## 6. v0.4.x → v0.5.0 마이그레이션 검출

- [x] 6.1 `LEGACY_PATTERNS: &[&str]` — rust(`/target`, `**/*.rs.bk`, `*.pdb`), ts(`node_modules/`, `dist/`, `.env`, `.env.local`, `*.tsbuildinfo`), python(`__pycache__/`, `*.py[cod]`, `*.egg-info/`, `.venv/`, `.ruff_cache/`, `.pytest_cache/`, `.mypy_cache/`)
- [x] 6.2 `strip_legacy_patterns(user_region) -> (cleaned, removed_count)` — 정확 매치만 (lookalike 보존), comments/blanks 보존, trailing newline 정책 보존
- [x] 6.3 `emit_migration_notice(removed)` — stderr 한 줄 안내 (`pm: migrated <N> legacy line(s) into the pm-managed .gitignore block`)
- [x] 6.4 단위 테스트 6개: rust 라인 검출, user 라인 보존, comments+blanks 보존, lookalikes 미스트립, all-legacy → 빈 결과, no-trailing-newline 입출력 일관

## 7. CLI: `pm project gitignore`

- [ ] 7.1 `src/cli.rs` 의 `ProjectCommand::Gitignore { diff, categories }` 변형 추가
- [ ] 7.2 `src/commands/project.rs::cmd_gitignore` 핸들러
- [ ] 7.3 `--diff` 모드 — `similar` crate (이미 의존성 있음) 의 `TextDiff` 로 unified diff 출력, 디스크 미수정
- [ ] 7.4 `--categories <comma>` 파싱 — case-insensitive, unknown 시 친절한 에러 (`valid categories: macos, linux, windows, vscode, jetbrains, rust, node, python, dart, go`)
- [ ] 7.5 default category resolution — `default_categories(lang, fw)` 사용
- [ ] 7.6 ProjectCommand 의 dispatch 추가
- [ ] 7.7 통합 테스트: `--diff` 가 디스크 수정 없는지, override 동작

## 8. `pm project init` / `sync` 통합

- [ ] 8.1 `cmd_init` 의 .gitignore 적용 경로 — synthesize + merge_into_existing 사용 (기존 `apply_merged` 우회)
- [ ] 8.2 `cmd_sync` / `sync_project` — managed 블록 재합성, 사용자 영역 byte-exact 보존
- [ ] 8.3 `is_file_outdated` — `.gitignore` 는 마커 블록 비교로 판정 (사용자 영역 다르면 outdated 아님)
- [ ] 8.4 단위/통합 테스트: init 후 마커 존재, sync 후 managed 영역 갱신 + user 영역 보존

## 9. Framework `.gitignore.extra` 추가

- [ ] 9.1 `configs/ts/nextjs/.gitignore.extra` 신규 — `.next/`, `out/`, `.vercel`
- [ ] 9.2 `configs/ts/nestjs/.gitignore.extra` 신규 — `dist/` (이미 있긴 하나 framework 명시), `*.tsbuildinfo`
- [ ] 9.3 `configs/rust/axum/.gitignore.extra` — 비워두거나 (`# axum has no special ignores`) 생략
- [ ] 9.4 `configs/rust/clap/.gitignore.extra` — 동일
- [ ] 9.5 `configs/python/fastapi/.gitignore.extra` — 비움 또는 `.pytest_cache/` 정도
- [ ] 9.6 `configs/dart/flutter/.gitignore.extra` — flutter build artifacts (`build/`, `.dart_tool/`, `.flutter-plugins`)

## 10. 기존 `configs/<lang>/common/.gitignore` 정리

- [ ] 10.1 `configs/rust/common/.gitignore` 의 라인을 historical migration 패턴으로 이동, 파일 자체는 placeholder (single comment) 또는 manifest 에서 제외
- [ ] 10.2 `configs/ts/common/.gitignore` 동일
- [ ] 10.3 `configs/python/common/.gitignore` 동일
- [ ] 10.4 `configs/dart/common/.gitignore` 동일
- [ ] 10.5 각 `configs/<lang>/common/manifest.yaml` 에서 `.gitignore` 항목 제거 (이제 합성 로직이 .gitignore 를 직접 만든다)

## 11. 통합 검증

- [ ] 11.1 `cargo build` — submodule 부재 시 친절한 에러
- [ ] 11.2 `cargo build` — submodule 정상 시 빌드 성공, 바이너리 +30~50KB
- [ ] 11.3 `cargo clippy --all-targets` — 신규 warning 0
- [ ] 11.4 `cargo test` — 신규 단위 테스트 모두 pass (5~10개 추가 예상)
- [ ] 11.5 수동 E2E: 빈 디렉토리에서 `pm project init -l ts -f nextjs -y` → 풍부한 `.gitignore` (마커 블록 + OS + IDE + Node + nextjs extras)
- [ ] 11.6 수동 E2E: 사용자 라인 추가 → `pm project gitignore` → 사용자 영역 그대로, managed 영역만 갱신
- [ ] 11.7 수동 E2E: `--categories rust,macos` 지정 → 다른 카테고리 빠짐
- [ ] 11.8 수동 E2E: `--diff` 가 디스크 미수정 + 차이 출력
- [ ] 11.9 수동 E2E: v0.4.x 형식 `.gitignore` (legacy 라인 + 마커 없음) → `pm project gitignore` → 마이그레이션 안내 + 라인 이동

## 12. 문서화

- [ ] 12.1 README "Local Dev Orchestrator" 또는 "pm project" 섹션에 `pm project gitignore` 추가
- [ ] 12.2 마커 블록 모델 설명 (사용자 영역 vs pm 영역)
- [ ] 12.3 `--categories` 카테고리 목록 표
- [ ] 12.4 v0.4.x → v0.5.0 마이그레이션 노트 (관여 작 — 자동 detection)
- [ ] 12.5 README Acknowledgements: github/gitignore CC0 명시

## 13. 버전 bump 및 릴리스

- [ ] 13.1 Cargo.toml version 0.4.0 → 0.5.0
- [ ] 13.2 git commit + tag v0.5.0
- [ ] 13.3 GitHub Actions release workflow 정상 동작 확인 (submodule 포함, 3 타겟 빌드)
- [ ] 13.4 GitHub Release notes — 신규 기능, 마이그레이션 자동화 명시 (BREAKING 없음)

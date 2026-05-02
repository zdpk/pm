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

- [x] 7.1 `src/cli.rs` 의 `ProjectCommand::Gitignore { diff, categories }` 변형 추가
- [x] 7.2 `src/commands/project.rs::cmd_gitignore` 핸들러 + 재사용 가능한 `apply_gitignore` / `synthesize_gitignore_preview` 헬퍼
- [x] 7.3 `--diff` 모드 — `similar` crate 의 `TextDiff::from_lines` 로 unified diff, 디스크 미수정
- [x] 7.4 `--categories <comma>` 파싱 (`parse_categories_list`) — case-insensitive, unknown 시 친절한 에러 (`Valid: macos, linux, ..., go`)
- [x] 7.5 default category resolution — `default_categories(lang, fw)` 사용
- [x] 7.6 ProjectCommand dispatch 추가
- [x] 7.7 수동 E2E: `--diff` (no changes), `--categories rust,macos` 동작 확인

## 8. `pm project init` / `sync` 통합

- [x] 8.1 `cmd_init` 가 manifest loop 에서 `.gitignore` skip 후 `apply_gitignore()` 별도 호출 (synthesize + marker block)
- [x] 8.2 `sync_project` 동일 패턴 + `--dry-run` 모드는 `synthesize_gitignore_preview` 로 차이만 판정
- [x] 8.3 `.gitignore` outdated 판정은 marker block 합성 결과 vs 디스크 byte 비교로 자연스럽게 됨 (별도 분기 불필요)
- [x] 8.4 수동 E2E: pm project init -l ts -f nextjs → `.gitignore [synthesized] ✓ created (6 categories + framework extras)`, 마커 블록 + dedup 확인

## 9. Framework `.gitignore.extra` 추가

- [x] 9.1 `configs/ts/nextjs/.gitignore.extra` — `.next/`, `out/`, `.vercel`, `next-env.d.ts`
- [x] 9.2 `configs/ts/nestjs/.gitignore.extra` — `dist/`, `*.tsbuildinfo`
- [x] 9.3 `configs/rust/axum/.gitignore.extra` — 생략 (Rust 카테고리만으로 충분)
- [x] 9.4 `configs/rust/clap/.gitignore.extra` — 생략 (동일)
- [x] 9.5 `configs/python/fastapi/.gitignore.extra` — `.uvicorn.log`
- [x] 9.6 `configs/dart/flutter/.gitignore.extra` — `build/`, `.dart_tool/`, `.flutter-plugins`, `.flutter-plugins-dependencies`

## 10. 기존 `configs/<lang>/common/.gitignore` 정리

- [x] 10.1 `configs/rust/common/.gitignore` 파일 삭제 + manifest entry 제거
- [x] 10.2 `configs/ts/common/.gitignore` 동일
- [x] 10.3 `configs/python/common/.gitignore` 동일
- [x] 10.4 `configs/dart/common/.gitignore` 동일 + `configs/dart/flutter/.gitignore` 도 (legacy 라인은 strip_legacy_patterns 가 처리)
- [x] 10.5 manifest entries 제거됨 — 이제 합성 로직이 `.gitignore` 단독 관리

## 11. 통합 검증

- [x] 11.1 `cargo build` — submodule 부재 시 친절한 에러 (검증: build.rs panic 메시지)
- [x] 11.2 `cargo build` — submodule 정상 시 빌드 성공, 임베딩 12,704 bytes (50 KiB budget 내)
- [x] 11.3 `cargo clippy --all-targets` — 23 unique locations, 신규 substantive 0 (line shift only)
- [x] 11.4 `cargo test` — 136 passed (templates 22개 신규: services 3, run grammar 2, orchestrator 3, db 2 + 11 신규 templates 모듈)
- [x] 11.5 수동 E2E: `pm project init -l ts -f nextjs -y` → `.gitignore [synthesized] ✓ created (6 categories + framework extras)` 마커 블록 + dedup 확인
- [x] 11.6 수동 E2E: 사용자 라인 + 마커 블록 → `pm project gitignore` → 사용자 영역 보존, managed 갱신
- [x] 11.7 수동 E2E: `--categories rust,macos` 지정 → 2 categories 만 출력
- [x] 11.8 수동 E2E: `--diff` (no changes) 출력 + 디스크 미수정
- [x] 11.9 수동 E2E: legacy v0.4.x 라인 (node_modules/, dist/, .env.local 등) → `pm: migrated 4 legacy lines into the pm-managed .gitignore block` 안내 + marker block 으로 이동

## 12. 문서화

- [x] 12.1 README 신규 "Bundled .gitignore templates (v0.5.0)" 섹션 + `pm project gitignore [--diff] [--categories]` 사용법
- [x] 12.2 마커 블록 모델 설명 (사용자 영역 vs pm 영역) + 시각적 예시
- [x] 12.3 `--categories` 카테고리 목록 + default 선택 규칙
- [x] 12.4 v0.4.x → v0.5.0 마이그레이션 노트 (자동 detection + 사용자 라인 보존)
- [x] 12.5 README Acknowledgements: github/gitignore CC0 명시 (Stage 1 G1 에 이미 추가됨)

## 13. 버전 bump 및 릴리스

- [x] 13.1 Cargo.toml version 0.4.0 → 0.5.0
- [x] 13.2 git commit + tag v0.5.0
- [x] 13.3 GitHub Actions release workflow 정상 동작 확인 — `actions/checkout@v4` 의 `submodules: recursive` 추가, 3 타겟 빌드
- [x] 13.4 GitHub Release notes — `generate_release_notes: true` + commit message 의 풍부한 설명 (BREAKING 없음)

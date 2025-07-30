# NPM 배포 자동화 계획

> **Generated**: 2025-07-29  
> **Status**: Ready for Implementation  
> **Priority**: High  

## 📋 개요

현재 GitHub Release는 성공하지만 NPM 배포가 실패하는 문제를 해결하여 완전 자동화된 배포 시스템을 구축합니다.

## 🚨 현재 문제점

### 문제 1: NPM 빌드 순서 오류
- **증상**: `postinstall` 스크립트가 `dist/install.js` 파일을 찾을 수 없음
- **원인**: TypeScript 빌드 전에 `npm ci`가 실행됨
- **영향**: NPM 배포 100% 실패

### 문제 2: 패키지 구조 문제
- **증상**: CI 환경에서 스크립트 실행 시점 불일치
- **원인**: `postinstall` vs `prepublishOnly` 혼재
- **영향**: 배포 환경별 동작 차이

## 🎯 해결 방안

### Phase 1: 즉시 수정 (Critical)

#### 1.1 NPM 스크립트 수정
```json
{
  "scripts": {
    "build": "tsc",
    "prepare": "npm run build",
    "postpack": "echo 'Package ready for installation'"
  }
}
```
- `postinstall` → `prepare`로 변경
- CI 환경에서 안전한 스크립트 실행

#### 1.2 워크플로우 빌드 순서 수정
```yaml
- name: Install dependencies (skip scripts)
  run: |
    cd npm
    npm install --ignore-scripts

- name: Build TypeScript
  run: |
    cd npm
    npm run build

- name: Publish to NPM
  run: |
    cd npm
    npm publish
```

#### 1.3 기본 배포 테스트
- v0.1.3 태그로 전체 플로우 검증
- GitHub Release + NPM 동시 성공 확인

### Phase 2: 단기 개선 (High Priority)

#### 2.1 에러 핸들링 강화
- 각 단계별 실패 시 명확한 오류 메시지
- 부분 실패 시 롤백 메커니즘

#### 2.2 빌드 최적화
- Node.js 의존성 캐싱
- TypeScript 컴파일 최적화

#### 2.3 배포 검증
- 패키지 크기 제한 (< 50MB)
- 필수 파일 존재 확인
- 버전 일치성 검증

### Phase 3: 장기 개선 (Medium Priority)

#### 3.1 멀티플랫폼 지원
- Linux (x86_64, aarch64) 빌드 매트릭스
- Windows 지원 검토

#### 3.2 자동 릴리스 노트
- 커밋 메시지 기반 생성
- 브레이킹 체인지 하이라이트

#### 3.3 배포 모니터링
- Slack/Discord 알림 통합
- 배포 성공률 대시보드

## 🔧 구현 단계

### Step 1: NPM 패키지 수정
**파일**: `npm/package.json`
**변경사항**:
- `postinstall` 스크립트 제거 또는 수정
- `prepare` 스크립트 추가
- 빌드 순서 최적화

### Step 2: GitHub Actions 수정
**파일**: `.github/workflows/release.yml`
**변경사항**:
- `publish-npm` job의 단계 순서 수정
- 에러 핸들링 추가
- 빌드 캐싱 구현

### Step 3: 테스트 및 검증
**방법**:
- 새 버전 태그 생성 (v0.1.3)
- 전체 배포 플로우 실행
- 성공 확인 및 문제점 수정

## ✅ 성공 지표

- [ ] 태그 푸시 → GitHub Release 자동 생성 (현재 ✅)
- [ ] NPM 패키지 자동 배포 (현재 ❌)
- [ ] 버전 동기화 완전 자동화 (현재 ✅)
- [ ] 배포 실패율 < 5%
- [ ] 전체 배포 시간 < 10분

## 📝 작업 체크리스트

### 즉시 수행 (Critical)
- [ ] `npm/package.json` 스크립트 수정
- [ ] `.github/workflows/release.yml` 빌드 순서 수정
- [ ] v0.1.3 태그로 테스트 배포

### 단기 개선 (1-2주)
- [ ] 에러 핸들링 및 로깅 개선
- [ ] 빌드 성능 최적화
- [ ] 배포 전 검증 단계 추가

### 장기 개선 (1개월+)
- [ ] 멀티플랫폼 빌드 지원
- [ ] 자동 릴리스 노트 생성
- [ ] 모니터링 및 알림 시스템

## 🚀 Agent 실행 지침

### 다음 Agent가 수행해야 할 작업:

1. **우선순위 1**: NPM 스크립트 수정
   - `npm/package.json`의 `postinstall` 문제 해결
   - 안전한 빌드 순서 구현

2. **우선순위 2**: 워크플로우 수정
   - `.github/workflows/release.yml`의 `publish-npm` job 개선
   - TypeScript 빌드를 `npm ci` 전에 실행

3. **우선순위 3**: 테스트 실행
   - v0.1.3 태그 생성 및 배포 테스트
   - 성공/실패 결과 보고

### 참고 파일:
- `npm/package.json` - NPM 패키지 설정
- `.github/workflows/release.yml` - 배포 워크플로우
- `Cargo.toml` - Rust 프로젝트 버전 (동기화 소스)

### 예상 작업 시간: 2-3시간

---

**🤖 Generated with Claude Code**  
**Co-Authored-By: Claude <noreply@anthropic.com>**
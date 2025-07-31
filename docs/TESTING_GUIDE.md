# Docker 기반 'pm' 테스트 가이드 (Docker-based 'pm' Testing Guide)

## 1. 개요 (Overview)

이 문서는 `pm` 프로젝트의 기능 안정성을 보장하기 위해 Docker를 활용한 테스트 환경의 구성과 사용법을 안내합니다. 이 테스트 환경은 다음과 같은 장점을 가집니다.

- **격리성 (Isolation):** 모든 테스트는 Docker 컨테이너 내부의 격리된 환경에서 실행되므로, 로컬 개발 환경에 영향을 주지 않고 일관된 결과를 보장합니다.
- **재현성 (Reproducibility):** `cargo vendor`를 통해 모든 빌드 의존성을 프로젝트 내부에 고정합니다. 이를 통해 언제 어디서든 동일한 조건으로 테스트를 재현할 수 있습니다.
- **네트워크 테스트 지원 (Network Testing Support):** 빌드 의존성은 `vendor`를 통해 격리되지만, 컨테이너 내에서 실행되는 테스트 자체는 외부 네트워크에 접근할 수 있습니다. 이를 통해 Git 리포지토리 클론과 같은 네트워크 기반 기능도 안정적으로 테스트할 수 있습니다.
- **속도 (Speed):** 초기 설정 이후에는 캐시된 Docker 이미지와 `vendor` 디렉터리를 활용하여 매우 빠르게 테스트를 실행할 수 있습니다.

## 2. 테스트 실행 방법 (How to Run Tests)

모든 테스트는 아래 스크립트 하나로 실행할 수 있습니다.

```bash
./script/test-docker.sh
```

이 스크립트는 다음 작업을 자동으로 수행합니다.
1. `cargo vendor`를 실행하여 의존성을 `vendor/` 디렉터리에 준비합니다.
2. `cargo test --no-run`을 통해 로컬에서 테스트 바이너리를 컴파일합니다.
3. `docker-compose`를 사용하여 테스트용 컨테이너를 실행하고, 컨테이너 내에서 모든 단위 테스트와 통합 테스트를 수행합니다.

## 3. 테스트 시나리오 (Test Scenarios)

아래는 `pm`의 핵심 기능을 검증하는 주요 테스트 시나리오 예시입니다.

### 시나리오 1: 프로젝트 초기화 및 상태 확인
- **목표:** 새로운 `pm` 작업 공간을 초기화하고, 기본 상태를 확인합니다.
- **테스트 명령어:**
  ```bash
  # 작업 디렉터리 생성 및 이동
  mkdir /tmp/test-project && cd /tmp/test-project

  # pm 초기화
  pm init

  # 상태 확인
  pm status
  ```
- **기대 결과:**
  - `.pm` 디렉터리가 생성됩니다.
  - `pm status` 실행 시 "No projects found."와 같은 초기 상태 메시지가 출력됩니다.

### 시나리오 2: 태그 및 프로젝트 관리
- **목표:** 태그를 생성하고, 해당 태그를 사용하여 프로젝트를 추가하고 조회합니다.
- **테스트 명령어:**
  ```bash
  # 'backend'와 'frontend' 태그 추가
  pm tag add backend frontend

  # 'service-a' 프로젝트를 'backend' 태그와 함께 추가
  pm project add /path/to/service-a --tags backend

  # 'backend' 태그가 달린 프로젝트 목록 조회
  pm project list --tags backend
  ```
- **기대 결과:**
  - `pm tag list` 실행 시 `backend`, `frontend` 태그가 표시됩니다.
  - `pm project list --tags backend` 실행 시 `service-a` 프로젝트가 표시됩니다.

### 시나리오 3: 네트워크를 사용하는 확장 기능 추가 (Git Clone)
- **목표:** Git 리포지토리에 호스팅된 원격 확장 기능을 추가하여 컨테이너의 네트워크 연결을 검증합니다.
- **테스트 명령어:**
  ```bash
  # 원격 확장 기능 추가 (예시 URL)
  pm extension add https://github.com/user/example-pm-extension.git

  # 설치된 확장 기능 목록 확인
  pm extension list
  ```
- **기대 결과:**
  - 네트워크 오류 없이 Git 리포지토리가 성공적으로 클론됩니다.
  - `pm extension list` 실행 시 `example-pm-extension`이 목록에 표시됩니다.

### 시나리오 4: 설정 변경 및 확인
- **목표:** `pm`의 내부 설정을 변경하고, 변경된 값이 올바르게 적용되었는지 확인합니다.
- **테스트 명령어:**
  ```bash
  # 백업 경로 설정 변경
  pm config set backup.path /new/backup/path

  # 변경된 설정 값 확인
  pm config get backup.path
  ```
- **기대 결과:**
  - `pm config get backup.path` 실행 시 `/new/backup/path`가 출력됩니다.

### 시나리오 5: 백업 및 복원
- **목표:** 현재 `pm` 상태를 백업하고, 특정 파일을 삭제한 후 백업으로부터 복원합니다.
- **테스트 명령어:**
  ```bash
  # 백업 생성
  pm backup create --reason "Initial state"

  # 설정 파일 임의 삭제 (시뮬레이션)
  rm .pm/config.toml

  # 가장 최근 백업으로 복원
  pm backup restore --latest
  ```
- **기대 결과:**
  - 백업 파일이 지정된 경로에 생성됩니다.
  - 복원 후 삭제되었던 `.pm/config.toml` 파일이 다시 생성됩니다.

## 4. 새로운 테스트 시나리오 추가하기 (Adding New Test Scenarios)

새로운 기능을 검증하기 위한 테스트 시나리오를 쉽게 추가할 수 있습니다.

1.  **스크립트 작성:** `tests/scenarios/` 디렉터리(신규 생성)에 `test_new_feature.sh`와 같은 셸 스크립트 파일을 작성합니다.
2.  **내용 구성:** 스크립트 내부에 `pm` 명령어를 사용하여 테스트할 기능의 절차를 작성하고, `grep`이나 `[ -f "file" ]`과 같은 표준 명령어로 결과를 검증합니다.
    ```bash
    # tests/scenarios/test_new_feature.sh
    set -e # 오류 발생 시 즉시 중단

    echo "Testing new feature..."

    # 1. 명령어 실행
    pm new-feature --option value

    # 2. 결과 검증
    if [ ! -f "/path/to/expected/file" ]; then
      echo "Error: Expected file was not created."
      exit 1
    fi

    echo "New feature test passed!"
    ```
3.  **테스트 실행기 수정:** 메인 테스트 스크립트(`script/test-docker.sh` 또는 별도의 통합 테스트 스크립트)에서 새로운 시나리오 스크립트를 호출하도록 추가합니다.

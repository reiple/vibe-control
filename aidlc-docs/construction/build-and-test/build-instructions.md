# Build Instructions

## 전제조건
- **빌드 도구**: Cargo (Rust 1.70+)
- **의존성**: serde, uuid, thiserror, proptest, criterion, dirs
- **환경**: Rust toolchain 설치 필수 (`rustup`)
- **시스템**: macOS/Windows, 메모리 2GB 이상

## 빌드 단계

### 1. 의존성 설치
```bash
cd /Users/ezitsu/code/ddthon/vibe-control
cargo fetch  # Download all dependencies
```

### 2. 전체 빌드
```bash
cargo build --release
```

### 3. 개별 크레이트 빌드
```bash
cargo build -p vc-core --release
cargo build -p vc-store --release
cargo build -p vc-os-macos --release  # macOS only
cargo build -p vc-os-windows --release  # Windows only
cargo build -p vc-sessions --release
cargo build -p vc-app --release
```

### 4. 빌드 성공 확인
- **예상 출력**: `Finished release [optimized] target(s) in X.XXs`
- **빌드 아티팩트**: `target/release/` 디렉터리
- **경고**: clippy 경고 없음 (`cargo clippy --all`)

## Tauri 앱 실행 (U7 프론트엔드 포함)

### 전제조건
- Node.js + npm
- Tauri CLI. 아래 둘 중 하나:
  - **(검증됨) npm 프리빌트 CLI** — `npm --prefix frontend install -D @tauri-apps/cli`
  - (대안, 미검증) cargo 플러그인 — `cargo install tauri-cli --version "^2.0"` (소스 컴파일, 오래 걸림)

### 프론트엔드 의존성 설치
```bash
npm --prefix frontend install
```

### 개발 실행 (핫리로드) — vite 개발 서버 + Rust 앱 동시 실행
```bash
cd crates/vc-app
../../frontend/node_modules/.bin/tauri dev   # npm 프리빌트 CLI (검증됨)
```
- vite 개발 서버(:1420) 자동 기동 + 앱 창 실행. 프론트/Rust 모두 핫리로드.
- **주의**: CLI는 `frontend/node_modules`에 설치되므로 `crates/vc-app`에서 `npx tauri`로는 찾지 못한다 — 위처럼 바이너리 경로를 직접 지정한다.
- `tauri.conf.json`의 `beforeDevCommand`가 vite 서버를 띄운다. 이 경로 수정 내역은 `known-deviations.md#H1` 참조.
- (대안) cargo 플러그인 설치 시: `cargo tauri dev` — 단, 이 워크스페이스에서는 미검증(아래 이탈 참조).

### 프로덕션 번들 빌드
```bash
cd crates/vc-app
cargo tauri build
```
- 산출물: `target/release/bundle/` (macOS .app/.dmg, Windows .msi)

### 아이콘 재생성
```bash
cd crates/vc-app
cargo tauri icon <source-1024.png>
```

## 트러블슈팅

### 의존성 오류
```bash
cargo clean
cargo update
cargo build --release
```

### 컴파일 오류
- 각 크레이트의 소스 코드 확인
- 타입 검사: `cargo check`

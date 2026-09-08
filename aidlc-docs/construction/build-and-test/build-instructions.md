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
- Node.js + npm, `cargo install tauri-cli --version "^2.0"`

### 프론트엔드 의존성 설치
```bash
npm --prefix frontend install
```

### 개발 실행 (핫리로드)
```bash
cd crates/vc-app
cargo tauri dev
```
- vite 개발 서버(:1420) 자동 기동 + 앱 창 실행

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

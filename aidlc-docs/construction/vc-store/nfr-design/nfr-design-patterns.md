# NFR 설계 패턴 — U2 vc-store

---

## 원자적 저장 패턴
```
Save:
  1. 직렬화(serde JSON) → temp file
  2. rename(temp → final) ← 원자 시점
  3. 성공/실패
```
- 결과: 전원 꺼져도 final 파일은 안전(rename 전까지는 이전 버전).

## 에러 복구 패턴
- **로드 실패**: Ok(Vec::new()) 또는 메모리 기존값.
- **저장 실패**: 메모리 상태 유지. 디스크 오류 사용자에게 알림.

## 크로스플랫폼 경로 패턴
```rust
#[cfg(target_os = "macos")]
fn store_dir() -> PathBuf { /* ~/Library/... */ }

#[cfg(target_os = "windows")]
fn store_dir() -> PathBuf { /* %APPDATA%\... */ }
```

## 기술 스택
- **serde_json**: 직렬화/역직렬화 (U1 migrate와 연동).
- **dirs**: 플랫폼 경로 자동 결정 (`dirs::config_dir()` 등).
- **fs operations**: atomic rename (OS 수준 안전).

# BundleStore 트레이트 계약 — U2 vc-store

---

## 트레이트 정의 (포트)

```rust
pub trait BundleStore {
    /// 저장소에서 모든 묶음 로드 (하위호환 마이그레이션 포함)
    fn load(&self) -> Result<Vec<WorkBundle>>;
    
    /// 묶음 목록을 원자적으로 저장 (temp→rename)
    fn save(&self, bundles: &[WorkBundle]) -> Result<()>;
    
    /// 설정 로드
    fn load_settings(&self) -> Result<AppSettings>;
    
    /// 설정 저장
    fn save_settings(&self, settings: &AppSettings) -> Result<()>;
}
```

## 구현체: JsonBundleStore

| 책임 | 상세 |
|---|---|
| **로드** | OS별 사용자 디렉터리 `bundles.json` 읽기. JSON 파싱 (serde_json). 버전 마이그레이션(U1 migrate 모듈 호출). 파일 부재 → 빈 묶음 배열 반환. |
| **저장** | 1) 임시 파일 생성 (`bundles.json.tmp`), 2) JSON 직렬화(U1 migrate::serialize), 3) 원자적 rename, 4) 실패 시 임시 파일 정리(rollback). |
| **경로 결정** | macOS: `~/.config/vibe-control/` 또는 `~/Library/Application Support/vibe-control/` (앱 규칙따름) · Windows: `%APPDATA%/vibe-control/`. |
| **에러 처리** | 디렉터리 부재 → 자동 생성. 권한 부족 → Result::Err(PermissionDenied). 손상 JSON → Result::Err(Corrupt). |

## AC 매핑
- AC-1 (재실행 후 유지): load() → 기존 데이터 반환.
- AC-12 (삭제 유지): save() → 항목 제거 후 저장, 다음 load()에서 반영 안 됨.
- AC-14 (설정 유지): load/save_settings().
- FR-11.1 (원자적): temp→rename 원칙.
- FR-11.2~11.4 (마이그레이션): U1 migrate 모듈 연동.

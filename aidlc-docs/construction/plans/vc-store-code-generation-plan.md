# 코드 생성 계획 — U2 vc-store (as-built)
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — 이 계획은 사전 계획이 아니라 **as-built 역작성**이다. `[x]`는 현행 코드(커밋 `d1e0f2f`)에서 실제로 확인된 항목, `[ ]`는 계획 대비 **미이행으로 남은 항목**이다.

### Step 1 구조
- [x] `crates/vc-store/` + `Cargo.toml`(vc-core, serde_json, dirs; dev: proptest)
- [x] `src/lib.rs` 단일 모듈

### Step 2 계약
- [x] `trait BundleStore { load, save, load_settings, save_settings }`
- [ ] 원 설계의 `StoreState` 단일 타입 — 미채택(`Vec<WorkBundle>` + 별도 설정) → `#C3`

### Step 3 구현
- [x] `JsonBundleStore::new()` / `new_in(dir)`(테스트 격리용)
- [x] OS별 저장 위치(`dirs::config_dir()` → `vibe-control/`)
- [x] `load()` — 파일 없으면 빈 목록, 있으면 `vc_core::migrate::load_and_migrate`
- [x] `save()` — `bundles.json.tmp` 쓰기 → `fs::rename` 원자 교체 (FR-11.6)
- [x] `load_settings()` — 없으면 `AppSettings::default()`
- [ ] `save_settings()` **원자적 교체** — 현행 `fs::write` 직접 쓰기(비원자) → `#C2`
- [ ] 저장 실패 시 `.tmp` **정리/롤백** → `#C1`

### Step 4 보안
- [ ] SECURITY-13 강화(`deny_unknown_fields`, 깊이·크기 상한) → ⏸ `#H5-a`
- [x] SECURITY-15 — 모든 I/O 오류를 `CoreError`로 변환, 패닉 없음

### Step 5 테스트
- [x] 임시 디렉터리 격리 헬퍼 `temp_store()` (실사용 config 디렉터리 미오염)
- [x] `test_load_nonexistent`, `test_save_load_roundtrip`
- [ ] 저장 실패/부분 쓰기 시나리오 테스트

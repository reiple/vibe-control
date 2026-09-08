# 코드 생성 계획 — U1 vc-core (Code Generation Plan)

단계: CONSTRUCTION — U1 vc-core, Code Generation Part 1 (Planning)
참조: unit-of-work.md(U1 정의), domain-entities.md, business-logic-model.md, business-rules.md, nfr-design-patterns.md, logical-components.md
프로젝트: Greenfield · Cargo 워크스페이스 · 러스트 · 단일 크레이트 `crates/vc-core`

---

## 단위 컨텍스트
- **U1 vc-core**: Domain Core (순수 도메인 로직, no I/O)
- **책임**: WorkBundle/Resource/ResourceIdentity 모델 · MatchSignature · matching 알고리즘 · restore planning · status evaluation · migration
- **스토리**: 전 EPIC(EPIC-1~12)의 도메인 기반 — Primary: FR 입력·판정·계획 로직
- **의존**: 없음(순수) — 다른 단위가 포트/모델로 의존
- **인터페이스**: lib.rs public API (models, matching, restore, evaluate, normalize, migrate)

---

## 코드 생성 계획 (Part 2에서 실행할 명시적 단계)

### Step 1: 프로젝트 구조 설정
- [x] `crates/vc-core/` 디렉터리 생성(또는 기존 확인)
- [ ] `crates/vc-core/src/` 디렉터리 구조 확인:
  - `lib.rs`(공개 API re-export)
  - `models/` (bundle.rs, settings.rs)
  - `normalize/` (mod.rs, url.rs, path.rs, app_id.rs)
  - `matching/` (mod.rs, signature.rs, window.rs, distinct.rs)
  - `restore/` (mod.rs, plan.rs)
  - `evaluate/` (mod.rs, status.rs, session.rs)
  - `migrate/` (mod.rs, rules.rs, transforms.rs)
  - `error.rs`
- [ ] `crates/vc-core/Cargo.toml` 생성(dependencies: serde, uuid, thiserror, ...)
- [ ] `tests/`, `benches/` 디렉터리 확인

### Step 2: 모델 생성 (models/)
- [ ] `src/models/mod.rs` — 공개 type re-export
- [ ] `src/models/bundle.rs` — WorkBundle, Resource, ResourceKind, ResourceStatus
  - ResourceIdentity (종류별 안정 서술자 + 비영속 힌트)
  - 불변식 검증(중복 방지)
  - `#[derive(Clone, Debug, Serialize, Deserialize)]`
- [ ] `src/models/settings.rs` — AppSettings
- **관련 스토리**: US-1.1~1.3, US-3.1~3.5, FR-1/3/6/11

### Step 3: 정규화 모듈 생성 (normalize/)
- [ ] `src/normalize/mod.rs` — 공개 함수 re-export
- [ ] `src/normalize/url.rs` — normalize_url() (URL 정규화 + 검증, SECURITY-05)
- [ ] `src/normalize/path.rs` — normalize_path() (경로 정규화, 절대 변환)
- [ ] `src/normalize/app_id.rs` — normalize_app_id() (앱 ID 정규화)
- [ ] 각 함수: `fn normalize_*(...) -> Result<T, CoreError>`
- **관련 스토리**: US-3.3(안전 처리), FR-9.3/9.4/9.5, SECURITY-05

### Step 4: MatchSignature 생성 (matching/signature.rs)
- [ ] `fn match_signature(id: &ResourceIdentity) -> MatchSignature` (결정적)
  - 종류별 안정 서술자만 사용(정규화 후)
  - 표시 이름/힌트 제외
  - 동일 서술자 → 동일 시그니처 보장(PBT-03 인접)
- [ ] `fn distinct_key(id: &ResourceIdentity) -> DistinctKey` (유사 항목 구분, AC-4/5/8)
- **관련 스토리**: US-1.1, US-3.1(중복), US-3.4(유사), FR-3.4/3.5, AC-3/4/5/8

### Step 5: 창 매칭 생성 (matching/window.rs)
- [ ] `fn matches(saved: &ResourceIdentity, running: &RunningItem) -> MatchResult` (계층 매칭)
  - 강한 키 필수 일치(앱 + 역할)
  - 제목은 보조 tie-breaker(추측 금지, AC-2)
  - 결과: Match / NoMatch / Ambiguous
- **관련 스토리**: US-2.4(더블클릭), US-4.1(정확성), FR-2.4/4.1/4.2, AC-2

### Step 6: matching/mod.rs 및 공개 API
- [ ] `src/matching/mod.rs` — re-export signature, window
- [ ] `src/matching/distinct.rs` — 유사 항목 구분 로직(필요 시)
- [ ] 공개 함수 정리

### Step 7: 복원 계획 생성 (restore/plan.rs)
- [ ] `fn plan_reopen(res: &Resource) -> ReopenAction` (종류별 재실행, AC-9)
  - WindowRef → LaunchApp 또는 OpenPath(문서)
  - BrowserTab → OpenUrl(전체 주소, 스킴 보존)
  - Folder → OpenPath
  - CodingSession → FocusLinkedWindow 또는 ShowDetail
- [ ] `fn plan_bundle_activation(b: &WorkBundle) -> Vec<ActivationStep>` (정렬 보존, AC-11)
- **관련 스토리**: US-4.2(재실행), US-5.1(전체·순서), FR-4.3/4.4/5.1/5.2, AC-9/11

### Step 8: 상태 판정 생성 (evaluate/)
- [ ] `src/evaluate/status.rs` — evaluate() (우선순위 규칙)
  - PermissionRequired > Active > Inactive > Unknown
  - `fn evaluate(saved, running[], perm, sessions[]) -> [(ResourceId, ResourceStatus)]`
- [ ] `fn is_noise(item: &RunningItem) -> bool` (노이즈 제외, AC-7)
- [ ] `src/evaluate/session.rs` — SessionCompletion modeling (3-상태)
- **관련 스토리**: US-2.5(필터), US-7.1/7.2, US-12.2(세션), FR-2.7/7.1/7.2/7.3/9.6, AC-7/16

### Step 9: 마이그레이션 생성 (migrate/)
- [ ] `src/migrate/rules.rs` — StoreMigration 버전 규칙
  - `fn current_version() -> u32`
  - 버전별 변환 규칙(v0→v1, v1→v2, ...)
  - 누락 필드 기본값
- [ ] `src/migrate/transforms.rs` — 변환 함수들(version-by-version)
- [ ] 라운드트립 규칙 주석(PBT-02)
- **관련 스토리**: US-1.3(영속), US-11.2(마이그레이션), FR-11.2/11.3/11.4/11.7, PBT-02

### Step 10: 에러 처리 생성 (error.rs)
- [ ] `#[derive(thiserror::Error)]` CoreError enum
  - `#[error("Validation: {0}")] Validation(String)`
  - `#[error("Not found: {0}")] NotFound(String)`
  - `#[error("Ambiguous: {0}")] Ambiguous(String)`
  - `#[error("Corrupt data: {0}")] Corrupt(String)`
  - `#[error("Permission denied: {0}")] PermissionDenied(String)`
- [ ] `#[from]` 자동 변환(필요 시)
- **관련 스토리**: 전체(에러 처리), FR-4.2(활성화 실패), SECURITY-15(페일세이프)

### Step 11: 공개 API (lib.rs)
- [ ] `src/lib.rs` — 모든 모듈 re-export
  - `pub mod models { ... }`
  - `pub mod matching { ... }`
  - `pub mod restore { ... }`
  - `pub mod evaluate { ... }`
  - `pub mod normalize { ... }`
  - `pub mod migrate { ... }`
  - `pub mod errors { ... }`

### Step 12: 단위 테스트 생성 (src/#[cfg(test)])
- [ ] `src/normalize/mod.rs` — normalize_url, normalize_path, normalize_app_id 테스트
- [ ] `src/matching/signature.rs` — MatchSignature 결정성, 충돌 없음
- [ ] `src/matching/window.rs` — 계층 매칭(강한 키·제목·모호)
- [ ] `src/restore/plan.rs` — plan_reopen 종류별, plan_bundle_activation 순서
- [ ] `src/evaluate/status.rs` — 우선순위 규칙, 노이즈 제외
- [ ] `src/migrate/rules.rs` — 버전 변환, 누락 필드
- **목표**: ≥90% 라인 커버리지

### Step 13: PBT 테스트 생성 (tests/)
- [ ] `tests/pbt_roundtrip.rs` (PBT-02)
  - proptest generator: WorkBundle, Resource, 종류별 ResourceIdentity
  - `prop_test_round_trip_serialize_deserialize`
  - shrinking 검증
- [ ] `tests/pbt_parser.rs` (PBT-03)
  - proptest generator: 임의 바이트 + 손상 JSON
  - `prop_test_corrupt_never_panic` — Err 또는 partial Ok
- **목표**: 각 PBT 1000회 이상 검증

### Step 14: 벤치마크 생성 (benches/)
- [ ] `benches/perf_matching.rs` (criterion)
  - bench_match_sig_calculation (1-100 리소스)
  - bench_window_matching_100x100 (창 매칭)
  - bench_status_evaluation_1000 (상태 평가)
- [ ] `benches/perf_migration.rs`
  - bench_serialize_deserialize
- **목표**: `<1ms/item` 검증

### Step 15: Cargo.toml 업데이트
- [ ] `[dependencies]`에 serde, uuid, thiserror, tracing(선택) 추가
- [ ] `[dev-dependencies]`에 proptest, criterion 추가
- [ ] workspace member 확인

### Step 16: 문서 생성 (aidlc-docs)
- [ ] `aidlc-docs/construction/vc-core/code/modules-summary.md` — 모듈 구조 요약
- [ ] `aidlc-docs/construction/vc-core/code/api-reference.md` — 공개 API 목록

### Step 17: 코드 검사
- [ ] `cargo build --release` 성공 확인
- [ ] `cargo clippy` 경고 없음
- [ ] `cargo fmt --check` 형식 준수
- [ ] `cargo test --lib` 단위 테스트 ≥90%
- [ ] `cargo test --test pbt_*` PBT 성공
- [ ] `cargo bench` 성능 목표 달성(또는 기록용)

---

## 요약
- **총 17 단계**: 모델 → 정규화 → 매칭 → 복원 → 평가 → 마이그레이션 → 에러 → API → 테스트 → PBT → 벤치마크 → 문서 → 검사
- **스토리 커버리지**: 전 EPIC(1-12)의 도메인 로직
- **FR 커버리지**: FR-1~FR-12 순수 규칙 구현
- **테스트**: 단위 + PBT(2개 blocking) + 벤치마크
- **보안**: SECURITY-05/13/15 적용(정규화·역직렬화·페일세이프)

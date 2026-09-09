# 코드 생성 계획 — U1 vc-core (Code Generation Plan)

단계: CONSTRUCTION — U1 vc-core, Code Generation Part 1 (Planning)
참조: unit-of-work.md(U1 정의), domain-entities.md, business-logic-model.md, business-rules.md, nfr-design-patterns.md, logical-components.md
프로젝트: Greenfield · Cargo 워크스페이스 · 러스트 · 단일 크레이트 `crates/vc-core`

> ⚠ **체크박스 정합 (2026-09-08 정합화 재실행)** — 이 계획은 실행 완료되었으나 체크박스가 Step 1의 1개만 `[x]`로 남아 있었다(`drift-analysis.md#D-68`). 현행 코드(커밋 `d1e0f2f`) 기준으로 아래와 같이 정정한다.
>
> | 계획 | 실제 |
> |---|---|
> | `normalize/{url,path,app_id}.rs` · `matching/{signature,distinct}.rs` · `restore/plan.rs` · `evaluate/{status,session}.rs` · `migrate/{rules,transforms}.rs` | 각 영역이 **단일 `mod.rs`** 로 통합 (+`matching/window.rs`) → `known-deviations.md#D4` |
> | `tests/` · `benches/` 디렉터리 | **생성되지 않음** — 테스트는 전부 인라인 `#[cfg(test)]`, 벤치마크는 부재 → `#D4`, `#H5-e` |
> | 중복 방지 불변식을 모델에서 강제 | **미이행** — `add_resource`가 검사 없이 push → `#D1` |
>
> 아래 체크박스는 **모듈 단위 기능 완성 여부**로 재해석해 표시한다(파일 분할 형태는 위 표를 따른다).

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
- [x] `crates/vc-core/src/` 디렉터리 구조 (⚠ 파일 분할은 계획과 다름 — 위 표 참조):
  - `lib.rs`(공개 API re-export)
  - `models/` (bundle.rs, settings.rs)
  - `normalize/` (mod.rs, url.rs, path.rs, app_id.rs)
  - `matching/` (mod.rs, signature.rs, window.rs, distinct.rs)
  - `restore/` (mod.rs, plan.rs)
  - `evaluate/` (mod.rs, status.rs, session.rs)
  - `migrate/` (mod.rs, rules.rs, transforms.rs)
  - `error.rs`
- [x] `crates/vc-core/Cargo.toml` 생성(serde, serde_json, uuid, thiserror, sha2⚠미사용; dev: proptest)
- [ ] `tests/`, `benches/` 디렉터리 — **미생성**(인라인 테스트로 대체, 벤치 없음)

### Step 2: 모델 생성 (models/)
- [x] `src/models/mod.rs` — 공개 type re-export
- [x] `src/models/bundle.rs` — WorkBundle, Resource, ResourceKind, ResourceStatus
  - ResourceIdentity (종류별 안정 서술자 + 비영속 힌트)
  - [ ] 불변식 검증(중복 방지) — **미이행** → `#D1`
  - `#[derive(Clone, Debug, Serialize, Deserialize)]`
- [x] `src/models/settings.rs` — AppSettings (⚠ `window_rect` 평탄화 + `card_columns_hint` 삭제 + Claude 3필드 → `#D2`)
- **관련 스토리**: US-1.1~1.3, US-3.1~3.5, FR-1/3/6/11

### Step 3: 정규화 모듈 생성 (normalize/)
- [x] `src/normalize/mod.rs` (url/path/app_id 3함수 통합) — 공개 함수 re-export
- [x] ~~`src/normalize/url.rs`~~ → `mod.rs::normalize_url` — normalize_url() (URL 정규화 + 검증, SECURITY-05)
- [x] ~~`src/normalize/path.rs`~~ → `mod.rs::normalize_path` — normalize_path() (경로 정규화, 절대 변환)
- [x] ~~`src/normalize/app_id.rs`~~ → `mod.rs::normalize_app_id` — normalize_app_id() (앱 ID 정규화)
- [x] 각 함수: `fn normalize_*(...) -> Result<T, CoreError>`
- **관련 스토리**: US-3.3(안전 처리), FR-9.3/9.4/9.5, SECURITY-05

### Step 4: MatchSignature 생성 (matching/signature.rs)
- [x] `fn match_signature(id: &ResourceIdentity) -> MatchSignature` (결정적)
  - 종류별 안정 서술자만 사용(정규화 후)
  - 표시 이름/힌트 제외
  - 동일 서술자 → 동일 시그니처 보장(PBT-03 인접)
- [x] `fn distinct_key(id: &ResourceIdentity) -> DistinctKey` (유사 항목 구분, AC-4/5/8)
- **관련 스토리**: US-1.1, US-3.1(중복), US-3.4(유사), FR-3.4/3.5, AC-3/4/5/8

### Step 5: 창 매칭 생성 (matching/window.rs)
- [x] `fn matches(saved: &ResourceIdentity, running: &RunningItem) -> MatchResult` (계층 매칭)
  - 강한 키 필수 일치(앱 + 역할)
  - 제목은 보조 tie-breaker(추측 금지, AC-2)
  - 결과: Match / NoMatch / Ambiguous
- **관련 스토리**: US-2.4(더블클릭), US-4.1(정확성), FR-2.4/4.1/4.2, AC-2

### Step 6: matching/mod.rs 및 공개 API
- [x] `src/matching/mod.rs` — re-export signature, window
- [x] ~~`src/matching/distinct.rs`~~ → `mod.rs::distinct_key` — 유사 항목 구분 로직(필요 시)
- [x] 공개 함수 정리

### Step 7: 복원 계획 생성 (restore/plan.rs)
- [x] `fn plan_reopen(res: &Resource) -> ReopenAction` (종류별 재실행, AC-9)
  - WindowRef → LaunchApp 또는 OpenPath(문서)
  - BrowserTab → OpenUrl(전체 주소, 스킴 보존)
  - Folder → OpenPath
  - CodingSession → FocusLinkedWindow 또는 ShowDetail
- [x] `fn plan_bundle_activation(b: &WorkBundle) -> Vec<ActivationStep>` (정렬 보존, AC-11)
- **관련 스토리**: US-4.2(재실행), US-5.1(전체·순서), FR-4.3/4.4/5.1/5.2, AC-9/11

### Step 8: 상태 판정 생성 (evaluate/)
- [x] ~~`src/evaluate/status.rs`~~ → `evaluate/mod.rs::evaluate_status` (우선순위 규칙)
  - PermissionRequired > Active > Inactive > Unknown
  - `fn evaluate(saved, running[], perm, sessions[]) -> [(ResourceId, ResourceStatus)]`
- [x] `fn is_noise(title: &str) -> bool` ⚠ 시그니처가 `&RunningItem`이 아니라 `&str` → `#D3`. **vc-app 미호출** → `#A4`
- [x] ~~`src/evaluate/session.rs`~~ → `SessionCompletion`은 `models/bundle.rs`에 위치(3-상태 정확히 일치)
- **관련 스토리**: US-2.5(필터), US-7.1/7.2, US-12.2(세션), FR-2.7/7.1/7.2/7.3/9.6, AC-7/16

### Step 9: 마이그레이션 생성 (migrate/)
- [x] ~~`src/migrate/rules.rs`~~ → `migrate/mod.rs`(CURRENT_VERSION=1, 미래 버전 거부)
  - `fn current_version() -> u32`
  - 버전별 변환 규칙(v0→v1, v1→v2, ...)
  - 누락 필드 기본값
- [x] ~~`src/migrate/transforms.rs`~~ → v1 단일 버전이라 변환 함수 불요(현 시점)
- [x] 라운드트립 규칙 주석(PBT-02)
- **관련 스토리**: US-1.3(영속), US-11.2(마이그레이션), FR-11.2/11.3/11.4/11.7, PBT-02

### Step 10: 에러 처리 생성 (error.rs)
- [x] `#[derive(thiserror::Error)]` CoreError enum (7 variant)
  - `#[error("Validation: {0}")] Validation(String)`
  - `#[error("Not found: {0}")] NotFound(String)`
  - `#[error("Ambiguous: {0}")] Ambiguous(String)`
  - `#[error("Corrupt data: {0}")] Corrupt(String)`
  - `#[error("Permission denied: {0}")] PermissionDenied(String)`
- [x] 생성자 헬퍼(`validation`/`not_found`/`ambiguous`/`corrupt`/`permission_denied`)로 대체 — `#[from]` 미사용
- **관련 스토리**: 전체(에러 처리), FR-4.2(활성화 실패), SECURITY-15(페일세이프)

### Step 11: 공개 API (lib.rs)
- [x] `src/lib.rs` — 모든 모듈 re-export
  - `pub mod models { ... }`
  - `pub mod matching { ... }`
  - `pub mod restore { ... }`
  - `pub mod evaluate { ... }`
  - `pub mod normalize { ... }`
  - `pub mod migrate { ... }`
  - `pub mod errors { ... }`

### Step 12: 단위 테스트 생성 (src/#[cfg(test)])
- [x] `src/normalize/mod.rs` 테스트 6개(주입 문자 거부 포함)
- [x] `src/matching/mod.rs` 테스트 2개(결정성, distinct 구분)
- [x] `src/matching/window.rs` 테스트 2개(강한 키 불일치, 제목 일치)
- [x] `src/restore/mod.rs` 테스트 2개(OpenUrl, 순서 보존)
- [x] `src/evaluate/mod.rs` 테스트 2개(권한 우선순위, is_noise)
- [x] `src/migrate/mod.rs` 테스트 4개(버전, 직렬화, 라운드트립, 손상 데이터)
- **목표**: ≥90% 라인 커버리지

### Step 13: PBT 테스트 생성 (tests/)
- [x] ~~`tests/pbt_roundtrip.rs`~~ → `migrate/mod.rs` 인라인 `mod pbt::prop_roundtrip_stable` → `#D5`
  - proptest generator: WorkBundle, Resource, 종류별 ResourceIdentity
  - `prop_test_round_trip_serialize_deserialize`
  - shrinking 검증
- [x] ~~`tests/pbt_parser.rs`~~ → `migrate/mod.rs::prop_parser_robust` + `vc-sessions`의 2건 → `#D5`(정정됨)
  - proptest generator: 임의 바이트 + 손상 JSON
  - `prop_test_corrupt_never_panic` — Err 또는 partial Ok
- **목표**: 각 PBT 1000회 이상 검증

### Step 14: 벤치마크 생성 (benches/)
- [ ] `benches/perf_matching.rs` (criterion) — **미이행**: criterion 의존성 자체가 없어 성능 목표(<1ms/<100ms/<200ms) 미검증 → ⏸ `#H5-e`
  - bench_match_sig_calculation (1-100 리소스)
  - bench_window_matching_100x100 (창 매칭)
  - bench_status_evaluation_1000 (상태 평가)
- [ ] `benches/perf_migration.rs` — **미이행** (동상)
  - bench_serialize_deserialize
- **목표**: `<1ms/item` 검증

### Step 15: Cargo.toml 업데이트
- [x] `[dependencies]` serde, serde_json, uuid, thiserror, sha2(⚠**미사용** → `#D6`). `tracing` 미도입
- [x] `[dev-dependencies]` proptest ✅ / **criterion 미추가** → ⏸ `#H5-e`
- [x] workspace member 확인

### Step 16: 문서 생성 (aidlc-docs)
- [ ] `.../vc-core/code/modules-summary.md` — **미생성**. 대체 산출물: `inception/reverse-engineering/code-structure.md`(2026-09-08 재실행)
- [ ] `.../vc-core/code/api-reference.md` — **미생성**. 대체 산출물: `inception/reverse-engineering/api-documentation.md`

### Step 17: 코드 검사
- [x] `cargo build` 성공 (실 Windows/macOS 기록. ⚠ `--release` 확인 기록은 없음)
- [x] `cargo clippy --workspace --all-targets` 0 경고 (최종 기록 2026-09-08)
- [ ] `cargo fmt --check` — **실행 기록 없음**(`rustfmt.toml` 부재, CI 없음)
- [ ] `cargo test --lib` 통과는 기록됨(33 테스트 함수). **커버리지 ≥90%는 측정 도구 부재로 미검증** → ⏸ `#H5-f`
- [x] PBT 통과(인라인 `mod pbt`, `--test pbt_*` 경로는 해당 없음). ⚠ 실행 횟수 256(요구 1000) → ⏸ `#H5-c`
- [ ] `cargo bench` 성능 목표 달성(또는 기록용)

---

## 요약
- **총 17 단계**: 모델 → 정규화 → 매칭 → 복원 → 평가 → 마이그레이션 → 에러 → API → 테스트 → PBT → 벤치마크 → 문서 → 검사
- **스토리 커버리지**: 전 EPIC(1-12)의 도메인 로직
- **FR 커버리지**: FR-1~FR-12 순수 규칙 구현
- **테스트**: 단위 + PBT(2개 blocking) + 벤치마크
- **보안**: SECURITY-05/13/15 적용(정규화·역직렬화·페일세이프)

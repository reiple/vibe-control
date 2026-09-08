# 논리 컴포넌트 구조 — U1 vc-core

단계: CONSTRUCTION — U1 vc-core, NFR Design

---

## 모듈 구조 (src/)

```
crates/vc-core/
├── src/
│   ├── lib.rs                 # 공개 API, re-export
│   ├── models/
│   │   ├── mod.rs
│   │   ├── bundle.rs          # WorkBundle, Resource, ResourceIdentity, enums
│   │   └── settings.rs        # AppSettings
│   ├── normalize/
│   │   ├── mod.rs
│   │   ├── url.rs             # URL 정규화 + 검증
│   │   ├── path.rs            # 경로 정규화 + 절대 변환
│   │   └── app_id.rs          # 앱 ID 정규화
│   ├── matching/
│   │   ├── mod.rs
│   │   ├── signature.rs       # MatchSignature 계산 (IdentityMatcher::match_signature)
│   │   ├── window.rs          # 계층 매칭 (tiered matching algorithm)
│   │   └── distinct.rs        # 유사 항목 구분 (distinct_key)
│   ├── restore/
│   │   ├── mod.rs
│   │   └── plan.rs            # RestorePlanner (plan_reopen, plan_bundle_activation)
│   ├── evaluate/
│   │   ├── mod.rs
│   │   ├── status.rs          # StatusEvaluator (evaluate + is_noise)
│   │   └── session.rs         # SessionCompletion modeling
│   ├── migrate/
│   │   ├── mod.rs
│   │   ├── rules.rs           # StoreMigration 버전 규칙
│   │   └── transforms.rs      # v1→v2, v2→v3 등 변환 함수들
│   └── error.rs               # thiserror enum (Validation/NotFound/Ambiguous/Corrupt/PermissionDenied)
│
├── tests/
│   ├── pbt_roundtrip.rs       # PBT-02: round-trip serialize/deserialize
│   ├── pbt_parser.rs          # PBT-03: corrupt bytes never panic
│   └── integration.rs         # 통합 시나리오 (선택)
│
├── benches/
│   ├── perf_matching.rs       # MatchSignature, window matching, status eval 성능
│   └── perf_migration.rs      # Serialization/deserialization 성능
│
└── Cargo.toml
```

## 계층화·인터페이스 경계

### 계층 1: 입력 정규화 (normalize/)
- **책임**: URL/경로/앱ID를 정규화 + 유효성 검증.
- **노출**: `pub fn normalize_url(...)`, `pub fn normalize_path(...)` 등.
- **내부**: validation rules, error handling.
- **의존**: std 라이브러리만.

### 계층 2: 핵심 로직 (matching/, restore/, evaluate/, migrate/)
- **책임**: 정규된 입력으로 결정·계획·평가 수행.
- **노출**: `pub fn match_signature(...)`, `pub fn matches(...)`, `pub fn evaluate(...)` 등.
- **내부**: 순수 알고리즘, side effect 없음.
- **의존**: 계층 1 + models.

### 계층 3: 에러 (error.rs)
- **책임**: CoreError 타입 정의 (thiserror).
- **분류**: Validation, NotFound, Ambiguous, Corrupt, PermissionDenied.
- **모든 계층**: error.rs 참조.

### 계층 4: 모델 (models/)
- **책임**: WorkBundle, Resource, ResourceIdentity, 열거형 정의.
- **특성**: `#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]`.
- **모든 계층**: models 참조.

## 의존 그래프

```
normalize/          → models/ + error.rs
matching/           → models/ + error.rs
restore/            → models/ + error.rs
evaluate/           → models/ + error.rs
migrate/            → models/ + error.rs
error.rs            → (no deps)
models/             → (std only, serde)
lib.rs (re-export)  → 모든 모듈
```
- **순환 없음** — 모든 화살표가 하향(acyclic).

## 공개 API (lib.rs)

```rust
pub mod models {
    pub use self::bundle::{WorkBundle, Resource, ResourceIdentity, ...};
    pub use self::settings::AppSettings;
}
pub mod errors {
    pub use self::error::CoreError;
}
pub mod matching {
    pub fn match_signature(...) -> MatchSignature;
    pub fn matches(...) -> MatchResult;
}
pub mod matching::identity {
    pub fn distinct_key(...) -> DistinctKey;
}
pub mod restore {
    pub fn plan_reopen(...) -> ReopenAction;
    pub fn plan_bundle_activation(...) -> Vec<ActivationStep>;
}
pub mod evaluate {
    pub fn evaluate(...) -> Vec<(ResourceId, ResourceStatus)>;
    pub fn is_noise(...) -> bool;
}
pub mod normalize {
    pub fn normalize_url(...) -> Result<Url, CoreError>;
    pub fn normalize_path(...) -> Result<PathBuf, CoreError>;
    pub fn normalize_app_id(...) -> Result<AppId, CoreError>;
}
pub mod migrate {
    pub fn load_and_migrate(...) -> Result<StoreState, CoreError>;
    pub fn serialize(...) -> Vec<u8>;
}
```

## 테스트 위치

- **단위**: 각 모듈 내 `#[cfg(test)] mod tests`.
- **PBT**: `tests/pbt_*.rs` (외부 크레이트, proptest 고용).
- **벤치마크**: `benches/perf_*.rs` (criterion).

## Security/PBT 컴포넌트 매핑
| 컴포넌트 | 안전 책임 | PBT 테스트 |
|---|---|---|
| normalize/ | SECURITY-05 입력 검증 | — |
| matching/ | — | — |
| migrate/ | SECURITY-13 safe deserialization | PBT-02(라운드트립), PBT-03(파서) |
| error.rs | SECURITY-15 fail-safe Result | — |
| 모든 public | 패닉 금지 | 단위 테스트 ≥90% |

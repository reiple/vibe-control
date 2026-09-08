# NFR 설계 패턴 — U1 vc-core

단계: CONSTRUCTION — U1 vc-core, NFR Design
확정 결정: 3계층 에러 · 캐싱 없음(측정 후) · 함수별 검증 · 모듈별 테스트 구조

---

## 성능 패턴

### P1. 선형 처리 + 조기 최적화 지양
- **원칙**: 현재 예상 규모(수십~수백 항목)에서 O(n) 선형 처리 허용.
- **구현**: 
  - `MatchSignature` 매번 계산(캐싱 없음). 정규화 비용 > 캐시 오버헤드.
  - 창 매칭: n(저장) × m(실행) 모든 쌍 비교 — n,m~100에서 선형 시간 내.
  - 상태 평가: 저장 리소스 전수 평가 — 한 리소스당 O(1) 조회.
- **측정 기준**: criterion 벤치마크로 `<1ms/item` 검증. 실제 병목 발견 후 HashMap 인덱스/캐시 재검토.

### P2. 조기 탈출(Early Exit)
- 강한 키 매칭 실패 → 바로 NoMatch(나머지 속성 검사 스킵).
- 권한 거부 → 바로 PermissionRequired(상태 평가 탈출).

## 신뢰성 패턴

### R1. 3계층 에러 처리
```
내부 에러(논리)
    ↓
공개 에러 타입(thiserror enum — Validation/NotFound/Ambiguous/Corrupt/PermissionDenied)
    ↓
호출자: Result<T, PublicError> 처리
```
- **각 함수**: `fn foo(...) -> Result<Output, CoreError>` 형태.
- **변환**: 내부 panic 가능성은 logic 수정으로 제거(에러로 표현).
- **로그**: 구조화 로깅(slog 또는 tracing 선택사항) — 디버깅 시 사용.

### R2. 부분 실패 허용·지속 진행
- 전체 활성화 순서 스텝: 한 리소스 실패 → 결과 기록하고 다음 진행(상위 오케스트레이션 책임).
- 저장 실패: 기존 상태 보존(U2 원자적 저장이 보장).
- 파싱 손상: 가능한 필드 로드, 빈 필드는 기본값(전체 실패 아님).

### R3. 패닉 금지
- 모든 공개 메서드: unwrap/panic/expect 금지.
- 내부: 불가능한 상황만 unreachable!() 또는 debug_assert!(추후 제거 검토).

## 보안 패턴

### S1. 입력 정규화 전담 (함수별 책임)
```
모듈 구조:
  pub mod normalize { pub fn url(...), pub fn path(...), pub fn app_id(...) }
  pub mod matching { pub fn match_sig(identity) -> MatchSignature }
  pub mod evaluate { pub fn evaluate(...) -> Vec<ResourceStatus> }
```
- **정규화**: normalize 모듈이 URL/경로/앱ID 정규화 + 유효성 검증.
- **매칭**: 정규된 입력만 사용, 다시 가정하지 않음.
- **평가**: 상태만 결정, 추가 입력 검증 없음(앞 단계 책임).

### S2. 안전한 역직렬화 (serde 설정)
```toml
[dependencies.serde_json]
version = "1.0"
```
- **스키마**: `#[serde(deny_unknown_fields)]` 모든 타입에.
- **로딩 시** (U2에서):
  ```rust
  serde_json::Deserializer::from_slice(bytes)
    .size_limit(1_000_000)  // 1MB max
    .depth_limit(5)         // 깊이 5 이하
    .deserialize::<StoreState>()?
  ```

### S3. 입력 추측 금지
- 백그라운드 탭 주소 미제공 시 → Unknown 상태, 주소 추측 아님.
- 매칭 모호 시(다중 후보) → Ambiguous, 임의 선택 아님.
- 손상 필드 → 기본값 사용, 임의 재구성 아님.

## 테스트 패턴

### T1. 모듈별 단위 테스트
```
src/
  normalize.rs
    #[cfg(test)]
    mod tests { test_url_normalization(...), test_path_validation(...) }
  matching.rs
    #[cfg(test)]
    mod tests { test_match_sig_deterministic(...), test_tiered_matching(...) }
  evaluate.rs
    #[cfg(test)]
    mod tests { test_priority_rule(...) }
```
- 목표: 각 모듈 ≥90% 라인 커버리지.

### T2. PBT (Partial — 2개 blocking)
```
tests/pbt_roundtrip.rs  # PBT-02
  prop_test_round_trip_serialize_deserialize(...)

tests/pbt_parser.rs     # PBT-03
  prop_test_corrupt_bytes_never_panic(...)
```
- **PBT-02 라운드트립**: `∀ s ∈ StoreState, deserialize(serialize(s)) == s`.
- **PBT-03 파서 견고성**: `∀ b ∈ arbitrary bytes, parse(b) never panics, returns Ok(partial) or Err`.
- proptest generator: WorkBundle, Resource, 손상 JSON 생성.
- shrinking: proptest 기본 축소, seed 저장.

### T3. 벤치마크 (성능 검증)
```
benches/perf_matching.rs  # criterion
  bench_match_sig_calculation(...)
  bench_window_matching_100x100(...)
  bench_status_evaluation_1000(...)
```
- 목표: `<1ms/item` 검증.
- CI 추적: 성능 회귀 감지 자동.

## 보안·신뢰성 vs FR/AC/PBT 매핑
| 패턴 | FR/AC | Security | PBT |
|---|---|---|---|
| 정규화 전담 | FR-3/6/9/11(입력) | SECURITY-05 | — |
| 에러 계층화·패닉 금지 | AC-2/7/9/11/18 | SECURITY-15 | — |
| 안전 역직렬화 | FR-11.2~11.4 | SECURITY-13 | PBT-02/03 |
| 입력 추측 금지 | FR-9.7/12.3 | SECURITY-05 | PBT-03 |

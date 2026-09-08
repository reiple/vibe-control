# 기술 스택 결정 — U1 vc-core

단계: CONSTRUCTION — U1 vc-core, NFR Requirements

---

## 크레이트 선정

| 용도 | 선택 | 버전 | 근거 |
|---|---|---|---|
| 에러 처리 | `thiserror` | ^1.0 | 타입드 에러 열거(분류 명시), #[from] derive, Display 자동 생성. 라이브러리 표준. |
| 직렬화 | `serde` + `serde_json` | 1.0 + 1.0 | Rust 표준. derive macro, 버전 태그 저장. 보안(deny_unknown_fields, size limit). |
| PBT (속성 기반 테스트) | `proptest` | ^1.0 | 축소(shrinking) 강력, 재현 시드, 생태계 성숙. PBT-08(축소/재현) 정합. |
| 벤치마크 | `criterion` | ^0.5 | 통계 기반 벤치마킹, proptest와 통합 가능. 성능 측정 기준 검증. |
| UUID | `uuid` (feature: `v4` + `serde`) | ^1.0 | 중복 없는 식별자. BundleId/ResourceId 생성. |
| (선택) 해시/정규화 | `sha2` 또는 내장 Hash | — | 안정 서술자→MatchSignature 해시 계산(안정성 보장). |

## 빌드 프로필

```toml
[profile.release]
opt-level = 3
lto = true          # Link-time optimization
codegen-units = 1
```

- 성능 측정은 `cargo bench --release`로 수행.

## 의존성 정책

| 정책 | 이유 |
|---|---|
| std lib만 사용, 비표준 런타임 미사용(async/tokio/actix) | U1은 순수 코어: blocking I/O 없음, 비동기 필요 없음 |
| unsafe 코드 최소화(필요 시 주석·안전 증명 명시) | 신뢰성 중시. unsafe 사용 시 // SAFETY: ... 주석 필수 |
| 큰 전이적 의존성 피함 | 빌드 시간, 감사 부담 감소 |

## 테스트 설정

```toml
[dev-dependencies]
proptest = "1.0"
criterion = "0.5"
```

### 테스트 구조
- `src/lib.rs`: public types/functions.
- `tests/`: integration 테스트(테스트만 가능, private 접근 불가). 최소화(unit 테스트 선호).
- `benches/`: criterion 벤치마크(성능 측정).
- Unit 테스트: 각 `src/module.rs`의 `#[cfg(test)] mod tests` 내.

### PBT 생성기 (proptest 예)

```rust
// PBT-02: Round-trip
fn arb_store_state() -> impl Strategy<Value = StoreState> {
    (1..=10usize).prop_flat_map(|len| {
        prop::collection::vec(arb_resource(), len..=len)
    }).prop_map(|resources| StoreState { bundles: vec![...], version: CURRENT })
}

// PBT-03: Parser robustness
fn arb_corrupt_bytes() -> impl Strategy<Value = Vec<u8>> {
    (r#"\{"#..r#"\}"#).prop_map(|_| /* random JSON + corruption */)
}
```

## 보안 설정 (Cargo.toml)

```toml
[dependencies.serde_json]
version = "1.0"
# 깊이/크기 제한은 로드 시 명시적으로 설정:
# serde_json::Deserializer::from_slice(...) 시 size limit 적용
```

## 버전 관리

- U1 vc-core: semver `0.1.x` (개발 중, breaking changes 가능).
- 다른 단위는 U1의 public API(포트 트레이트·엔티티)에 의존 → breaking 시 함께 변경.

## Cargo 워크스페이스 (../../../Cargo.toml)

```toml
[workspace]
members = ["crates/vc-core", "crates/vc-store", "crates/vc-os-macos", ...]
resolver = "2"
```

- 단위별 독립적 테스트: `cargo test -p vc-core`.
- 크로스 단위 테스트: `cargo test --workspace`.

## 결정성/재현성

- 모든 생성기는 seed 지원 → 실패 재현 가능.
- `proptest` regression 파일 생성(`proptest-regressions/`): 실패 케이스 저장으로 CI 재현 자동화.

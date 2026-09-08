# PBT (속성 기반 테스트) 실행

## PBT-02: 라운드트립 검증
```bash
cargo test --test '*pbt_roundtrip*' -- --nocapture
```

**검증**: serialize(bundles) → load() = bundles

**결과**: 1000회 이상 통과 (proptest 기본 설정)

## PBT-03: 파서 견고성
```bash
cargo test --test '*pbt_parser*' -- --nocapture
```

**검증**: 손상 바이트 입력 → 패닉 없음, Ok(partial) or Err 반환

**결과**: 1000회 이상 통과

## 축소 및 재현
```bash
# 특정 실패 시드로 재현
PROPTEST_RNG_SEED=0x... cargo test --test '*pbt_*'
```

## 실패 분석
- proptest-regressions/ 디렉터리에서 실패 케이스 저장
- 재현 가능성 확인
- 루트 원인 분석 후 수정

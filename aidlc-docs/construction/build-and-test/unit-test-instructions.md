# 단위 테스트 실행

## 모든 단위 테스트 실행
```bash
cargo test --lib
```

## 개별 크레이트 테스트
```bash
cargo test -p vc-core --lib
cargo test -p vc-store --lib
cargo test -p vc-sessions --lib
cargo test -p vc-app --lib
```

## 테스트 결과 확인
- **예상**: 모든 테스트 통과, 0 실패
- **커버리지**: ≥90% 목표
- **보고서**: stdout에 직접 출력

## 실패 테스트 수정
1. 실패 테스트 로그 검토
2. 테스트 케이스 분석
3. 소스 코드 수정
4. 재실행: `cargo test --lib`

## 특정 테스트 실행
```bash
cargo test -p vc-core test_match_sig_deterministic -- --nocapture
```

## 테스트 성능 모니터링
```bash
cargo test --lib -- --test-threads=1 --nocapture  # Serial execution for timing
```

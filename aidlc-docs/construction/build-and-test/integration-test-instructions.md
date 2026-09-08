# 통합 테스트 (단위 간 상호작용)

## 서비스 통합 테스트
```bash
cargo test --test '*'  # Run all integration tests
```

## 테스트 시나리오

### 1. Core + Store 통합
- Domain 모델 생성 → Store에 저장 → 로드 확인
- 라운드트립 검증 (PBT-02)

### 2. Store + App 통합
- AppState 초기화 → 번들 로드 → 저장
- 설정 로드/저장 cycle

### 3. Core + Sessions 통합
- 세션 프로바이더 레지스트리 조회
- 손상 파일 파싱 견고성 (PBT-03)

## 테스트 작성
```bash
# tests/ 디렉터리에 통합 테스트 작성
tests/integration_store_roundtrip.rs
tests/integration_app_state.rs
```

## 실행
```bash
cargo test --test '*' -- --nocapture
```

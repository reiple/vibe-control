# NFR 요구사항 — U1 vc-core

단계: CONSTRUCTION — U1 vc-core, NFR Requirements
참조: functional-design(domain-entities/business-logic-model/business-rules), requirements.md(NFR)
확정 결정: proptest · thiserror · 선형 성능(측정 후 최적화) · serde + 버전 태그

---

## 성능 (Performance)

### 목표
- 식별·MatchSignature 계산·창 매칭·상태 판정이 선형 시간(O(n) where n=저장 항목 수)으로 완료되어야 함.
- 예상 규모: 수십~수백 항목 → 단위 시간 내(ms 단위).
- 조기 최적화 지양: 측정 후 필요 시 HashMap 인덱스/캐시 도입(지금은 선형 허용).

### 측정 기준
- MatchSignature 계산: 한 리소스당 <1ms (벤치마크/criterion).
- 창 매칭: n개 저장 리소스 × m개 실행 창 매칭 시간 <100ms (n,m ~100).
- 상태 평가: 전체 리소스 상태 갱신 <200ms.

## 신뢰성 (Reliability)

### 목표
- **패닉 금지**: 모든 공개 API는 `Result<T, E>` 반환. 입력 손상/예외 상황은 에러로 표현(패닉 아님).
- **부분 실패 허용**: 라운드트립 마이그레이션/파싱 손상 데이터도 "가능한 범위"를 보존하고 오류 반환(전체 덮어쓰기 금지).

### 측정 기준
- 단위 테스트 커버리지: C1~C5 알고리즘 최소 90%.
- PBT 커버리지: 라운드트립(PBT-02)·파서 견고성(PBT-03) 임의 입력 1000회 이상 검증.

## 보안 (Security) — Full 적용

### SECURITY-05 (입력 검증/주입 방지)
- 경로/URL/앱ID는 값 타입으로 취급. 코어는 명령 문자열 조합하지 않음.
- 모든 public 입력(ResourceIdentity 구축, 마이그레이션 로딩)에 정규화/검증 함수 적용.
- 예: URL 정규화는 스킴/호스트/경로/쿼리/포트만 추출, 실행 가능한 콘텐츠 미포함.

### SECURITY-13 (안전 역직렬화)
- `serde` 구성: 엄격 스키마, 알 수 없는 필드 거부(`deny_unknown_fields`).
- 깊이/크기 제한(serde_json 설정): 중첩 5단계 이하, 문자열 길이 1MB 이하.
- 미래 버전(알 수 없는 버전 번호): 오류 반환(초기화 아님).

### SECURITY-15 (페일세이프)
- 모든 에러는 기존 상태/메모리와 어긋나지 않는 상태로 유지.
- 저장 실패 시 메모리 상태가 파일과 불일치하지 않도록 설계(U2 store에서 원자적 구현).
- 부분 실패도 지속 진행(상위 오케스트레이션에서 종합).

## 유지보수/테스트 (Maintainability)

### 단위 테스트
- 각 함수(MatchSignature, matches, evaluate, etc.)별 단위 테스트.
- 테스트 데이터: 임의 ResourceIdentity, 다양 시나리오(강한 키 일치/제목 변경/힌트 모호).

### PBT (Partial — 2개 규칙 blocking)
- **PBT-02 라운드트립**: 임의 StoreState `s`에 대해 `load(serialize(s)) == s`. 생성기: WorkBundle(1-10 리소스)·리소스(종류별), shrinking(축소) 지원으로 실패 케이스 최소화.
- **PBT-03 파서 견고성**: 임의 바이트열/부분 손상 입력에서 패닉 없음(Ok(partial) 또는 Err). 생성기: 유효 JSON + 임의 손상(필드 누락/타입 오류/깊이 초과).
- 프레임워크: **proptest** (criterion 벤치마크 통합 가능).

### 코드 품질
- `clippy` lint: warn/deny 규칙 활성화.
- 문서: 각 public type·함수에 doc comments(예제 포함). SECURITY/PBT 제약을 주석으로 명시.
- 결정성: 모든 알고리즘은 같은 입력 → 같은 출력(재현성 보장).

## 가용성/확장/인증 (N/A)
- **확장성**: N/A — 로컬 라이브러리, 수평 확장 개념 없음.
- **가용성/업타임**: N/A — 프로세스 지역 메모리.
- **인증/인가**: N/A — 단일 사용자 로컬 앱.

## 사용성/접근성 (N/A)
- N/A — U7 frontend 소관.

---

## Security(Full)/PBT(Partial) 규칙 매핑

| 확장 | 규칙 ID | 적용 | 이 단계 |
|---|---|---|---|
| Security | SECURITY-05 | Core 입력 검증 | 구현: URL·경로 정규화, 마이그레이션 엄격 스키마 |
| Security | SECURITY-13 | Safe 역직렬화 | 구현: serde deny_unknown + 깊이/크기 제한 |
| Security | SECURITY-15 | Failsafe 오류 | 구현: Result API, 부분 실패 지속 |
| PBT | PBT-02 | Round-trip 검증 | 생성기: WorkBundle, shrinking, 1000회 |
| PBT | PBT-03 | Parser robustness | 생성기: corrupt bytes, 임의 손상, 패닉 금지 |
| PBT | PBT-07 | Generator quality | 구현: meaningful test data, edge cases |
| PBT | PBT-08 | Shrinking/reproducibility | proptest: 기본 shrinking, seed 저장 |
| PBT | PBT-09 | Framework selection | 확정: proptest |

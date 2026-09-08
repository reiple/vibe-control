# 기능 설계 계획 — U2 vc-store (Functional Design Plan)

단계: CONSTRUCTION — U2 vc-store, Functional Design
참조: unit-of-work.md(U2 정의), requirements.md(FR-11: 데이터/영속성, AC-1/12)
성격: 영속성 계층. BundleStore 구현체(JsonBundleStore) + 원자적 저장 + 버전 마이그레이션 + 설정.

---

## A. 설계 결정 질문

### Question 1 — 파일 위치/형식
단일 JSON 파일 저장 경로와 형식은?

A) **OS별 사용자 디렉터리**: macOS `~/Library/Application Support/vibe-control/`, Windows `%APPDATA%/vibe-control/`. 파일명 `bundles.json`. **(권장)**

B) **현재 디렉터리**: 상대 경로

[Answer]: A

### Question 2 — 원자적 저장 전략
파일 쓰기 중 충돌/오류 시 기존 파일 보호?

A) **Temp 파일 + 원자적 교체**: 임시 파일 기록 후 rename(최종 파일). **(권장)**

B) **직접 덮어쓰기**: 간단하나 위험

[Answer]: A

---

## B. 산출물 체크리스트

- [ ] `store-contract.md` — BundleStore 트레이트 계약(인터페이스 정의)
- [ ] `json-schema.md` — JSON 스키마(필드/버전/제약)
- [ ] `atomic-operations.md` — 원자적 저장/로드 알고리즘
- [ ] 설계 vs FR-11/AC-1/12 매핑 확인

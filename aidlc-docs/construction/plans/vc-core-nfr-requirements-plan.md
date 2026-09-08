# NFR 요구사항 계획 — U1 vc-core (NFR Requirements Plan)

단계: CONSTRUCTION — U1 vc-core, NFR Requirements
참조: functional-design(domain-entities/business-logic-model/business-rules), `requirements.md`(NFR), 확장(Security=Full, PBT=Partial)
성격: 순수 코어의 NFR은 **성능(매칭 알고리즘)·신뢰성(무패닉)·보안(안전 역직렬화)·유지보수/테스트(PBT 프레임워크)** 중심. 가용성/확장(서버) 개념은 N/A(로컬 라이브러리).

---

## A. NFR 결정 질문 (NFR Questions)

### Question 1 — PBT 프레임워크 (PBT-09, blocking)
러스트 속성 기반 테스트 프레임워크를 무엇으로 할까요?

A) **proptest** — 축소(shrinking)·재현 시드 강력, 생태계 성숙. **(권장, PBT-08 정합)**

B) **quickcheck** — 경량, 축소 기능 제한적

C) **bolero/arbitrary** — 퍼즈 연계

[Answer]: A

### Question 2 — 에러 처리 방식
코어의 에러 타입을 어떻게 구성할까요?

A) **`thiserror` 기반 타입드 에러 열거** — 분류(Validation/NotFound/Ambiguous/Corrupt/PermissionDenied) 명시, 라이브러리에 적합. **(권장)**

B) `anyhow` 단일 동적 에러

C) 수작업 enum(외부 크레이트 없음)

[Answer]: A

### Question 3 — 매칭 성능 목표/전략
식별·매칭·상태 판정의 성능 전략은?

A) **예상 규모(수십~수백 항목)에서 선형 처리 허용, 측정 후 필요 시 최적화** — 조기 최적화 지양. **(권장)**

B) **MatchSignature 인덱스(HashMap) 선구축** — 대량 대비 상수 시간 조회

[Answer]: A

### Question 4 — 직렬화/결정성
도메인 타입의 직렬화 계약을 어떻게 둘까요?

A) **`serde` derive + 버전 태그, 결정적(안정 필드 순서) 직렬화** — 라운드트립(PBT-02) 보장 용이. 실제 파일 I/O는 U2. **(권장)**

B) 수작업 직렬화

[Answer]: A

---

## B. NFR 산출물 체크리스트 (생성 시 수행)

- [x] `nfr-requirements.md` — 성능/신뢰성/보안/유지보수 요구와 측정 기준
- [x] `tech-stack-decisions.md` — 크레이트/버전/근거(proptest, thiserror, serde 등)
- [x] Security(Full)/PBT(Partial) 적용·N/A 판정 요약

---

## C. 참고 (N/A 판정)
- 확장성(수평/자동 스케일), 가용성(업타임/장애조치), 인증/인가: **N/A** — 로컬 순수 라이브러리.
- 사용성/접근성: U7(프론트) 소관.

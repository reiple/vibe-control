# 기능 설계 계획 — U1 vc-core (Functional Design Plan)

단계: CONSTRUCTION — U1 vc-core, Functional Design
참조: `unit-of-work.md`(U1 정의), `unit-of-work-story-map.md`(U1 = 전 스토리 도메인 기반), `components.md`(C1-C5), `component-methods.md`
성격: **기술 비종속 순수 도메인**. 저장 포맷/OS API는 다루지 않음(어댑터 단위에서). PBT-02/PBT-03의 순수 계약을 여기서 정의.

이 계획서는 (1) 산출물 체크리스트와 (2) 도메인 로직 확정 질문(embedded `[Answer]:`)으로 구성됩니다. 질문은 대화형으로도 제시됩니다.

---

## A. 도메인 결정 질문 (Domain Questions)

### Question 1 — 리소스 식별(ResourceIdentity) 모델
세션 간 안정적이면서 실행 창과 매칭 가능한 식별을 코어에서 어떻게 표현할까요?

A) **복합 식별**: 종류별 안정 서술자(예: 앱 식별자 + 창 역할/제목 시그니처 / 브라우저 + 전체 URL + 탭 구분자 / 폴더 전체 경로 / 세션 도구ID+파일ID)를 저장하고, 휘발성 핸들(PID/HWND)은 **비영속 힌트**로만. 코어가 서술자로 `MatchSignature`를 계산. **(권장)**

B) **불투명 ID**: 어댑터가 제공하는 단일 문자열 ID만 저장, 코어는 해석하지 않음

[Answer]: A

### Question 2 — 창 매칭 허용도(제목은 변함)
저장 창과 실행 창의 동일성 판정 기준은?

A) **계층 매칭**: 강한 키(앱 + 안정 속성)는 필수, 제목은 보조 tie-breaker로만 사용, 제목 기반 추측 금지. **(권장)**

B) **엄격 일치**: 모든 속성 완전 일치(제목 변경 시 실패 가능)

C) **유사도 스코어링**: 퍼지 매칭

[Answer]: A

### Question 3 — 상태(Status) 판정 우선순위
`ResourceStatus` 도출 규칙은?

A) **우선순위**: PermissionRequired > (실행 매칭됨 → Active) > (재실행 정보 있음 → Inactive) > Unknown. **(권장)**

B) **단순**: 매칭되면 Active, 아니면 Inactive

[Answer]: A

### Question 4 — 코딩 세션 답변 완료여부 의미(코어 열거)
세션의 완료여부를 코어에서 어떻게 모델링할까요?

A) **3-상태**: Waiting(사용자 입력 대기) / NotWaiting / Unknown. 코어는 provider가 제공한 분류를 권위로 삼고 추측하지 않음. **(권장)**

B) **2-상태**: waiting / not-waiting

[Answer]: A

---

## B. 기능 설계 산출물 체크리스트 (생성 시 수행)

- [x] `domain-entities.md` — 엔티티/값 타입·관계·불변식
- [x] `business-logic-model.md` — 식별/매칭·복원 계획·상태 판정·마이그레이션 순수 알고리즘
- [x] `business-rules.md` — 결정/검증 규칙, 에러 분류, PBT 계약(라운드트립/파서)
- [x] FR/AC 추적 및 Security(Full)/PBT(Partial) 순수 경계 반영 확인

---

## C. 참고 (경계)
- 프론트엔드 컴포넌트 없음(U1은 순수 코어) → `frontend-components.md` N/A.
- 저장 직렬화의 구체 포맷은 U2에서, OS 식별 상세는 U3/U4에서, 세션 파일 포맷은 U5에서 확정. U1은 **계약과 규칙**만 정의.

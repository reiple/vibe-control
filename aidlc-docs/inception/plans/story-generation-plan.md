# 스토리 생성 계획 (Story Generation Plan)

역할: Product Owner
단계: INCEPTION — User Stories, Part 1 (Planning)
참조: `aidlc-docs/inception/requirements/requirements.md` (FR-1..FR-12, NFR, AC-1..AC-19), `user-stories-assessment.md`

이 계획서는 (1) 스토리 생성 방법론 체크리스트와 (2) 방법론을 확정하기 위한 질문(embedded `[Answer]:`)으로 구성됩니다. 질문에 답해 주시면(대화형으로도 가능) 답변을 아래에 기록하고, 모호성 분석 후 계획 승인을 요청드립니다.

---

## A. 방법론 결정 질문 (Planning Questions)

### Question 1 — 사용자 페르소나 구성
어떤 사용자 페르소나로 스토리를 구성할까요?

A) 3종: **개발자**, **디자이너**, **AI 협업 개발자**(코딩 에이전트 세션 사용) — 니즈를 가장 명시적으로 구분

B) 2종: **파워 유저**(개발자·디자이너 통합) + **AI 협업 개발자**

C) 1종: **멀티프로젝트 파워 유저** 단일 페르소나

X) Other (please describe after [Answer]: tag below)

[Answer]: A  (3종: 개발자, 디자이너, AI 협업 개발자)

### Question 2 — 스토리 분해(breakdown) 접근
스토리를 어떤 기준으로 나눌까요?

A) **기능 기반**(FR 그룹별: 작업 묶음/실행 중 표시/등록/활성화/전체 실행/편집/상태/화면/브라우저/OS/데이터/코딩 세션) — 요구사항과 직접 매핑

B) **사용자 여정 기반**(핵심 흐름: 묶음 만들기 → 등록 → 전환/복원 → 세션 확인)

C) **하이브리드**(기능 그룹을 에픽으로, 핵심 여정을 관통 스토리로 보강)

X) Other (please describe after [Answer]: tag below)

[Answer]: C  (하이브리드)

### Question 3 — 스토리 세분화(granularity)
스토리 계층 구조를 어떻게 할까요?

A) **에픽 + 하위 스토리**(2계층) — 그룹별 에픽 아래 세부 스토리

B) **단일 계층**(플랫 스토리 목록)

X) Other (please describe after [Answer]: tag below)

[Answer]: A  (에픽 + 하위 스토리)

### Question 4 — 수용 기준(Acceptance Criteria) 형식
각 스토리의 수용 기준을 어떤 형식으로 작성할까요?

A) **Given/When/Then**(Gherkin 스타일) — 테스트·PBT 연계에 유리

B) **체크리스트형**(불릿 조건 목록)

C) **서술형**(문장 서술)

X) Other (please describe after [Answer]: tag below)

[Answer]: A  (Given/When/Then)

---

## B. 스토리 생성 실행 체크리스트 (Part 2에서 수행)

- [x] 확정된 페르소나로 `personas.md` 생성(아키타입·특성·동기·주요 니즈)
- [x] 확정된 분해 접근으로 `stories.md` 생성
- [x] 각 스토리를 INVEST 기준(Independent, Negotiable, Valuable, Estimable, Small, Testable)으로 작성
- [x] 각 스토리에 확정된 형식의 수용 기준 포함
- [x] 각 스토리를 요구사항 ID(FR-*/AC-*)와 매핑(추적성)
- [x] 각 스토리를 관련 페르소나와 매핑
- [x] Security(Full)/PBT(Partial) 확장 제약이 관련 스토리의 수용 기준에 반영되었는지 확인
- [x] 코딩 세션 기능(FR-12) 및 OS별 동작(FR-10)에 대한 스토리 포함
- [x] 모든 FR 그룹이 최소 1개 스토리로 커버되는지 확인(커버리지 점검)

---

## C. 방법론 참고 (분해 접근 트레이드오프)

- **기능 기반**: 요구사항 추적성 최고, 구현 범위 명확. 여정 흐름은 별도 확인 필요.
- **여정 기반**: 사용자 경험 흐름이 자연스러움. 기능 커버리지 누락 주의.
- **하이브리드**: 커버리지+흐름 모두 확보하나 문서량 증가.

기본 권장: **기능 기반(에픽+하위 스토리, Given/When/Then)** — 요구사항/인수 조건과의 추적성과 테스트 연계가 가장 강함. (최종 결정은 위 답변을 따름)

# 애플리케이션 설계 계획 (Application Design Plan)

단계: INCEPTION — Application Design
참조: `requirements.md`(FR-1..FR-12, NFR, AC-1..AC-19), `stories.md`(EPIC-1..EPIC-12, JS-1..JS-3), `personas.md`, `execution-plan.md`
확장: Security=Full, PBT=Partial(테스트를 위한 어댑터 모의 가능성 고려)

이 계획서는 (1) 산출물 체크리스트와 (2) 설계 확정을 위한 질문(embedded `[Answer]:`)으로 구성됩니다. 질문은 대화형으로도 제시되며, 답변을 아래에 기록한 뒤 모호성 분석 후 설계 산출물을 생성합니다.

---

## A. 설계 결정 질문 (Design Questions)

### Question 1 — 아키텍처 스타일 / 컴포넌트 구성
러스트 코어의 구조를 어떤 스타일로 구성할까요?

A) **헥사고날(포트&어댑터)** — 순수 도메인 코어 + 트레이트(포트)로 OS/브라우저/세션 접근을 추상화하고 플랫폼별 어댑터로 구현. 테스트 계획(모의 어댑터 통합 테스트)과 가장 잘 맞음. **(권장)**

B) **단순 레이어드** — UI → 커맨드 핸들러 → 서비스 → OS 호출(엄격한 포트 없이)

C) **기능 모듈 중심** — 기능별(묶음/리소스/세션)로 모듈 그룹화

[Answer]: A

### Question 2 — 상태 소유(프론트↔코어 경계)
앱 상태(작업 묶음·리소스·설정)의 단일 소스는 어디에 둘까요?

A) **러스트 코어가 단일 소스** — 프론트엔드는 Tauri 커맨드/이벤트로 조회·구독하는 얇은 뷰. 저장·마이그레이션·상태 판정 모두 코어. **(권장)**

B) **프론트엔드가 상태 보유** — 러스트는 무상태 커맨드 실행기

C) **분리 소유** — 영속 데이터는 코어, UI 상태는 프론트

[Answer]: A

### Question 3 — OS 기능 추상화 단위
OS별 기능을 하나의 큰 트레이트로 둘까요, 기능별로 나눌까요?

A) **기능별 분리 트레이트** — 예: `WindowEnumerator`(열거), `WindowActivator`(활성화), `BrowserTabReader`(탭), `CodingSessionReader`(세션). 단일 책임 + 개별 모의에 유리. **(권장)**

B) **플랫폼 단일 트레이트** — `PlatformAdapter` 하나에 모든 기능

[Answer]: A

### Question 4 — 코딩 세션(FR-12) 확장 구조
코딩 에이전트 세션 지원의 확장성을 어떻게 설계할까요?

A) **도구별 어댑터 + 레지스트리** — `CodingSessionProvider` 트레이트, Claude Code 구현 우선, 신규 도구는 구현체 추가·등록만으로 확장(기존 코드 불변). **(권장)**

B) **단일 파서 + 도구 타입 분기** — 하나의 파서에서 도구 종류로 분기

[Answer]: A

---

## B. 설계 산출물 체크리스트 (Part 2에서 수행)

- [x] `components.md` — 컴포넌트 정의·책임·인터페이스
- [x] `component-methods.md` — 메서드 시그니처·입출력 타입(상세 비즈니스 규칙은 Functional Design)
- [x] `services.md` — 서비스 정의·오케스트레이션 패턴
- [x] `component-dependency.md` — 의존 관계 매트릭스·통신 패턴·데이터 흐름
- [x] `application-design.md` — 위 문서 통합본
- [x] 설계 완전성·일관성 검증(FR/AC 커버리지, Security(Full)/PBT(Partial) 반영)

---

## C. 참고 (설계 원칙)
- 테스트 전략(도메인 단위 + 모의 어댑터 통합 + 실제 OS E2E)을 지원하도록 경계를 명확히.
- Security(Full): 입력 검증(SECURITY-05), 안전 역직렬화(SECURITY-13), 페일세이프(SECURITY-15)를 관련 컴포넌트 책임에 반영.
- PBT(Partial): 저장소 라운드트립(PBT-02), 세션 파서 견고성(PBT-03)이 검증 가능한 경계로 분리.

# 작업 단위 계획 (Unit of Work Plan)

단계: INCEPTION — Units Generation, Part 1 (Planning)
참조: `application-design.md`(컴포넌트/서비스/포트), `stories.md`(EPIC/US/JS), `requirements.md`, `execution-plan.md`
맥락: 단일 배포 산출물(Tauri 데스크톱 앱). "Unit of Work" = 단일 앱 내 논리적 모듈(독립 배포 서비스 아님). 개발/설계 편의를 위한 그룹.

이 계획서는 (1) 분해 산출물 체크리스트와 (2) 분해 확정을 위한 질문(embedded `[Answer]:`)으로 구성됩니다. 질문은 대화형으로도 제시됩니다.

---

## A. 분해 결정 질문 (Decomposition Questions)

### Question 1 — 단위 분해 세분화
시스템을 어떤 단위로 나눌까요?

A) **계층/능력별 7단위**: 도메인 코어 · 영속성 · macOS 어댑터 · Windows 어댑터 · 코딩 세션 · 서비스+브리지 · 프론트엔드. 헥사고날 경계와 정합, 병렬/순차 명확. **(권장)**

B) **거친 3~4단위**: 코어+영속성 / 플랫폼 어댑터 / 서비스+브리지 / 프론트엔드

C) **단일 단위**: 전체를 하나로

[Answer]: A

### Question 2 — 코드 조직(Greenfield)
러스트 코드 구조를 어떻게 조직할까요?

A) **Cargo 워크스페이스 + 크레이트 분리** — 단위별 크레이트(core, store, os-macos, os-windows, sessions, app/bridge) + `frontend/` 웹 디렉터리. 경계·테스트·병렬 개발에 유리. **(권장)**

B) **단일 크레이트 + 모듈** — 하나의 크레이트 안에 모듈로 구분 + `frontend/`

[Answer]: A

### Question 3 — 플랫폼 어댑터 단위 구성
macOS/Windows 어댑터를 별도 단위로 둘까요?

A) **별도 단위(cfg 게이트)** — macOS/Windows를 독립 단위로 분리해 병렬 개발, 플랫폼별 빌드 게이트. **(권장)**

B) **단일 플랫폼 단위** — 한 단위 안에서 cfg 분기

[Answer]: A

### Question 4 — 구현/설계 순서(Per-Unit Loop 순서)
CONSTRUCTION의 단위별 루프를 어떤 순서로 진행할까요?

A) **기반 우선** — 도메인 코어 → 영속성 → (플랫폼 어댑터: macOS, Windows) → 코딩 세션 → 서비스+브리지 → 프론트엔드. 테스트 가능한 코어부터. **(권장)**

B) **수직 슬라이스 우선** — 대표 기능 1개를 end-to-end로 먼저 관통

[Answer]: A

---

## B. 분해 산출물 체크리스트 (Part 2에서 수행)

- [x] `unit-of-work.md` — 단위 정의·책임·코드 조직 전략(Greenfield)
- [x] `unit-of-work-dependency.md` — 단위 간 의존 매트릭스
- [x] `unit-of-work-story-map.md` — 스토리→단위 매핑(모든 EPIC/US/JS 배정)
- [x] 단위 경계·의존 검증
- [x] 모든 스토리가 단위에 배정되었는지 확인

---

## C. 참고 (분해 원칙)
- 헥사고날 경계를 단위 경계로 사용 → 서비스는 포트 트레이트에만 의존, 어댑터 단위는 교체·모의 가능.
- 배포 모델: 단일 Tauri 앱(플랫폼별 빌드). 단위는 논리적 모듈/크레이트.
- 테스트 정합: 도메인 단위(순수), 어댑터 모의 통합, 실제 OS E2E.

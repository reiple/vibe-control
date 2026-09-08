# 정합화 범위 결정 질문 (Reconciliation Scope Questions)

작성: 2026-09-08 · 참조: `drift-analysis.md`
아래 질문에 `[Answer]:` 뒤에 문자 선택지를 적어 주세요. 해당하는 항목이 없으면 마지막 "Other"를 고르고 설명을 덧붙여 주세요.
**코드는 아직 수정하지 않았습니다.** 답변 이후 재실행 계획을 세웁니다.

## Question 1
발견된 45건의 드리프트 중 이번에 처리할 **범위**를 선택해 주세요. (상세: `drift-analysis.md` §5)

A) 옵션 B — 부팅 스플래시 정식화 + 차단성 NFR 이행(SECURITY-13 강화·CSP·PBT 1000회·시드/CI) + 문서 정합화 (권장)

B) 옵션 A — 문서 정합화만. 코드 무수정. 미충족 NFR 8건은 정식 면제(waiver)로 기록

C) 옵션 C — 옵션 B + 기능 공백 해소(상태 표시·이름변경·리소스 제거/편집·대화 뷰어·레이아웃 영속). 요구사항 재확정 포함

D) 옵션 D — 옵션 C + 아키텍처 복원(포트 트레이트·서비스 계층·이벤트화·도메인 배선) + Windows 탭/트레이 + macOS 권한 체커

X) Other (please describe after [Answer]: tag below)

[Answer]: A) 옵션 B — 부팅 스플래시 정식화 + 차단성 NFR 이행(SECURITY-13 강화·CSP·PBT 1000회·시드/CI) + 문서 정합화 (권장)

## Question 2
**부팅 스플래시(D-01)** — 문서에 전혀 없는 신규 기능입니다. 어떻게 처리할까요?

A) 정식 요구사항으로 승격 — Requirements Analysis에 신규 FR + AC 추가 → 스토리·U7 설계·Build&Test까지 반영

B) `known-deviations.md`에 [수용] 이탈로만 기록하고 요구사항은 개정하지 않음

C) 요구사항에 없는 기능이므로 제거 대상으로 분류(코드 롤백 후보)

X) Other (please describe after [Answer]: tag below)

[Answer]: A) 정식 요구사항으로 승격 — Requirements Analysis에 신규 FR + AC 추가 → 스토리·U7 설계·Build&Test까지 반영

## Question 3
**차단성 확장 규칙 미이행 8건(D-50~D-57)** — Security Baseline은 Full(전 규칙 차단), PBT는 Partial(PBT-02/03/07/08/09 차단)로 승인되어 있습니다. AI-DLC 규칙상 활성 확장 규칙의 미준수는 차단 사유입니다.

A) 전부 이행 — `deny_unknown_fields`+깊이/크기 제한, CSP 적용, proptest 1000회, 시드/CI, criterion 벤치마크, 커버리지 도구, fast-check, 의존성 감사

B) 보안 관련(D-50 SECURITY-13, D-51 CSP, D-57 공급망)만 이행하고 테스트 정책(D-52~D-56)은 면제 기록

C) 전부 면제(waiver)로 기록 — 개인 프로젝트/PoC 성격으로 확장 모드를 하향 조정

X) Other (please describe after [Answer]: tag below)

[Answer]: C) 전부 면제(waiver)로 기록 — 개인 프로젝트/PoC 성격으로 확장 모드를 하향 조정

## Question 4
**미충족 요구사항 13건(D-30~D-42)** — 기준선 요구사항이 코드에 없는 항목들입니다. 기본 처리 방침을 선택해 주세요.

A) 요구사항을 기준선으로 유지하고 **미구현 백로그**로 관리(문서 개정 없음, 우선순위만 부여)

B) 실제 제품 방향에 맞춰 요구사항을 **정식 개정**(축소/삭제)하고 개정 이력을 남김

C) 항목별로 개별 판단 — 다음 턴에 13건 각각에 대한 결정 질문지를 별도 작성

X) Other (please describe after [Answer]: tag below)

[Answer]: B) 실제 제품 방향에 맞춰 요구사항을 **정식 개정**(축소/삭제)하고 개정 이력을 남김

## Question 5
**D-26 — "완료로 기록되었으나 산출물이 없는 CONSTRUCTION 단계"** (U5·U7 디렉터리 자체 부재, U3/U4/U6 부분, 코드 생성 계획은 vc-core 1개뿐). 기록 정합성 처리 방침은?

A) `aidlc-state.md` 체크박스를 "(auto — 문서 미생성)"으로 정정하여 실제 상태를 반영

B) 누락된 Functional Design / NFR / 코드 생성 계획 산출물을 현행 코드 기준으로 **소급 생성**

C) 이번 정합화 범위에서 제외하고 별도 작업으로 분리

X) Other (please describe after [Answer]: tag below)

[Answer]: B) 누락된 Functional Design / NFR / 코드 생성 계획 산출물을 현행 코드 기준으로 **소급 생성**

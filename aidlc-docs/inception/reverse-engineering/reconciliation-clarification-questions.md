# 정합화 범위 — 모순 해소 질문 (Clarification)

작성: 2026-09-08 · 참조: `reconciliation-questions.md`, `drift-analysis.md`

답변에서 **직접적인 모순 1건**이 발견되어 확인이 필요합니다. 이 항목을 제외한 나머지(Q2 스플래시 승격 / Q4 요구사항 개정 / Q5 산출물 소급 생성 / 문서 정정)는 **먼저 진행 중**입니다.

## 모순: 차단성 확장 규칙 8건을 "이행"과 "면제" 양쪽으로 답하셨습니다

- **Q1 = A (옵션 B)** — 선택지 문구에 **"차단성 NFR 이행(SECURITY-13 강화·CSP·PBT 1000회·시드/CI)"** 이 포함되어 있습니다. 즉 D-50·D-51·D-52·D-53을 **구현**한다는 뜻입니다.
- **Q3 = C** — **"전부 면제(waiver)로 기록 — 확장 모드 하향 조정"**. 즉 D-50~D-57을 **구현하지 않고** Security=Full / PBT=Partial 설정을 낮춘다는 뜻입니다.

같은 4건(D-50~D-53)이 동시에 "구현 대상"이자 "정식 면제 대상"일 수는 없습니다.

참고로 각 항목의 실제 작업량은 다음과 같습니다.

| ID | 내용 | 예상 작업량 |
|---|---|---|
| D-50 | serde `deny_unknown_fields` + JSON 깊이/크기 제한 (SECURITY-13) | 작음 — 모델 derive 속성 + 로드 시 크기·깊이 가드 |
| D-51 | Tauri `csp` 설정 (SECURITY-04) | 매우 작음 — `tauri.conf.json` 한 줄 |
| D-52 | proptest 실행 횟수 256 → 1000 | 매우 작음 — `ProptestConfig` 한 줄 |
| D-53 | 시드 보존 + CI 파이프라인 | 큼 — `.github/workflows` 신설 |
| D-54 | criterion 벤치마크로 성능 목표 측정 | 큼 — 벤치 하네스 신설 |
| D-55 | 커버리지 도구 + vc-app/vc-os-windows/frontend 테스트 작성 | 매우 큼 |
| D-56 | 프론트엔드 `fast-check` 도입 | 큼 — 테스트 러너부터 신설 |
| D-57 | 의존성 취약점 스캔(+ 미사용 `sha2` 제거) | 중간 — 스캔은 CI 의존, `sha2` 제거는 작음 |

### Clarification Question 1
D-50~D-57을 최종적으로 어떻게 처리할까요?

A) **분할 처리 (권장)** — 값싸고 실효가 큰 보안 3건만 이행하고(**D-50** 안전 역직렬화, **D-51** CSP, **D-57**의 `sha2` 제거), 테스트 정책 5건(**D-52~D-56**)은 정식 면제로 기록 + 확장 모드를 그에 맞게 하향. Q1의 보안 취지와 Q3의 "PoC 성격" 판단을 모두 만족합니다

B) **Q3(=C)가 최종** — 8건 전부 면제. 코드 무수정, `aidlc-state.md`의 Extension Configuration을 Security=Reduced / PBT=Off로 하향하고 면제 사유를 기록

C) **Q1(=A)이 최종** — D-50·D-51·D-52·D-53을 전부 이행(CI 파이프라인 신설 포함), D-54~D-57만 면제

D) **전부 이행** — D-50~D-57 8건 모두. 커버리지·프론트 테스트·벤치마크까지 포함(작업량 매우 큼)

X) Other (please describe after [Answer]: tag below)

[Answer]: B) **Q3(=C)가 최종** — D-50~D-57 8건 전부 면제. 코드 무수정. *(사용자 확정 2026-09-08: "Q3의 답과 같이 D-50~D-57 전부 면제로 결정")*

### Clarification Question 2
면제(waiver)로 기록되는 항목에 대해 `aidlc-state.md`의 **확장 설정을 실제로 하향 조정**할까요? (하향하면 이후 단계에서 해당 규칙이 차단 사유로 재검출되지 않습니다)

A) 예 — Security Baseline은 유지하되 **PBT 확장을 Off**로 내리고, 면제 사유를 표에 기록

B) 예 — Security도 **Full → Reduced**(핵심 규칙만 차단)로 함께 하향

C) 아니오 — 설정은 Full/Partial 그대로 두고, 개별 항목만 "승인된 면제"로 표시(이후 감사에서 계속 보이게)

X) Other (please describe after [Answer]: tag below)

[Answer]: A) — **미응답 항목. AI 판단으로 A 채택** (근거는 아래 참조)

> **AI 판단 근거 (CQ2 미응답분)**: Security Baseline을 통째로 Reduced로 내리면(B) 현재 **충족 중인** SECURITY-05(주입 방지)·12(자격증명)·15(페일세이프)·09·11까지 함께 면제되어, 앞으로의 작업에서 그 규칙들이 검증 대상에서 빠진다. 면제는 **문제가 된 규칙에만 좁게** 거는 것이 원칙이므로 Security Baseline은 **Full 유지 + 명시적 예외 2건(SECURITY-04 CSP, SECURITY-13 강화 조항)** 으로 처리한다. 반면 PBT는 차단 규칙 5개 중 4개(PBT-02/03 실행횟수, PBT-08, PBT-09)가 면제 대상이므로 확장 자체를 **Advisory(권고)** 로 하향하는 것이 정직한 표기다. (C를 택하면 면제한 항목이 매 감사마다 차단 사유로 재검출되어 waiver의 목적이 사라진다.)

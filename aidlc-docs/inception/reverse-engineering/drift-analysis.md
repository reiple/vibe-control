# 정합성 분석 — 승인된 AI-DLC 산출물 vs 현행 소스코드
# (Reconciliation / Drift & Impact Analysis)

단계: INCEPTION — Reverse Engineering 재실행에 이은 **드리프트 분석**
작성: 2026-09-08 · 기준 커밋 `d1e0f2f` (main, clean)
기준선(baseline): `aidlc-docs/` 아래 **승인된 산출물** — 요구사항 / 사용자 스토리 / 애플리케이션 설계 / 작업 단위 / 기능 설계 / NFR 요구사항·설계 / 인프라 설계 / 코드 생성 계획 / Build&Test 지침
제약: **코드 미수정** · **문서를 코드에 맞춰 자동 개정하지 않음** — 본 문서는 차이의 *기록과 영향 분석*이며, 개정 여부는 사용자 승인 사항이다.

---

## 0. 분석 방법과 한계

- 정적 분석(파일 열람·grep·git 이력)으로 25개 소스 파일 전량 대조.
- **한계**: 본 세션 환경(WSL/Linux)에 `cargo`가 없어 `cargo build/test/clippy`를 **재실행하지 못했다**. "빌드/테스트 통과"는 `aidlc-state.md`·`audit.md`에 기록된 실 Windows 실행 결과의 인용이며, 본 분석이 재확인한 사실이 아니다.
- 기존 `known-deviations.md`(2026-09-08)의 항목은 **재검증**하여 유효/무효를 표시했다.

---

## 1. 판정 요약

| 분류 | 신규 발견 | 기존 기록 유지 | 합계 |
|---|---:|---:|---:|
| ① 의도된 신규 기능 (intended new functionality) | 1 | 2 | 3 |
| ② 구현 결함 (implementation defect) | 3 | 2 | 5 |
| ③ 아키텍처/설계 위반 (architectural violation) | 2 | 5 | 7 |
| ④ 요구사항 위반 (requirement violation) | 8 | 5 | 13 |
| ⑤ NFR 위반 (NFR violation) | 8 | 0 | 8 |
| ⑥ 문서 전용 드리프트 (documentation-only) | 5 | 4 | 9 |
| **합계** | **27** | **18** | **45** |

**핵심 결론 3가지**

1. **직전 정합화(2026-09-08) 이후 코드에 들어온 변경은 1건**뿐이다 — 커밋 `7ab755a` **부팅 스플래시**. 이것이 문서에 **전혀 없는** 순수 vibe-coding 산출물이다(요구사항·스토리·설계·이탈기록 어디에도 없음). 커밋 메시지가 스스로 밝힌 **고DPI 렌더링 결함**도 미기록.
2. **직전 정합화가 놓친 영역이 있다** — `known-deviations.md`는 A~G 섹션에서 아키텍처·어댑터·영속·도메인·프론트 이탈은 잘 잡았으나, **NFR/보안 확장 규칙의 미이행(SECURITY-13 강화, SECURITY-04 CSP, PBT 실행 횟수·CI, 성능 측정 기준, 커버리지 목표)** 과 **미구현 요구사항 다수(FR-1.1 이름변경, FR-3.2/3.3, FR-6 전체, FR-7.1~7.3 상태 표시)** 는 다루지 않았다. 본 분석에서 신규로 잡았다.
3. **CONSTRUCTION 산출물 자체가 대부분 부재**하다. `aidlc-state.md`는 U3~U7의 Functional Design / NFR Requirements / NFR Design을 "(auto)"로 완료 표시했으나, `aidlc-docs/construction/` 실물은 vc-core(전체)·vc-store(대부분)·vc-os-macos/windows(각 1개)·vc-app(claude-console 1개)뿐이며 **U5·U7 디렉터리는 존재하지 않고, 코드 생성 계획은 vc-core 1개뿐**이다. 즉 "설계 대비 코드 이탈"의 상당 부분은 **애초에 대조할 설계 문서가 없는 상태**다.

---

## 2. 발견 목록

표기: **최초 재검토 단계** = 이 발견을 해소하려면 가장 먼저 되돌아가야 할 AI-DLC 단계. **하위 재생성** = 그 단계를 다시 실행할 때 함께 재생성되어야 하는 이후 단계.

### ① 의도된 신규 기능 (Intended New Functionality)

| ID | 발견 | 근거 | 최초 재검토 단계 | 하위 재생성 |
|---|---|---|---|---|
| **D-01** | **부팅 스플래시(신규·미문서)** — 전체 화면 오버레이가 최초 로드(묶음·Claude 상태·첫 실행앱 스캔) 완료까지 **모든 입력을 차단**. 최소 650ms 표시, 8초 하드캡, 400ms 페이드아웃 후 언마운트. `index.html`은 프리마운트 배경색만 인라인. 요구사항/스토리/설계/이탈기록 **어디에도 없음** | 커밋 `7ab755a`; `App.tsx:133-162`(Splash), `:234-323`(부트 시퀀스), `:520`; `styles.css`(+138줄); `index.html:7-14` | **Requirements Analysis** (신규 FR: 부팅/로딩 UX + 입력 차단 정책) | User Stories(US-8.x) → Application Design(U7 책임) → Units(U7만) → Functional Design(U7) → Code Generation 계획(U7) → Build&Test(E2E 항목) |
| D-02 | **앱 내 Claude 프롬프트 콘솔(Bedrock)** — 기존 기록됨(`#F1`). 재검증 결과 **유효**하며 [수용] 판정 유지 | `claude.rs`, `lib.rs:509-565`, 콘솔 UI | (해소됨 — 문서화 완료) | 없음 (단, D-40/D-41 참조) |
| D-03 | **창 단위 펼침/접기 UX(FR-2.8/AC-20)** — 기존 기록됨(`#G1~G3`). 재검증 결과 **구현 완료·문서 일치** | `lib.rs:302-370`, `App.tsx:541-599`, `vc-os-windows:305-414` | (해소됨) | 없음 (단, D-30 참조) |

### ② 구현 결함 (Implementation Defect)

| ID | 발견 | 근거 | 최초 재검토 단계 | 하위 재생성 |
|---|---|---|---|---|
| **D-10** | **스플래시 고DPI 렌더 결함** — 고DPI Windows에서 WebView2가 초기 프레임을 모니터 배율로 래스터화한 뒤 창 DPI에 동기화하여 **스플래시 카드가 잠시 확대되어 보임**. 커밋 메시지가 결함으로 명시하고 "Tauri window-nudge 수정은 보류"라고 기록했으나 **`known-deviations.md`·`aidlc-state.md`에 미등록** | 커밋 `7ab755a` 메시지 "Known caveat"; `index.html:7-14` 주석 | **Code Generation (U7)** — 결함 등록 후 수정/보류 결정 | Build&Test(고DPI 회귀 항목 추가) |
| **D-11** | **죽은 공개 API** — G1 재구현 후 `WinWindowEnumerator::list_running()`이 어디서도 호출되지 않음(macOS 동명 함수는 탭 리더가 사용) | `vc-os-windows/src/lib.rs:307`; 호출처 grep 0건 | **Code Generation (U4)** | 없음 |
| **D-12** | **`reqwest` 주석 오류** — "Claude (Anthropic Messages API) client"라 표기하나 실제 대상은 Bedrock. 기존 문서가 후속 과제로 지목했으나 미수정 | `vc-app/Cargo.toml`; `claude-console/design.md §6` | **Code Generation (U6)** | 없음 |
| D-13 | 저장 실패 시 `.tmp` 파일 미정리(누수) — 기존 `#C1`, **여전히 유효** | `vc-store/src/lib.rs:80-85` | Code Generation (U2) | Build&Test |
| D-14 | 도메인 중복 방지 불변식 미강제 — 기존 `#D1`, **여전히 유효**(`add_resource`가 검사 없이 push) | `bundle.rs:82-85`; `is_duplicate` grep 0건 | Functional Design (U1, BR-2) | NFR Design(U1) → Code Gen(U1,U6) → Build&Test |

### ③ 아키텍처/설계 위반 (Architectural / Design Violation)

| ID | 발견 | 근거 | 최초 재검토 단계 | 하위 재생성 |
|---|---|---|---|---|
| D-20 | **포트 트레이트 P1–P8 부재** — vc-core에 `trait` 0개. P4는 vc-sessions, P5는 vc-store에만 존재. 어댑터는 core 트레이트를 구현하지 않음. 기존 `#A1`, **여전히 유효** | `vc-core/src/**`(trait 0건), `vc-sessions:17`, `vc-store:5` | **Application Design** | Units Generation → Functional Design(U1,U3,U4,U5) → NFR Design → Code Generation(전 단위) → Build&Test |
| D-21 | **서비스 S1–S7 / TauriCommandBridge / RefreshScheduler 부재** — `AppState` + 18개 커맨드 + 자유함수로 평면화, 어댑터를 `cfg`로 **구체 타입 직접 호출**(DI 아님) | `vc-app/src/lib.rs:23,566-745,760-783` | **Application Design** | 상동 (특히 U6 전량) |
| D-22 | **이벤트 기반 얇은 뷰 미구현** — 백엔드 `emit` 0건, 프론트 `listen` 0건. 1초 `setInterval` 폴링 | `App.tsx:349-353` | **Application Design** | Functional Design(U6,U7) → Code Gen(U6,U7) → Build&Test |
| D-23 | **도메인 코어 미배선** — vc-app이 `matching`/`restore`/`evaluate`/`normalize`를 **import조차 하지 않음**. 33개 테스트가 통과하는 도메인 로직이 런타임 동작에 기여하지 않음 | `vc-app/src/lib.rs:13` import 목록 | **Application Design** | Functional Design(U1,U6) → Code Gen(U1,U6) → Build&Test |
| D-24 | **모킹 통합 테스트 불가 구조** — 승인된 테스트 범위는 "OS 어댑터 모킹 통합 테스트"인데, 어댑터가 트레이트가 아니라 컴파일타임 cfg 디스패치라 **모킹 지점이 존재하지 않음**. `tests/` 디렉터리 부재는 결과이지 원인이 아님 | `integration-test-instructions.md` vs `vc-app/src/lib.rs` cfg 블록 | **Application Design** (D-20의 종속 결과) | NFR Design → Code Gen → **Build&Test(통합 테스트 지침 전면 재작성)** |
| **D-25** | **Infrastructure Design SKIP 결정이 더 이상 유효하지 않음** — Workflow Planning은 "로컬 데스크톱 앱 — 클라우드 인프라 없음"을 근거로 인프라 설계를 건너뛰었다. 그러나 Bedrock 콘솔이 **외부 클라우드 런타임 의존성**(엔드포인트·리전·자격증명 해석 순서·타임아웃·요금)을 도입했고, 이에 대한 인프라 산출물이 **하나도 없다** | `execution-plan.md`(SKIP 사유) vs `claude.rs:63-149` | **Workflow Planning** (SKIP 결정 재평가) | Infrastructure Design(U6) → NFR Design(U6) → Code Gen(U6) → Build&Test |
| **D-26** | **CONSTRUCTION 산출물 대량 부재** — `aidlc-state.md`는 U3~U7의 Functional Design/NFR Requirements/NFR Design을 "(auto)"로 완료 표시했으나 실물 없음: **U5 vc-sessions·U7 frontend 디렉터리 자체가 없고**, U6는 claude-console 1건뿐, U3/U4는 functional-design 1건씩, 코드 생성 계획은 **vc-core 1개**뿐. 즉 승인 게이트를 통과한 것으로 기록된 단계의 산출물이 존재하지 않는다 | `aidlc-state.md` 체크박스 vs `aidlc-docs/construction/` 실제 트리 | **Functional Design (U3·U4·U5·U6·U7)** | 각 단위 NFR Requirements → NFR Design → Code Generation 계획 → Build&Test |

### ④ 요구사항 위반 (Requirement Violation)

| ID | 요구사항 | 현행 코드 | 근거 | 최초 재검토 단계 | 하위 재생성 |
|---|---|---|---|---|---|
| **D-30** | **FR-7.1/7.2/7.3 · US-7.1** 저장된 항목의 활성/비활성/권한필요 상태를 아이콘 왼쪽 작은 원으로 표시, 비활성은 흐리게 | **미구현** — 프론트 `Resource` 타입에 `status` 필드 자체가 없고, 상태 점·흐림 처리 없음. 백엔드도 `evaluate`를 호출하지 않음(D-23) | `frontend/src/types.ts:24-30`, `App.tsx:664-700`, `vc-app/src/lib.rs`(evaluate 미호출) | **Requirements Analysis** (유지/축소 판단) → 유지 시 **Application Design** | User Stories → App Design(S5) → Functional Design(U1,U6,U7) → NFR Design → Code Gen(U1,U6,U7) → Build&Test |
| **D-31** | **FR-6.1/6.2/6.3 · US-6.1~6.3** 항목 표시 이름 변경 / 재실행 주소·경로 편집 / 편집해도 식별정보 불변 | **전부 미구현** — 편집 UI 없음, 해당 Tauri 커맨드 없음 | `App.tsx`(편집 UI 0건), `lib.rs`(update_resource 커맨드 없음) | **Requirements Analysis** | 상동 (U6·U7 중심) |
| **D-32** | **FR-1.1** 작업 묶음 **이름 변경** | **미구현** — 생성/삭제만 존재(`create_bundle`/`delete_bundle`), rename 없음 | `lib.rs:427-455`, `App.tsx:638-673` | **Requirements Analysis** | App Design(S2) → Functional Design(U6) → Code Gen(U6,U7) → Build&Test |
| **D-33** | **FR-1.2 (제거측)** 묶음에서 리소스 **제거** | **미구현** — `vc-core::WorkBundle::remove_resource`는 존재하나 이를 호출하는 커맨드·UI 없음. 잘못 드래그한 항목을 되돌릴 수 없음 | `bundle.rs:88`, `lib.rs`(remove 커맨드 0건) | **Requirements Analysis** | App Design(S2/S3) → Functional Design(U6) → Code Gen(U6,U7) → Build&Test |
| **D-34** | **FR-3.2** 묶음의 **추가 버튼**으로 실행 중 창을 선택해 추가 | **미구현** — 등록 경로는 드래그(FR-3.1) 하나뿐 | `App.tsx`(추가 버튼 0건) | **Requirements Analysis** | 상동 |
| **D-35** | **FR-3.3 · US-3.3** 파일·폴더·앱 실행파일·**웹 주소를 직접 끌어다 놓아** 추가 | **미구현이며 현재 구조상 차단** — `tauri.conf.json`이 `dragDropEnabled: false`로 OS 파일 드롭을 **의도적으로 비활성화**(HTML5 DnD와 충돌 회피, 커밋 `1ae75df`). 즉 FR-3.1을 살리려 FR-3.3을 희생한 미기록 트레이드오프 | `tauri.conf.json:16`, 커밋 `1ae75df` | **Requirements Analysis** (FR-3.1 vs FR-3.3 충돌 해소) | App Design(S3) → Functional Design(U6,U7) → Code Gen(U6,U7) → Build&Test |
| **D-36** | **NFR-U1 · 완료기준** 데이터 저장 위치·권한 설정 방법·기본 사용법 **사용자 문서** 제공 | **부재** — 리포에 README·사용자 문서가 없음(`REQUIREMENTS.ko.md`·`CLAUDE.md`만 존재). Claude 콘솔의 **외부 전송 고지**도 미제공 | 리포 루트 `ls *.md` | **Requirements Analysis** (DoD 확인) | Build&Test / Operations |
| **D-37** | **FR-7.5 · NFR-Pf3** 앱이 보이지 않거나 리사이즈 중에는 폴링을 줄인다 | **미구현** — 가시성/리사이즈 조건 없이 1초 무조건 폴링(`document.hidden`·visibilitychange 미사용). 동시성 가드(FR-7.6)는 드래그 가드로만 부분 존재 | `App.tsx:349-353`, `:325-344` | **Construction — NFR Requirements (U6/U7)** | NFR Design(U6,U7) → Code Gen(U6,U7) → Build&Test |
| D-38 | **FR-12.3 · AC-17** 세션 **전체 대화 스크롤 열람** | **미구현** — 대화 패널이 제거되었고 `SessionSnapshot.conversation`은 fetch되나 렌더 안 됨. 기존 `#E1`, **여전히 유효** | `App.tsx:176-223`, `types.ts:43-48` | **Requirements Analysis** (요구 유지 vs 정식 축소) | User Stories(US-12.2) → App Design(S6) → Functional Design(U7) → Code Gen(U7) → Build&Test |
| D-39 | **FR-8.3/8.5/8.10 · AC-14** 패널/카드 크기 조절 설정이 재실행 후 유지 | **부분 미구현** — CSS `resize: horizontal/vertical`로 조절은 되나 **어떤 커맨드도 레이아웃 설정을 읽거나 쓰지 않음**(S7 미배선). `AppSettings`의 레이아웃 4필드는 기본값 그대로. 기존 `#E2`, **여전히 유효** | `styles.css:269,696`; `lib.rs`(get/update_settings 커맨드 0건) | **Construction — Functional Design (U6)** | NFR Design(U6) → Code Gen(U6,U7) → Build&Test |
| D-40 | **FR-10.12 · AC-7/AC-8** Windows 브라우저 탭을 UI Automation으로 구분 | **스텁** — `WinBrowserTabReader::read_tabs()`가 빈 벡터 반환. Windows에서 탭 관련 FR-2.1/9.1/9.2/9.3이 성립하지 않음. 기존 `#B3`, **여전히 유효** | `vc-os-windows/src/lib.rs:550-564` | **Construction — Functional Design (U4)** | NFR Design(U4) → Code Gen(U4,U6,U7) → Build&Test |
| D-41 | **FR-10.13 · US-10.4** Windows 트레이 상주 + 전역 단축키 | **부재**. 기존 `#B5`, **여전히 유효** | vc-os-windows 전반, `tauri.conf.json`(tray 설정 없음) | **Construction — Functional Design (U4)** | Code Gen(U4,U6) → Build&Test |
| D-42 | **FR-10.2 · AC-13** macOS 접근성 권한 안내 + 실제 권한 상태 재확인 | **부재** — `PermissionChecker` 없음. 결과적으로 `ResourceStatus::PermissionRequired`가 런타임에 **생성될 수 없음**(D-30과 연쇄). 기존 `#B6`, **여전히 유효** | vc-os-macos 전반 | **Construction — Functional Design (U3)** | NFR Design(U3) → Code Gen(U3,U6,U7) → Build&Test |

### ⑤ NFR 위반 (NFR Violation) — **전부 신규 발견**

| ID | 승인된 NFR/확장 규칙 | 현행 코드 | 근거 | 최초 재검토 단계 | 하위 재생성 |
|---|---|---|---|---|---|
| **D-50** | **SECURITY-13(핵심 적용)** — U1 NFR 요구사항이 명시: serde `deny_unknown_fields`, **중첩 5단계 이하**, **문자열 1MB 이하**. Build&Test 요약도 "필수"로 등재 | **전부 미구현** — `deny_unknown_fields` 0건, 깊이·크기 제한 없음. 미래 버전 거부만 구현 | `grep deny_unknown_fields crates` = 0; `migrate/mod.rs:14-41`; `vc-core-nfr-requirements.md`; `build-and-test-summary.md` | **Construction — NFR Requirements (U1·U2)** | NFR Design(U1,U2) → Code Gen(U1,U2) → Build&Test |
| **D-51** | **SECURITY-04** — "Tauri WebView에 제한적 CSP 적용" | **미적용** — `"csp": null` | `tauri.conf.json:25`; `requirements.md §6` | **Construction — NFR Requirements (U6)** | NFR Design(U6) → Code Gen(U6) → Build&Test |
| **D-52** | **PBT-02/PBT-03(차단 규칙)** — "임의 입력 **1000회 이상**" (U1 NFR 요구사항), Build&Test 요약 "proptest 1000회" | **미충족** — `ProptestConfig` 미설정 → proptest 기본 **256회** | `grep ProptestConfig crates` = 0; `migrate/mod.rs:131`, `vc-sessions:457` | **Construction — NFR Requirements (U1)** | Code Gen(U1,U5) → Build&Test(pbt 지침) |
| **D-53** | **PBT-08(차단 규칙)** — "실패 시 최소 반례 축소 + **시드 로깅, CI 통합**" | **미충족** — `.github/` 없음(CI 자체 부재), `proptest-regressions` 시드 파일 미커밋 | 리포 전역 | **Construction — NFR Requirements (U1)** | Build&Test → (Operations: CI) |
| **D-54** | **NFR 성능 측정 기준** — MatchSignature <1ms, 창 매칭 <100ms, 상태 평가 <200ms (criterion 벤치마크), Build&Test의 창 열거 <500ms 등 | **측정 수단 부재** — criterion 의존성 없음, `benches/` 없음. 성능 목표가 **한 번도 검증되지 않음** | `grep criterion */Cargo.toml` = 0; `ls crates/*/benches` = 없음 | **Construction — NFR Requirements (U1)** | NFR Design(U1) → Code Gen(U1) → Build&Test |
| **D-55** | **커버리지 목표** — "C1~C5 알고리즘 최소 90%", 완료기준 "커버리지 ≥90%" | **측정 불가·미충족 추정** — 커버리지 도구 없음. vc-app(795 LOC)·vc-os-windows(782 LOC)·frontend(957 LOC)는 **테스트 0개** | 정적 카운트(§code-quality-assessment) | **Construction — NFR Requirements** | Code Gen(테스트 추가) → Build&Test |
| **D-56** | **PBT-09** — 프레임워크 선정: Rust `proptest` **+ TS/JS `fast-check`(프론트엔드)** | **프론트 미도입** — `fast-check` 미설치, 프론트 테스트 러너 자체 없음 | `frontend/package.json` | **Construction — NFR Requirements (U7)** | Code Gen(U7) → Build&Test |
| **D-57** | **SECURITY-10 공급망** — "의존성 취약점 스캔, 버전 핀" | **부분 미충족** — lock 파일 2개는 커밋됨(✅). 그러나 취약점 스캔(`cargo audit`/`npm audit`) 설정·실행 흔적 없음(CI 부재). 미사용 의존성 `sha2`가 표면에 잔존 | `vc-core/Cargo.toml:13`; 리포 전역 | **Construction — NFR Requirements** | Code Gen → Build&Test |

> **주의**: D-50~D-57은 `aidlc-state.md`의 **Extension Configuration에서 Security=Full(전 규칙 차단), PBT=Partial(PBT-02/03/07/08/09 차단)** 로 승인된 상태다. AI-DLC 규칙상 **활성 확장 규칙의 미준수는 차단 사유(blocking finding)** 이므로, 이 8건은 "나중에" 항목이 아니라 **정식 면제(waiver) 결정 또는 이행** 중 하나를 요구한다.

### ⑥ 문서 전용 드리프트 (Documentation-Only)

| ID | 발견 | 근거 | 최초 재검토 단계 | 하위 재생성 |
|---|---|---|---|---|
| **D-60** | **커맨드 수 17 → 18** — `activate_window`가 추가되었으나 `aidlc-state.md:96`, `known-deviations.md:20`(A2), `services.md:9`가 모두 "17개"로 기술 | `lib.rs` `generate_handler!` 18개 | Reverse Engineering 산출물 + 상태 문서 갱신 | 없음 |
| **D-61** | **claude-console 설계의 줄 번호 전면 무효** — `claude_status :468 → 실제 511`, `set_claude_api_key :478 → 519`, `set_claude_model :491 → 532`, `send_claude_message :507 → 548`, `ClaudeStatus :448 → 491`. `activate_window` 삽입으로 밀림 | `claude-console/design.md §4` vs `vc-app/src/lib.rs` | Construction — Functional Design (U6, claude-console) | 없음 |
| **D-62** | **`known-deviations.md#D5` 부정확** — "PBT-03는 `vc-sessions`에 있고 **vc-core 아님**"이라 단정하나, 실제로는 `vc-core/src/migrate/mod.rs:143`에도 `prop_parser_robust`가 존재한다 | `migrate/mod.rs:142-146` | Construction — NFR Design(U1) 기록 정정 | Build&Test(pbt 지침) |
| **D-63** | **`aidlc-state.md` 워크스페이스 경로가 Windows 경로 고정** — `C:\Users\gayeon\Documents\coding\vibe-control`. 본 세션은 `/home/trsprs/workspace/nott/vibe-control` | `aidlc-state.md:16` | 상태 문서 갱신 | 없음 |
| **D-64** | **미래 날짜 기재** — 상태/감사 로그에 `2026-09-09` 항목이 다수이나 모든 커밋 날짜는 `2026-09-08`. 이력 신뢰도 저하 | `aidlc-state.md`, `audit.md` vs `git log --date=short` | 상태 문서 갱신 | 없음 |
| D-65 | 어댑터 명명 이탈(`*Launcher`/`*IconReader`) — 기존 `#B7`, 유효 | `vc-os-*/src/lib.rs` | (기록 완료) | 없음 |
| D-66 | `AppSettings` 형상 이탈(`window_rect` 평탄화, `card_columns_hint` 삭제) — 기존 `#D2`, 유효 | `settings.rs:5-26` | (기록 완료) | 없음 |
| D-67 | 도메인 시그니처 이탈(`matches`/`evaluate`/`load_and_migrate`/`reorder`) — 기존 `#D3`, 유효 | `matching/window.rs:14` 등 | (기록 완료) | 없음 |
| D-68 | 파일 레이아웃 이탈(설계의 `url.rs`/`signature.rs`/`plan.rs` 등 → 단일 `mod.rs`) — 기존 `#D4`, 유효. 코드 생성 계획 `vc-core-code-generation-plan.md` Step 1~9의 체크박스가 **1개만 [x]**인 채 남아 있음 | `vc-core/src/` 트리; `vc-core-code-generation-plan.md:24-` | Construction — Code Generation 계획(U1) 체크박스 정정 | 없음 |

---

## 3. 요구사항 커버리지 매트릭스 (요약)

| FR 그룹 | 충족 | 부분 | 미충족 | 주요 공백 |
|---|:--:|:--:|:--:|---|
| FR-1 묶음 관리 | 1.3, 1.4 | **1.1**(이름변경 없음), **1.2**(제거 없음) | — | D-32, D-33 |
| FR-2 실행 항목 표시 | 2.1, 2.2, 2.3, 2.5, 2.8 | 2.4(창 그룹 내 점 ✅ / 저장 리소스 상태 ❌), 2.6(단일클릭으로 변경—§13.4 개정 반영), 2.7(노이즈 필터: Win 툴윈도우만) | — | D-30 |
| FR-3 리소스 등록 | 3.1, 3.4 | 3.5(앱 단위 dedup으로 같은 앱 여러 창 등록 불가) | **3.2, 3.3** | D-34, D-35 |
| FR-4 개별 활성화 | 4.1(창 지정 ✅), 4.3 | 4.2(앱 폴백 경로는 여전히 앱만 활성화), 4.4(재발견 추적 없음) | — | D-23 |
| FR-5 전체 활성화 | 5.1, 5.2, 5.3, 5.4 | — | — | ✅ 온전 |
| FR-6 항목 편집 | — | — | **6.1, 6.2, 6.3** | D-31 |
| FR-7 상태 표시·갱신 | 7.4 | 7.6(드래그 가드만) | **7.1, 7.2, 7.3, 7.5** | D-30, D-37 |
| FR-8 화면 | 8.1, 8.2, 8.4, 8.6, 8.7, 8.8, 8.9 | 8.3, 8.5(조절은 되나 미영속) | **8.10** | D-39 |
| FR-9 브라우저 | (macOS만) 9.1~9.4 | 9.5, 9.6, 9.7 | Windows 전 항목 | D-40 |
| FR-10 OS별 | 10.1, 10.4~10.6(mac), 10.8~10.11(win) | 10.3, 10.7, 10.9 | **10.2, 10.12, 10.13** | D-40, D-41, D-42 |
| FR-11 데이터 | 11.1~11.4, 11.6(bundles), 11.7, 11.8 | **11.5/11.6**(settings.json 비원자적) | — | 기존 #C2, D-13 |
| FR-12 코딩 세션 | 12.1, 12.2, 12.4~12.9 | — | **12.3**(전체 대화 열람) | D-38 |
| **신규(미문서)** | — | — | 부팅 스플래시 | **D-01** |

**인수 조건(AC)**: AC-1/2/3/11/12/18/19/20 충족 · AC-4/5/6/9/10/15 부분 또는 미검증 · **AC-7/8(Windows) · AC-13 · AC-14 · AC-17 미충족**.

---

## 4. 단계별 영향 집계 (Impact Roll-up)

AI-DLC 재실행 규칙: **어떤 단계를 되돌리면, 그 산출물을 소비하는 모든 후속 단계를 재생성해야 한다.** 발견을 최초 재검토 단계별로 집계하면 다음과 같다.

| 최초 재검토 단계 | 해당 발견 | 재생성이 필요한 하위 단계 |
|---|---|---|
| **INCEPTION — Requirements Analysis** | D-01, D-30, D-31, D-32, D-33, D-34, D-35, D-36, D-38 (**9건**) | User Stories → Workflow Planning → Application Design → Units Generation → (해당 단위의) Functional Design → NFR Requirements → NFR Design → Code Generation → Build&Test |
| **INCEPTION — Workflow Planning** | D-25 (인프라 설계 SKIP 재평가) | Infrastructure Design(U6) → NFR Design(U6) → Code Generation(U6) → Build&Test |
| **INCEPTION — Application Design** | D-20, D-21, D-22, D-23, D-24 (**5건**, 헥사고날 복원) | Units Generation → 전 단위 Functional Design → NFR Design → Code Generation(U1·U3·U4·U5·U6·U7) → Build&Test |
| **CONSTRUCTION — Functional Design** | D-14(U1), D-26(U3·U4·U5·U6·U7), D-39(U6), D-40(U4), D-41(U4), D-42(U3), D-61(U6) (**7건**) | 해당 단위 NFR Requirements → NFR Design → Code Generation 계획+실행 → Build&Test |
| **CONSTRUCTION — NFR Requirements** | D-37, D-50, D-51, D-52, D-53, D-54, D-55, D-56, D-57 (**9건, 전부 차단성 확장 규칙**) | NFR Design → Code Generation → Build&Test |
| **CONSTRUCTION — Code Generation** | D-10, D-11, D-12, D-13, D-68 (**5건**) | Build&Test |
| **문서/상태 갱신만** | D-60, D-62, D-63, D-64, D-65, D-66, D-67 (**7건**) | 없음 |
| **조치 불요(해소 확인)** | D-02, D-03 | — |

### 최악 경로(모두 수용 시)
Requirements Analysis 재실행이 필요한 발견이 9건이므로, 전부 수용하면 **INCEPTION 전 단계 + CONSTRUCTION 전 단위 + Build&Test가 통째로 재생성 대상**이 된다(사실상 전체 워크플로 재실행). 이는 현실적 선택지가 아니며, 아래 §5의 범위 결정이 필요하다.

### 최소 경로(문서 정합화만)
D-60/D-61/D-62/D-63/D-64 + 신규 이탈 등록(D-01/D-10/D-11/D-12 및 D-50~D-57의 waiver 기록)만 수행 → **코드 무수정, 재생성 단계 0개.** 단 이 경우 차단성 확장 규칙 8건이 "정식 면제"로 남는다는 점이 명시적으로 승인되어야 한다.

---

## 4-b. 최종 처리 결과 (2026-09-08 확정)

사용자 결정: **Q1=A(옵션 B) · Q2=A · Q3=C · Q4=B · Q5=B**, Q1/Q3 모순은 **Q3 우선**으로 해소(`reconciliation-clarification-questions.md`).

| 처리 | 건수 | 발견 ID |
|---|---:|---|
| ✅ **코드 구현 완료** | 1 | **D-37** 폴링 절약 (FR-7.5/7.6, NFR-Pf3) — `document.hidden` 스킵 · 복귀 시 즉시 갱신 · resize 400ms 유예 · in-flight 가드. `tsc` + `vite build` 통과 |
| ✅ **요구사항 정식 승격** | 1 | D-01 스플래시 → FR-13 + AC-21 + EPIC-13/US-13.1 |
| ✅ **요구사항 정식 개정** (축소·연기·범위제외) | 13 | D-30~D-42 (requirements v1.1, 각 항목에 되돌릴 근거 명기) |
| ✅ **승인된 면제 (waiver)** | 8 | D-50~D-57 → `known-deviations.md#H-5` (수용 리스크 + 재검토 트리거 기록, 확장 설정 하향) |
| ✅ **신규 이탈 등록** | 4 | D-10→H2(고DPI, 수정 보류) · D-11→H3 · D-12→H4 · D-01→H1 |
| ✅ **문서 정정** | 5 | D-60(17→18) · D-61(줄번호) · D-62(D5 정정) · D-63(경로) · D-64(날짜) |
| ✅ **산출물 소급 생성** | 1 | D-26 → CONSTRUCTION 문서 11개 + 코드생성계획 6개 |
| 📌 **백로그 유지** (조치 안 함, 기록됨) | 12 | D-13/D-14(구현결함) · D-20~D-25(아키텍처) · D-68 등 |
| ⏹ **조치 불요** | 2 | D-02, D-03 (이미 해소 확인) |

**잔여 미해결: 없음** — 45건 전량이 *구현 / 승격 / 개정 / 면제 / 등록된 백로그* 중 하나로 귀결되었다.

**코드 변경 총량**: `frontend/src/App.tsx` 1개 파일(D-37). `crates/`는 무수정.

---

## 5. 승인 요청 — 범위 옵션 *(확정: 옵션 B — 아래는 이력으로 보존)*

아래 중 하나(또는 조합)를 선택해 주시면 그에 맞춰 재실행 계획을 세운다. **어떤 선택에서도 지금까지 코드는 수정하지 않았다.**

### 옵션 A — 문서 정합화만 (코드 무수정)
- 대상: D-60·D-61·D-62·D-63·D-64 정정 + D-01(스플래시)·D-10(고DPI 결함)·D-11·D-12를 `known-deviations.md`에 신규 등록 + D-50~D-57을 **명시적 waiver**로 기록.
- 재생성 단계: 없음. 소요: 짧음.
- 리스크: 요구사항 13건·NFR 8건이 미충족 상태로 고착. Security(Full)/PBT(Partial) 확장의 차단 규칙이 면제로 남는다.

### 옵션 B — 스플래시 정식화 + 차단성 NFR 이행 (권장)
- 대상: **D-01**(부팅 스플래시를 FR로 정식 승격 → 스토리·U7 설계·Build&Test 반영) + **D-10** 결함 등록 + **D-50~D-53**(SECURITY-13 강화, CSP, PBT 1000회, 시드/CI) + D-37(폴링 절약) + 옵션 A 전량.
- 재실행 경로: `Requirements Analysis(스플래시 한정) → User Stories → Application Design(U7 국소) → Functional Design(U7) → NFR Requirements(U1·U2·U6·U7) → NFR Design → Code Generation(U1·U2·U6·U7) → Build&Test`.
- 리스크: 중간. 아키텍처(D-20~D-24)는 건드리지 않으므로 blast radius가 제한된다.

### 옵션 C — 기능 공백 해소 (요구사항 재확정 포함)
- 대상: 옵션 B + **D-30**(상태 표시) · **D-31/D-32/D-33/D-34**(편집·이름변경·제거·추가버튼) · **D-38**(대화 뷰어) · **D-39**(레이아웃 영속) · **D-35**(FR-3.1 vs FR-3.3 충돌 해소).
- 재실행 경로: Requirements Analysis 전면 → 이후 전 단계.
- 리스크: 큼. 사실상 2차 반복(iteration) 규모.

### 옵션 D — 아키텍처 복원까지
- 대상: 옵션 C + **D-20~D-24**(포트 트레이트화·서비스 계층·이벤트화·도메인 배선) + **D-25**(인프라 설계) + **D-40/D-41/D-42**(Windows 탭·트레이·mac 권한).
- 리스크: 최대. 전 단위 Code Generation 재실행 = 사실상 리라이트.

> **참고 — 별개 판단 사항**: D-26(“완료로 기록되었으나 산출물이 없는 CONSTRUCTION 단계”)은 위 옵션과 무관하게 **기록 정합성** 문제다. (a) 산출물을 소급 생성할지, (b) `aidlc-state.md`의 해당 체크박스를 “(auto — 문서 미생성)”으로 정정할지 별도 결정이 필요하다.

---

## 6. 관련 문서
- 본 회차 RE 산출물: `business-overview.md`, `architecture.md`, `code-structure.md`, `api-documentation.md`, `component-inventory.md`, `technology-stack.md`, `dependencies.md`, `code-quality-assessment.md`, `reverse-engineering-timestamp.md`
- 기존 이탈 기록: `../../known-deviations.md` (A~G) — 본 분석에서 전 항목 재검증
- 기준선: `../requirements/requirements.md`, `../user-stories/stories.md`, `../application-design/*`, `../../construction/**`

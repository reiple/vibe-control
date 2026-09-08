# 알려진 이탈 (Known Deviations) — 원 설계 대비 실제 구현

작성: 2026-09-08 · 문서 정합화(Documentation Reconciliation) 단계
정합화 기준: **현행 코드 기준 + 이탈 기록** (설계/상태 문서는 실제 구현대로 기술하되, 원 헥사고날 설계 의도와의 차이를 본 문서에 보존)

> 이 문서는 INCEPTION Application Design(`inception/application-design/*`)이 기술한 **원 설계**와 CONSTRUCTION 결과인 **실제 코드** 사이의 확인된 차이를 한곳에 모은다. 각 이탈은 유형으로 분류한다:
> - **[문서반영]** — 코드가 사실상 최종 형태이므로 설계/상태 문서를 코드에 맞게 갱신함(또는 본 문서로 기록). 되돌릴 계획 없음.
> - **[코드백로그]** — 원 설계 의도가 여전히 유효하며, 향후 코드로 구현/보완할 후보. (이번 작업에서 코드는 수정하지 않음)
> - **[수용]** — 원 설계와 다르지만 의도적으로 수용한 결정.

검증 근거: 2026-09-08 읽기 전용 코드 리뷰(vc-core / 어댑터 / vc-app·frontend 3영역 대조). 이후 같은 날 git pull(커밋 `6039456` Windows 사용자 앱만 표시+아이콘, `1abc6ed` 활성화/열거 신뢰성)로 들어온 변경분을 재확인하여 B1/B4/B7 반영.

---

## A. 아키텍처 수준 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거(file:line) |
|---|---|---|---|---|
| A1 | vc-core가 포트 트레이트 **P1–P8**를 모두 정의(도메인이 능력을 선언, 어댑터가 구현) | vc-core에 `trait` 정의 **0개**. P4는 `vc-sessions`, P5는 `vc-store`에 트레이트로 존재. P1/P3는 트레이트 없이 OS별 구체 struct만. P2/P6/P7/P8은 트레이트로 존재하지 않음 | [문서반영]+[코드백로그] | `vc-core/src/*`(trait 없음), `vc-sessions/src/lib.rs:17`, `vc-store/src/lib.rs:5` |
| A2 | 애플리케이션 서비스 **S1–S7** + `TauriCommandBridge` + `RefreshScheduler`로 유스케이스 오케스트레이션(포트 DI) | 서비스/브리지/스케줄러 struct 부재. 기능이 `AppState` + 17개 `#[tauri::command]` 핸들러 + 자유 함수로 평면화, 어댑터는 `#[cfg(target_os=…)]` 블록에서 **구체 타입 직접 호출** | [문서반영]+[코드백로그] | `vc-app/src/lib.rs:23`(AppState), `:683-701`(invoke_handler), `:542-668`(cfg dispatch) |
| A3 | 프론트엔드는 코어 이벤트를 **구독하는 얇은 뷰**(`status_delta`/`activation_report` emit) | 백엔드 `emit` 없음, 프론트 `listen()` 없음. 프론트가 1초 `setInterval`로 **폴링** | [문서반영]+[코드백로그] | `frontend/src/App.tsx:262` |
| A4 | 도메인 코어(`IdentityMatcher`/`RestorePlanner`/`StatusEvaluator`)를 서비스가 조합 | vc-app이 이들을 **import조차 하지 않음**; 매칭/복원계획/노이즈판정이 오케스트레이션에 연결되지 않음 | [코드백로그] | `vc-app/src/lib.rs:13`(import 목록) |

**해석**: 코드는 헥사고날의 "포트/어댑터"를 트레이트-DI가 아닌 **컴파일타임 `cfg` 분기 + 구체 타입**으로 실현했다. MVP로서는 동작하지만, 도구/OS 확장성·테스트 모킹·이벤트 기반 UI라는 원 설계의 이점은 아직 미실현. 설계 문서에는 "구현 현황" 배너로 현재 구조를 명시하고, 원 설계는 향후 리팩터링 목표로 남긴다.

---

## B. 플랫폼 어댑터 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| B1 | Windows 창 열거 = **UI Automation, 최상위 창** | PowerShell `Get-Process`에서 `MainWindowHandle != 0 && MainWindowTitle` 필터로 **사용자가 띄운 가시 창 앱만** 열거(이름 기준 dedup). UI Automation은 구현된 적 없음(설계→`tasklist /v`→`Get-Process` 이력) | [문서반영] | `vc-os-windows/src/lib.rs:57-124` |
| B2 | macOS 창 열거 = **AXUIElement / accessibility 크레이트** | `osascript`/JXA + `NSWorkspace` 셸아웃(네이티브 접근성 의존성 없음) | [문서반영] | `vc-os-macos/src/lib.rs:47-118` |
| B3 | `WinBrowserTabReader`가 Edge/Chrome 탭 수집 | **스텁** — 빈 벡터 반환("Not yet implemented on Windows") | [코드백로그] | `vc-os-windows/src/lib.rs:264-275` |
| B4 | `WinIconProvider`(Windows 아이콘 제공) | ✅ **구현됨** (2026-09-08 커밋 `6039456`) — `WinIconReader`가 PowerShell + C# `Add-Type`(shell32 `SHGetImageList` jumbo 256px → 48px → 32px 폴백, 투명 여백 트림)로 exe 아이콘을 base64 PNG data URI로 추출. vc-app이 `extract_app_icon`에서 호출 + 캐시. **이탈 해소** | [문서반영] | `vc-os-windows/src/lib.rs:127-259`, `vc-app/src/lib.rs:365-372` |
| B5 | Windows 트레이/전역 단축키 훅(FR-10.13, US-10.4) | **부재** | [코드백로그] | vc-os-windows 크레이트 전반 |
| B6 | `MacPermissionChecker`(P6, AC-13) | **부재** — PermissionChecker struct 없음 | [코드백로그] | vc-os-macos 크레이트 전반 |
| B7 | 어댑터 이름 `*WindowActivator` / `*IconProvider` | 활성화는 `*Launcher`(macOS/Windows), 아이콘은 `MacIconReader`/`WinIconReader`로 명명. `open_app`은 실행 전 이미 떠 있는 창 포커스 시도(FR-2.6, 커밋 `1abc6ed`) | [문서반영] | `vc-os-macos/src/lib.rs:174,309`, `vc-os-windows/src/lib.rs:127,278,315` |

**참고**: B3/B5/B6은 여전히 미구현. B4(Windows 아이콘)는 2026-09-08 git pull로 들어온 커밋 `6039456`에서 구현되어 이탈 해소됨. Windows 앱 활성화 중복창 문제도 커밋 `1abc6ed`에서 "실행 중이면 기존 창 포커스" 경로 추가로 해결됨(FR-2.6).

---

## C. 영속성(vc-store) 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| C1 | 저장 실패 시 임시 파일 **정리/롤백** | `save()`가 쓰기 실패 시 `Err`만 반환, `.tmp` 파일 정리 없음(누수) | [코드백로그] | `vc-store/src/lib.rs:80-81` |
| C2 | 단일 JSON 저장소가 설정 포함 **원자적 교체** | `bundles.json`은 temp→rename 원자적. 그러나 `settings.json`은 **별도 파일 + 비원자적** `fs::write` | [문서반영]+[코드백로그] | `vc-store/src/lib.rs:103-112` |
| C3 | `BundleStore`가 `StoreState`에 대한 `load()/save()` | 실제 트레이트는 `Vec<WorkBundle>` 대상 + `load_settings`/`save_settings` 추가. `StoreState` 타입 없음. 버전 태깅·마이그레이션은 vc-store가 아니라 `vc-core/migrate`에 존재 | [문서반영] | `vc-store/src/lib.rs:5-10`, `vc-core/src/migrate/mod.rs:6,44` |

---

## D. 도메인(vc-core) 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| D1 | 묶음 내 **중복 식별 금지** 불변식 강제(AC-3, BR-2). `WorkBundle::add_resource -> Result<()>`가 거부, `IdentityMatcher::is_duplicate` 구현 | `add_resource`가 `()` 반환, 검사 없이 `push`(주석: "IdentityMatcher 책임"). `is_duplicate`는 **미구현**. 등록 중복 방지는 vc-app 커맨드 내 인라인으로만 존재 | [코드백로그] | `bundle.rs:82-85`, (`is_duplicate` grep 0건), `vc-app/src/lib.rs:429` |
| D2 | `AppSettings { panel_width, card_height, window_rect: Option<Rect>, card_columns_hint }` | `window_rect`가 `window_x/y/width/height`로 평탄화, `card_columns_hint` **삭제**, Claude 3필드(`claude_api_key/model/region`) 추가 | [문서반영] | `vc-core/src/models/settings.rs:5-26` |
| D3 | 시그니처: `matches -> bool`(component-methods), `evaluate(saved,running,perm,sessions) -> Vec<(ResourceId,ResourceStatus)>`, `load_and_migrate -> StoreState`, `reorder(order: Vec<ResourceId>) -> Result<()>` | `matches -> MatchResult`, `evaluate(&[Resource],&[String],bool) -> Vec<StatusSnapshot>`, `load_and_migrate -> Vec<WorkBundle>`, `reorder(&mut self)`(인자 없이 순번 재부여). `RunningItem`/`PermissionState`/`SessionSnapshot`은 vc-core에 없거나 축소 | [문서반영] | `matching/window.rs:14`, `evaluate/mod.rs:38`, `migrate/mod.rs:14`, `bundle.rs:88` |
| D4 | 파일 레이아웃: `normalize/{url,path,app_id}.rs`, `matching/{signature,distinct}.rs`, `restore/plan.rs`, `evaluate/{status,session}.rs`, `migrate/{rules,transforms}.rs`, `tests/*.rs`, `benches/*.rs` | 각 영역이 단일 `mod.rs`(+`matching/window.rs`)로 통합. `tests/`·`benches/` 디렉터리 없음 | [문서반영] | `vc-core/src/` 트리 |
| D5 | PBT-02(라운드트립)·PBT-03(파서 견고성)이 vc-core `tests/pbt_*.rs` | PBT-02는 `vc-core/migrate/mod.rs`(인라인), **PBT-03는 `vc-sessions/lib.rs:461`**(vc-core 아님) | [문서반영] | `migrate/mod.rs:131-146`, `vc-sessions/src/lib.rs:461-474` |
| D6 | — | 미사용 `sha2` 의존성(매칭은 `std::DefaultHasher` 사용) | [코드백로그] | `vc-core/Cargo.toml`, `matching/mod.rs:27` |

**일치 확인(이탈 아님)**: `ResourceKind`(6개), `ResourceStatus`(4개), `SessionCompletion`(Waiting/NotWaiting/Unknown) enum은 설계와 **정확히 일치**.

---

## E. 프론트엔드/UX 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| E1 | 세션 카드에서 **전체 대화 스크롤 열람**(FR-12.3, AC-17) | 인라인 대화 패널 제거됨. "View"는 터미널 포커스로 대체, 카드엔 완료 칩+마지막 질문 1줄만. `SessionSnapshot.conversation`은 fetch되나 미렌더 | [코드백로그] | `App.tsx:144-191,353`, `audit.md`(대화 패널 제거 기록) |
| E2 | 레이아웃 설정 재실행 후 유지(FR-8.3/8.5/8.10, AC-14) | 저장 스키마·CSS 리사이즈는 있으나 어떤 커맨드도 레이아웃 설정을 **읽거나 쓰지 않음**(SettingsService 미배선). 실제로 읽고/쓰는 설정은 Claude 3필드뿐 | [코드백로그] | `vc-app/src/lib.rs`(get/update_settings 커맨드 없음) |
| E3 | "캡처" 모델(capture_current) | 프론트는 그룹 생성 + 드래그 모델 사용. `capture_current`/`save_bundles`는 백엔드·`api.ts`에 존재하나 `App.tsx`가 호출 안 함(고아) | [문서반영] | `api.ts:19,24`, `App.tsx`(미호출) |

**일치 확인**: 프론트↔백엔드 커맨드 정합성은 깨끗(프론트가 호출하는 모든 커맨드가 백엔드에 존재; 없는 커맨드 호출 없음).

---

## F. 원 설계에 없는 신규 기능

| # | 내용 | 유형 | 상세 |
|---|---|---|---|
| F1 | **앱 내 Claude 프롬프트 콘솔 (AWS Bedrock 경유)** — `claude.rs` + 4개 커맨드 + `AppSettings` 3필드 + `reqwest` 의존성 + 프론트 콘솔 UI | [수용] | 별도 설계 문서 `construction/vc-app/claude-console/design.md` 신설. **NFR-S1과 충돌**(로컬 전용 vs 외부 HTTPS 송신) → `requirements.md` NFR-S1 개정 + SECURITY 매핑에 보안 노트 추가. 유형은 **[수용]으로 확정**(기능이 의도적으로 도입·동작 중이며 NFR-S3로 규율). 단, 향후 "로컬 전용"이 강제 요구사항으로 격상되면 옵트인 토글 / 기본 비활성으로 완화 재검토 여지 있음. |

---

## 백로그 (우선순위)

원 설계 의도가 유효하나 코드에 아직 반영되지 않은 항목(이번 정합화에서 **코드는 수정하지 않음**; 향후 반복 후보).

| 우선 | 항목 | 관련 이탈 | 관련 FR/AC |
|---|---|---|---|
| P1 | 세션 전체 대화 뷰어 복원(fetch된 `conversation` 렌더) | E1 | FR-12.3, AC-17 |
| P1 | 레이아웃 설정 영속화(get/update_settings 배선) | E2 | FR-8.10, AC-14 |
| P1 | 저장 실패 시 `.tmp` 정리 + `settings.json` 원자적 쓰기 | C1, C2 | FR-11.5/11.6, SECURITY-15 |
| P2 | 중복 식별 금지 불변식 도메인화(`add_resource`/`is_duplicate`) | D1 | FR-3.4, AC-3 |
| P2 | Windows 브라우저 탭 읽기(Edge/Chrome) | B3 | FR-10.12, AC-7/8 |
| P3 | Windows 트레이/전역 단축키 | B5 | FR-10.13 |
| P3 | macOS 권한 체커/안내 상태 재확인 | B6 | FR-10.2, AC-13 |
| P3 | 포트 트레이트화 + 서비스/이벤트 리팩터링(헥사고날 복원) | A1–A4 | 아키텍처 |
| P4 | 미사용 `sha2` 의존성 제거 | D6 | — |

---

## 관련 문서
- 원 설계: `inception/application-design/components.md`, `services.md`, `component-methods.md`, `unit-of-work.md`
- 신규 기능: `construction/vc-app/claude-console/design.md`
- 상태/이력: `aidlc-state.md`, `audit.md`

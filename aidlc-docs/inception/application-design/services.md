# 서비스 (Services)

단계: INCEPTION — Application Design
참조: `components.md`, `component-methods.md`
역할: 유스케이스 오케스트레이션 — 포트(트레이트)를 조합하고, 도메인 로직을 호출하며, 상태 변경을 이벤트로 방출.

> 서비스는 어댑터 구체 타입에 의존하지 않고 **포트 트레이트**에 의존(DI). 테스트 시 모의 포트 주입(통합 테스트 계획과 정합).

> ⚠ **구현 현황(2026-09-08 정합화)** — 아래 S1–S7 서비스·`TauriCommandBridge`·`RefreshScheduler`·이벤트(`status_delta`/`activation_report`)·SEQ 시퀀스는 **원 설계 의도**이며 **코드에는 struct로 존재하지 않는다**. 실제로는 `vc-app`의 `AppState` + 17개 `#[tauri::command]` 핸들러가 도메인/어댑터를 직접 호출하고, 상태 갱신은 이벤트 구독이 아니라 프론트의 1초 폴링으로 이뤄진다. 차이·백로그: **[`known-deviations.md#A-아키텍처-수준-이탈`](../../known-deviations.md)**.

---

## S1. RunningInventoryService
- **책임**: 실행 중 창/탭을 열거·그룹핑·검색 필터링하여 UI에 제공.
- **협력 포트**: `WindowEnumerator`, `BrowserTabReader`, `IconProvider`; 도메인 `StatusEvaluator.is_noise`, `IdentityMatcher`.
- **오케스트레이션**: 열거 → 노이즈 제거(AC-7) → 앱별 그룹핑(FR-2.1/2.2) → 선택 창 표시(FR-2.4) → 검색 필터(FR-2.5).
- **관련**: FR-2, EPIC-2.
> 🔧 **창 단위 재정렬(2026-09-08 진행 중, FR-2.8/AC-20)**: 이 서비스가 규정한 "**앱별 그룹핑 + 그룹 내 창 나열 + 선택 창 표시**"가 원 설계이며 창 단위다. 출하 코드는 이를 앱 단위(프로세스당 대표 창)로 축소 시행했고, 현재 창 단위 열거로 재정렬 중이다. 상세: `known-deviations.md#G1`, `construction/vc-os-windows/functional-design/window-enumeration.md`.

## S2. BundleService
- **책임**: 작업 묶음/리소스의 CRUD, 항목 편집(표시명·재실행 주소), 정렬.
- **협력 포트**: `BundleStore`; 도메인 `DomainModel`, `StoreMigration`.
- **오케스트레이션**: 변경 → 도메인 불변식 검증 → `BundleStore.save`(원자적) → 변경 이벤트 방출. 삭제 확인 UI는 프론트(FR-1.3).
- **관련**: FR-1, FR-6, FR-11 · AC-1/12/14.

## S3. RegistrationService
- **책임**: 실행 중 항목 또는 드롭된 파일/폴더/앱/URL을 묶음에 등록(중복 방지, 유사 항목 개별화).
- **협력 포트**: `WindowEnumerator`/`BrowserTabReader`(실행 항목 해석), `BundleStore`; 도메인 `IdentityMatcher`.
- **오케스트레이션**: 후보 식별 → `is_duplicate` 검사(AC-3) → `distinct_key`로 유사 항목 구분(AC-4/5/8) → 입력 검증(SECURITY-05: 경로/URL 안전 보관) → 저장.
- **관련**: FR-3, EPIC-3.

## S4. ActivationService
- **책임**: 개별 항목 활성화 및 묶음 전체 활성화(정렬 순서, 부분 실패 지속, 결과 요약).
- **협력 포트**: `WindowActivator`, `BrowserTabReader.open_url`, `CodingSessionProvider`(세션 활성화 대체); 도메인 `IdentityMatcher`, `RestorePlanner`.
- **오케스트레이션**:
  - 개별: 매칭되는 실행 창 있으면 `focus`, 없으면 `RestorePlanner.plan_reopen` → `reopen`/`open_url`. 앱만 활성 시 성공 처리 안 함(FR-4.2).
  - 전체: `plan_bundle_activation` 순서대로 각 스텝 실행, 실패해도 계속(AC-11), `ActivationReport`(성공/미발견/오류) 반환(FR-5.4).
- **관련**: FR-4, FR-5, FR-9.3 · AC-2/9/10/11.
> 🔧 **창 지정 활성화(2026-09-08 진행 중, FR-2.8/AC-20)**: "매칭되는 실행 창을 `focus`, 앱만 활성 시 성공 아님(FR-4.2)"은 **특정 창** 대상이 원 설계다. 출하 코드 `open_app`은 앱 대상(아무 창 포커스)에 머물러 있어, HWND/창 지정 활성화 경로를 추가 중이다. 상세: `known-deviations.md#G2`.

## S5. StatusService
- **책임**: 주기적으로 실행 목록·권한·세션을 수집해 저장 리소스 상태를 갱신하고 이벤트로 방출.
- **협력 포트**: `RefreshScheduler`, `WindowEnumerator`, `BrowserTabReader`, `PermissionChecker`, `CodingSessionProvider`; 도메인 `StatusEvaluator`.
- **오케스트레이션**: `RefreshScheduler.on_tick` → (가시성/리사이즈 절약, 진행 중 중복 방지) → 수집 → `StatusEvaluator.evaluate` → 상태 델타 이벤트.
- **관련**: FR-7, FR-12.7 · NFR-Pf1/Pf2/Pf3.

## S6. CodingSessionService
- **책임**: 등록된 코딩 세션의 대화·마지막 질문·답변 완료여부 스냅샷 제공 및 변경 감지 갱신.
- **협력 포트**: `SessionProviderRegistry`/`CodingSessionProvider`.
- **오케스트레이션**: 세션 리소스 → 해당 도구 provider 선택 → `read_snapshot`(읽기 전용, 손상 허용) → UI 이벤트. 로컬 전용, 대화 로그 출력 금지(NFR-S1).
- **관련**: FR-12, EPIC-12 · AC-16/17/18/19 · PBT-03, SECURITY-13.

## S7. SettingsService
- **책임**: UI 설정(패널 너비·카드 높이·창 위치) 로드/저장으로 재실행 후 유지.
- **협력 포트**: `BundleStore`(또는 동일 저장소 섹션).
- **관련**: FR-8.3/8.5/8.6/8.10 · AC-14.

---

## 서비스 상호작용 (핵심 시퀀스)

### SEQ-A 전체 활성화 (JS-2)
```
UI.activateBundle(id)
  -> Bridge -> ActivationService.activate_bundle(id)
       -> RestorePlanner.plan_bundle_activation
       -> for step in steps:
            IdentityMatcher.matches(running?) ? WindowActivator.focus
                                              : RestorePlanner.plan_reopen -> reopen/open_url/session
            (실패 시 기록 후 계속)
       -> ActivationReport (성공/미발견/오류)
  -> Bridge.emit("activation_report")
```

### SEQ-B 상태 갱신 루프 (FR-7)
```
RefreshScheduler.on_tick
  -> (visible & !resizing & !in-progress?) 
  -> StatusService.refresh
       -> WindowEnumerator.list_running / BrowserTabReader.read_tabs
       -> PermissionChecker.state / CodingSessionProvider.read_snapshot
       -> StatusEvaluator.evaluate
  -> emit("status_delta")
```

### SEQ-C 코딩 세션 열람 (JS-3)
```
UI.openSession(resId)
  -> CodingSessionService.snapshot(resId)
       -> Registry.provider(toolId).read_snapshot(sessionRef)
  -> emit/return SessionSnapshot(conversation, last_question, completion)
```

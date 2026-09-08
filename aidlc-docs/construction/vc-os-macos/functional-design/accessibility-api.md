# macOS 접근성 API 설계 — U3 vc-os-macos

---

## 책임
- **WindowEnumerator**: 사용자에게 보이는 창 열거 (접근성 API + Process Management)
- **WindowActivator**: 창 활성화 및 앱 실행
- **BrowserTabReader**: Safari/Chrome 탭 수집
- **PermissionChecker**: 접근성 권한 상태

## 기술
- **Accessibility API**: `AXUIElement` 사용 (Rust 바인딩: `accessibility` 크레이트 또는 ffi).
- **Process Management**: `NSRunningApplication`, `Workspace` API.
- **Browser Access**: Safari는 AppleScript, Chrome은 `chrome://extensions/` API 또는 파일 시스템.

## 핵심 구현 포인트
- 보조 창 필터링 (시스템, 메뉴, 도크).
- 다중 데스크톱(Spaces) 지원.
- 접근성 권한 상태 확인 및 프롬프트 제공.

## 창 단위 재정렬 (2026-09-08 진행 중, FR-2.8/AC-20)
- **열거**: 현행 `list_running_apps()`는 `NSWorkspace` **앱 단위**. 창 단위로 확장 — 앱별 AX 창(`AXWindows`, 제목) 또는 `CGWindowListCopyWindowInfo`(창 번호·제목) 스냅샷을 U1 `RunningItem` 형태로 방출(`known-deviations.md#G1`).
- **창 지정 활성화**: 앱 활성화 후 특정 창 raise(AX `AXRaise` 가능 시, 아니면 창 번호 기반). 기존 `activate_terminal(match_hint)`(코딩 세션 창 포커스) 계열 로직 재사용(`#G2`). FR-4.2 준수(앱만 활성화는 성공 아님).
- **용어**: 여기의 "창/탭"은 vc-sessions 코딩 "세션"과 구분(window/tab vs session).
- 크로스유닛 계약: `../vc-os-windows/functional-design/window-enumeration.md`.

## AC 매핑
- AC-5/6/10/13/15: 접근성 창 식별, 파인더/편집기/브라우저 복원, 응답성, 권한, 아이콘.
- AC-20: 앱 그룹 펼침 → 특정 창 선택 → 정확한 창 최전면(단일 창 앱 동일).

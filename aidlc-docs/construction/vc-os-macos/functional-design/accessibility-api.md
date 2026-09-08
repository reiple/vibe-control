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

## AC 매핑
- AC-5/6/10/13/15: 접근성 창 식별, 파인더/편집기/브라우저 복원, 응답성, 권한, 아이콘.

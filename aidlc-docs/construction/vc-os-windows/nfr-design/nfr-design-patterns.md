# NFR 설계 패턴 — U4 vc-os-windows (as-built)

단계: CONSTRUCTION — U4, NFR Design · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

| 패턴 | 적용 | 목적 |
|---|---|---|
| **Native FFI facade** | `winffi` 모듈이 user32/dwmapi/kernel32 심볼만 `#[link]` + `extern "system"`으로 선언. 크레이트 의존 0 | 폴링 성능 + 공급망 축소 |
| **Callback accumulation** | `enum_windows_cb`가 필터를 통과한 창만 행으로 누적 | NFR-Pf2 탐색 상한 |
| **Guard-then-act** | `focus_window`가 `IsWindow`로 먼저 확인 후 활성화 | fail-closed (SECURITY-15) |
| **Guaranteed + best-effort 쌍** | 이름은 Toolhelp로 보장, 경로는 `QueryFullProcessImageName` best-effort + 폴백 | 권한 부족 프로세스에서도 목록 품질 유지 |
| **Env-var argument passing** | HWND를 명령 문자열이 아닌 환경변수로 PowerShell에 전달 | SECURITY-05 주입 방지 |
| **Minimal blast radius** | G1 수정 시 `raw_windows()`만 교체하고 `list_running_windows`/`list_running_apps`/상위 계약 전부 보존 | 회귀 억제 |
| **Silent subprocess** | `CREATE_NO_WINDOW`로 콘솔 플래시 제거 | SECURITY-09 / UX |

**미적용(백로그)**: 열거 결과 캐시/변경 감지(현재 매 초 전량 열거), 브라우저 탭 리더, 트레이 훅.

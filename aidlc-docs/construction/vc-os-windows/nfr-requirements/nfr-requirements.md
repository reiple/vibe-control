# NFR 요구사항 — U4 vc-os-windows (as-built)

단계: CONSTRUCTION — U4, NFR Requirements · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

## 성능 (이 단위의 핵심 NFR)
- **목표**: 창 열거 < 500ms, **1초 폴링에 안전할 것**.
- **결정 이력**: `tasklist /v`(응답 없는 창이 있으면 수 분 행) → PowerShell `Get-Process`(~1s, 그러나 프로세스당 창 1개만) → **Win32 `EnumWindows` 네이티브 Rust FFI**(현행). 인라인 C# `Add-Type`은 매 폴링 `csc` 재컴파일(~150–400ms)이 발생해 **의도적으로 배제**했다.
- **현행 비용**: 열거 경로에 서브프로세스·컴파일 **0회**. 아이콘 추출과 실행/포커스만 PowerShell을 사용하며, 아이콘은 vc-app에서 캐시된다.
- **미측정**: 정량 벤치마크 없음 → `#H5-e`.

## 신뢰성
- `WinLauncher::focus_window(hwnd)`: 10진수 전(全)자릿수 검증 → `IsWindow` 가드(닫힌 창이면 "gone" 오류) → `ShowWindowAsync` + `AppActivate(pid)` + `SetForegroundWindow`.
- PID→이름은 Toolhelp 스냅샷으로 **보장**(권한 부족 프로세스도 이름 유지), PID→전체 경로는 `QueryFullProcessImageNameW` **best-effort**(실패 시 `name.exe` 폴백).
- 모든 열거 실패는 빈 목록으로 degrade.
- **UI 프리즈 방지**: OS를 건드리는 vc-app 커맨드는 `async`여야 한다(U6 규칙 BR — 2026-09-08 프리즈 사고의 재발 방지책).

## 보안
| 규칙 | 처리 |
|---|---|
| SECURITY-05 / NFR-S2 | HWND는 숫자 검증 후 **환경변수**로 전달(명령 문자열 삽입 금지). 경로는 `ps_single_quote`(`'` 중복)로 인용 |
| SECURITY-09 | 콘솔 창 플래시 억제(`CREATE_NO_WINDOW = 0x0800_0000`) |
| SECURITY-11 | Win32 호출을 `winffi` 모듈 한 곳에 격리, 전부 `#[cfg(target_os="windows")]` |
| SECURITY-10 | `windows`/`winapi` 크레이트를 **도입하지 않음** — 공급망 표면 축소, 필요한 심볼만 `#[link]` |

## 확장 준수
| 규칙 | 상태 |
|---|---|
| SECURITY-05/09/11 | ✅ |
| SECURITY-13 | N/A |
| 커버리지 | ⏸ **테스트 0개 / 782 LOC** — 이 단위가 프로젝트 최대 리스크 지점 (`#H5-f`) |

## 창 필터 (실측 검증됨)
`IsWindowVisible` ∧ `GetWindowTextLengthW > 0` ∧ `GetWindow(GW_OWNER)==0` ∧ ¬(`GWL_EXSTYLE` & `WS_EX_TOOLWINDOW`) ∧ ¬`DWMWA_CLOAKED`

## 알려진 공백
- `WinBrowserTabReader` 스텁 → FR-10.12 / AC-7 / AC-8 미충족 (`#B3`, `#D-40`)
- 트레이/전역 단축키 부재 → FR-10.13 **범위 제외**로 개정 (`#B5`, `#D-41`)
- `list_running()` 죽은 API (`#H3`)

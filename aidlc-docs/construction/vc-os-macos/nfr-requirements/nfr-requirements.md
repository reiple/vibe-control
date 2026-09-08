# NFR 요구사항 — U3 vc-os-macos (as-built)

단계: CONSTRUCTION — U3, NFR Requirements · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

## 성능
- **목표**: 창 열거 1회 < 500ms(Build&Test 목표). 1초 폴링에 물리므로 호출당 비용이 주기를 넘지 않아야 한다.
- **현행**: 모든 경로가 `osascript`/JXA **서브프로세스 셸아웃**이다. `list_running_windows()`는 `NSWorkspace` 앱 목록 위에 System Events 창 제목을 덧씌운다 — 접근성 권한이 없으면 창 정보 없이 앱 목록만 반환(degrade, 회귀 없음).
- **상한**: 탐색 깊이·개수 상한은 스크립트 내부 질의 범위로 암묵 제한(NFR-Pf2). 명시적 카운트 상한은 없음.
- **미측정**: 실측 벤치마크 없음 → `#H5-e`.

## 신뢰성
- 모든 공개 API가 `Result`. 스크립트 실패·타임아웃은 `unwrap_or_default()`로 흡수되어 **빈 목록으로 degrade**하며 앱을 중단시키지 않는다(NFR-R1, SECURITY-15).
- `focus_window(handle)`는 앱 활성화 후 제목 기준 `AXRaise`를 **best-effort**로 시도한다 — 실패해도 앱 단위 활성화까지는 성공으로 남는다(FR-4.2 관점에서는 부분 성공).

## 보안
| 규칙 | 처리 |
|---|---|
| SECURITY-05 / NFR-S2 | 경로·URL은 `open` 인자로 전달. 터미널 명령은 `shell_quote`(POSIX 단일 인용, 내부 `'` 이스케이프)로 감싼다 |
| SECURITY-03 | 창 제목·대화 내용을 로그로 남기지 않음 |
| SECURITY-15 | 모든 셸아웃 실패를 흡수 |

## 확장 준수
| 규칙 | 상태 |
|---|---|
| SECURITY-05 | ✅ 인용/인자 분리 |
| SECURITY-15 | ✅ degrade-on-failure |
| SECURITY-13 | N/A — 이 크레이트는 역직렬화 경계가 아님 |
| PBT-02/03 | N/A — 순수 함수 없음(전부 I/O) |
| 커버리지 | ⏸ 테스트 1개(macOS 게이트)뿐 — `#H5-f` |

## 알려진 공백
- `PermissionChecker` 부재 → FR-10.2 / AC-13 미충족 (`#B6`, `#D-42`). 접근성 권한이 없을 때 사용자는 **원인을 알 수 없이** 창 목록이 앱 단위로 축소되는 것만 본다.

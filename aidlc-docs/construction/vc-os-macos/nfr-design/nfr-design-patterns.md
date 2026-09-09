# NFR 설계 패턴 — U3 vc-os-macos (as-built)

단계: CONSTRUCTION — U3, NFR Design · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

| 패턴 | 적용 | 목적 |
|---|---|---|
| **Graceful degradation** | 접근성 권한 부재 시 `list_running_windows()`가 창 정보 없는 앱 목록으로 하강 → 프론트가 "창 0개 = 앱 단위 항목"으로 일관 처리 | FR-2.8의 단일 창 일관성 |
| **Shell-out isolation** | 모든 OS 상호작용을 `run_osascript`/`run_jxa` 두 함수로 집중 | SECURITY-11(OS 명령 실행 로직 격리) |
| **Quote-at-boundary** | 셸 명령 조립 시 경계에서 인용(`shell_quote`) | SECURITY-05 |
| **Best-effort raise** | `AXRaise` 실패를 오류로 승격하지 않음 | NFR-R1 |
| **Opaque handle** | 창 핸들 = `name\u{1f}title` — 상위 계층이 파싱하지 않음 | FR-11.4(비영속 식별자) |

**미적용(백로그)**: 호출 타임아웃 상한, 결과 캐시(현재 매 폴링마다 전량 재질의), 권한 상태 검사.

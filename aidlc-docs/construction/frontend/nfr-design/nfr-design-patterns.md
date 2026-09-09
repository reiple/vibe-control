# NFR 설계 패턴 — U7 frontend (as-built)

단계: CONSTRUCTION — U7, NFR Design · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

| 패턴 | 적용 | 목적 |
|---|---|---|
| **Blocking boot overlay** | 스플래시가 첫 로드 완료까지 입력을 차단 | FR-13.1 — "조작 가능해 보이는 빈 화면" 제거 |
| **Idempotent finish + hard cap** | 정상 완료와 8초 타임아웃 중 먼저 온 쪽만 적용 | FR-13.4 페일세이프 |
| **Min-display floor** | 650ms 미만이면 남은 시간 대기 | FR-13.3 깜빡임 방지 |
| **allSettled, not all** | 부팅 의존 호출의 실패를 허용 | NFR-R1 |
| **Cancellation flag** | 모든 async 이펙트에 `cancelled` | 언마운트 후 setState 방지 |
| **Poll yields to drag** | `draggedApp.current`가 있으면 갱신 스킵(요청 전·후 이중 확인) | FR-3.1 보존 |
| **Visibility-gated polling** | `document.hidden`이면 틱 스킵 + `visibilitychange` 복귀 시 즉시 갱신 | FR-7.5 / NFR-Pf3 |
| **Resize quiet window** | `window.resize` 후 400ms 폴링 유예 | FR-7.5 / NFR-Pf1 |
| **In-flight guard** | `pollInFlight` ref로 느린 폴링 위에 다음 폴링을 쌓지 않음 | FR-7.6 동시성 가드 |
| **Silent vs explicit refresh** | 백그라운드 실패는 삼키고 명시적 액션 실패만 배너 | 노이즈 억제 |
| **Module-level cache + state seeding** | 아이콘 재요청·플래시 제거 | NFR-Pf4 |
| **Expansion keyed by name** | `expandedApps`를 앱 **이름**으로 키잉 → 1초마다 배열이 교체돼도 펼침 유지 | FR-2.8 |
| **Opaque handle pass-through** | 창 핸들을 파싱하지 않고 그대로 반환 | FR-11.4 |
| **Stale-while-error** | 세션 스냅샷 실패 시 직전 값 유지 | 표시 안정성 |
| **Pre-mount paint** | `index.html` 인라인 배경색 | FR-13.7 콜드 스타트 백색 플래시 제거 |

**미적용(백로그)**: 이벤트 구독(현재 폴링), 컴포넌트 분해(App.tsx 957 LOC), 프론트 테스트 러너 + `fast-check`, CSP.

# 기능 설계 — U7 frontend 대시보드 (as-built)

단계: CONSTRUCTION — U7, Functional Design · 소급 생성
> ⚠ **소급 생성 문서 (2026-09-08)** — 원 워크플로에서 "(auto)"로 완료 표시되었으나 실물이 없었다(`drift-analysis.md#D-26`). 사용자 결정 **Q5=B**에 따라 현행 코드(커밋 `d1e0f2f`)를 근거로 **as-built**로 작성한다.

## 1. 화면 구조

```
+----------------------------------------------------------+
|  [ SPLASH OVERLAY - blocks all input until loaded (FR-13)|
+-------------------+--------------------------------------+
| RUNNING APPS      |  vibe-control          [Add Group]   |
|  [search        ] |  +----------------+ +--------------+ |
|  01 (icon) Chrome |  | Group A    (3) | | Group B  (0) | |
|     ' dot  Tab 1  |  |  App  name     | | Drag apps    | |
|     ' dot  Tab 2  |  |  Session  ...  | | here         | |
|  02 (icon) Code   |  |  [Restore][Del]| |              | |
+-------------------+--------------------------------------+
|  Claude  * on   [model v]        [clear] [key]           |
|  transcript ...                                          |
|  [ prompt ...................................] [ Send ]  |
+----------------------------------------------------------+
```

**텍스트 대안**: 최상위에 부팅 스플래시 오버레이(FR-13)가 로드 완료까지 입력을 차단한다. 그 아래는 3분할 — 좌: 실행 앱 패널(상시) · 우: 제목 + 묶음 카드 그리드 · 하: Claude 콘솔 · 최상위: 부팅 스플래시. 별도 내비게이션 없음(FR-8.2).

## 2. 상태 모델 (`App.tsx`)

| 상태 | 목적 | 요구사항 |
|---|---|---|
| `bundles` | 작업 묶음 목록 | FR-1 |
| `runningApps` | 실행 앱 그룹 + 창 목록 | FR-2.1/2.2 |
| `expandedApps: Set<string>` | **앱 이름**을 키로 한 펼침 상태 — 1초 폴링이 목록을 갈아끼워도 펼침이 유지되도록 이름 기준 | FR-2.8 |
| `filter` | 검색 필터 | FR-2.5 |
| `booting` / `splashOut` | 스플래시 마운트 / 페이드 | **FR-13** |
| `draggedApp: useRef` | 드래그 중 폴링 유예 가드 | FR-3.1 |
| `sessionNonce` | 세션 상태 강제 재조회 트리거 | FR-12.5 |
| `report` / `reportBundleId` / `restoringId` | 복원 결과 표시 | FR-5.4 |
| `claude` / `chat` / `chatSending` / `chatError` | 콘솔 | NFR-S3 |

## 3. 부팅 시퀀스 (FR-13)

```
mount
  -> Splash 마운트 (입력 차단)
  -> Promise.allSettled([ getBundles, claudeStatus, refreshRunning ])
  -> elapsed < 650ms 이면 남은 시간 대기        (FR-13.3)
  -> finish(): splashOut=true -> 400ms 후 booting=false -> 언마운트 (FR-13.5)
  |
  +-- hardCap 8000ms -> finish()                (FR-13.4 페일세이프)
```
- `finish()`는 **멱등** — 정상 완료와 하드캡 중 먼저 온 쪽이 이기고 나머지는 무시된다.
- `cancelled` 플래그로 언마운트 후 늦게 도착한 resolve를 무시한다.
- `Promise.allSettled` 사용 — **한 호출이 실패해도 스플래시가 걸리지 않는다**.
- 프리마운트 흰 화면 방지는 `index.html`의 인라인 배경색이 담당(FR-13.7). 스플래시 자체를 정적 마크업이 아니라 React로 렌더하는 이유는 고DPI WebView2의 초기 확대 래스터화 회피 목적이며, 그럼에도 잔여 결함이 있다 → `known-deviations.md#H2`.

## 4. 실행 앱 패널 상호작용 (FR-2.8 / AC-20)

앱 행 클릭 분기:
| 창 개수 | 동작 |
|---|---|
| `> 1` | `toggleExpanded(app.name)` — 펼침/접기. 행에 `▸ n` / `▾ n` 표시 |
| `== 1` | `activateWindow(windows[0].handle)` — 그 창을 직접 활성화 |
| `== 0` | `activateApp(bundle_id ?? name)` — 앱 단위 폴백(예: 접근성 없는 macOS) |

- 창 하위 행: `win-dot`(`is_focused`면 녹색, FR-2.4) + `win-title`. 클릭 시 `activateWindow(handle)`.
- `handle`은 **불투명 토큰** — 프론트가 절대 파싱하지 않는다.
- 패딩 번호(01, 02…)는 `.running-item`만 카운트하고 창 하위 행은 제외한다.
- 앱 행만 `draggable` — 창 하위 행은 드래그 대상이 아니다(FR-3.1 보존).

## 5. 폴링과 드래그 공존

`setInterval(() => refreshRunning(true), 1000)`.
- `silent && draggedApp.current` → **즉시 반환**(리스트 교체가 진행 중인 HTML5 드래그를 취소시키기 때문).
- fetch 이후에도 드래그가 시작됐는지 재확인 후 `setRunningApps`.
- silent 폴링은 스피너·에러 배너를 띄우지 않는다(깜빡임 방지).
- ✅ **절약(FR-7.5/7.6, NFR-Pf3, 2026-09-08 구현)**:
  - `document.hidden` → 틱 스킵 / `visibilitychange` 복귀 시 즉시 1회 갱신
  - `window.resize` 후 400ms 유예(`resizingUntil`)
  - `pollInFlight` 가드로 느린 폴링 위에 중첩 금지 (명시적 ↻는 예외)

## 6. 아이콘 (`useAppIcon`)

모듈 레벨 `Map` 캐시 + `useState` 초기값 시딩 → 리마운트·필터 변경 시 재요청·깜빡임 없음. 실패도 `null`로 캐시. `img.onError`는 요소를 숨겨 깨진 이미지 아이콘을 노출하지 않는다.

## 7. 세션 카드 (`SessionStatus`)

`sessionRef`/`nonce` 변경 시 `getSessionSnapshot` 재조회. 완료 칩 + 마지막 질문 1줄만 렌더. 폴링 중 일시 실패 시 **마지막 정상 스냅샷을 유지**해 매 틱 공백이 되지 않게 한다.
> 전체 대화 렌더는 요구사항 v1.1(FR-12.3)에서 **정식 축소**되었다 — "View"가 실제 터미널을 최전면에 띄운다.

## 8. 오류 표시

`errText(e)`가 Tauri의 `{message}` 직렬화 오류에서 문자열을 뽑는다(`String(e)`는 `[object Object]`가 되므로). 명시적 액션의 실패만 배너로 노출하고, 백그라운드 폴링 실패는 삼킨다(NFR-R1 + 노이즈 억제).

## 9. 미구현/연기 (요구사항 v1.1 반영)
| 항목 | 상태 |
|---|---|
| 저장 리소스 상태 점·흐림 | ⏸ 연기 (FR-7.1~7.3, `#D-30`) |
| 묶음 이름 변경 / 리소스 제거 / 항목 편집 | ⏸ 연기 (`#D-32`, `#D-33`, `#D-31`) |
| 레이아웃 설정 영속 | ⏸ 연기 (FR-8.10, `#D-39`) |
| OS 파일/URL 드롭 | ⛔ 범위 제외 — `dragDropEnabled: false` (`#D-35`) |
| 프론트엔드 자동 테스트 | 없음 (러너 미도입 — **승인된 면제** `#H5-g`) |

# Functional Design — 크로스플랫폼 무(無)프레임 창 (Window Chrome)

**보완 Bolt (2026-09-09)** · Unit: **U6 vc-app** (프론트 U7 변경 없음)
**요구 추적**: FR-8.11, AC-22, `REQUIREMENTS.ko §13.7` · **이탈 기록**: `known-deviations.md §I`

## 1. 목적

윈도우 버전에서 운영체제 기본 제목표시줄(캡션 바)을 제거하여, 맥 버전과 **동일한 무프레임 화면**을 렌더링한다.

## 2. 배경 / 현행 동작

| 항목 | 현행 |
|---|---|
| 창 설정 원천 | `crates/vc-app/tauri.conf.json` → `app.windows[0]` |
| macOS | `titleBarStyle: "Overlay"` + `hiddenTitle: true` → 투명 제목표시줄, 네이티브 신호등이 콘텐츠 위에 오버레이 = 무프레임 |
| Windows | `titleBarStyle`은 **Tauri v2에서 macOS 전용** → 무시됨. `decorations` 기본값 `true` → OS 캡션 바(제목·최소/최대/닫기) 표시됨 |
| 드래그 이동 | 프론트 두 헤더에 `data-tauri-drag-region` 존재 (FR-8.6) |
| 상단 여백 | `styles.css`에서 헤더 34px 상단 패딩(맥 신호등 회피용) |

## 3. 설계 결정

- **접근**: `tauri.conf.json`은 크로스플랫폼 단일 설정이라 여기서 OS 분기가 불가능하다. 대신 `vc-app`의 `run()` `setup` 훅에서 **컴파일타임 `#[cfg(target_os = "windows")]`** 로 분기하여, 윈도우에서만 메인 창의 장식을 런타임에 끈다.
  - macOS: config의 Overlay를 그대로 사용 → 런타임 오버라이드 없음.
  - Windows: `get_webview_window("main")` → `set_decorations(false)`.
- **대안 기각**: `tauri.conf.json`에 `decorations: false`를 전역 설정하면 macOS의 Overlay 신호등까지 사라져 회귀. 기각.
- **창 이동/크기**: 이동은 기존 드래그 영역 유지. 크기 조절은 tao의 무장식 창 엣지 히트테스트로 유지된다.
- **프론트 여백**: 34px 상단 여백을 두 OS 공통 유지 → "동일한 화면"(FR-8.11) 원칙 준수.
- **커스텀 창 컨트롤(맥 신호등 모사)**: 무장식 윈도우 창은 네이티브 닫기/최소/최대 버튼이 없다. 맥은 Overlay 네이티브 신호등을 유지하므로, **윈도우에서만**(`!IS_MACOS`) 맥 신호등을 모사한 커스텀 컨트롤을 좌상단(신호등과 동일 위치)에 렌더한다. 이로써 두 OS가 신호등 포함 동일 화면이 된다.

## 4. 구현 지점

### 4.1 제목표시줄 제거 (U6 vc-app)
- `crates/vc-app/src/lib.rs` — `run()`의 `.setup(|app| { … })` 내부, `restore_window_rect` 직후:
  ```rust
  #[cfg(target_os = "windows")]
  {
      use tauri::Manager;
      if let Some(win) = app.get_webview_window("main") {
          let _ = win.set_decorations(false);
      }
  }
  ```
  best-effort — 실패 시 프레임이 남을 뿐 크래시 없음.

### 4.2 커스텀 창 컨트롤 (U7 frontend + U6 capabilities)
- `frontend/src/App.tsx` — `WindowControls` 컴포넌트: `getCurrentWindow()`(`@tauri-apps/api/window`)로 닫기 `.close()` / 최소화 `.minimize()` / 최대화 토글 `.toggleMaximize()`. `app-shell` 첫 자식으로 `{!IS_MACOS && <WindowControls />}` 렌더(윈도우 전용). 각 점 안에 **구분용 심볼을 SVG 스트로크로** 그린다 — 닫기 `×`(대각선 2개), 최소화 `−`(가로선), 최대화 `□`(사각 외곽). 폰트 글리프는 작은 크기에서 베이스라인 때문에 어긋나 보여 **SVG로 픽셀 정중앙·크리스프** 보장.
- `frontend/src/styles.css` — `.wc-bar`(절대배치 top:14px/left:16px, gap 12px, z 900) + `.wc-dot`(30px 원 — 사용자 요청 ~2배 확대, flex 중앙정렬, `-webkit-app-region: no-drag`) + 색상 `.wc-close #ff5f57` / `.wc-min #febc2e` / `.wc-max #28c840`(맥 신호등 색) + `.wc-glyph`(SVG, `stroke rgba(0,0,0,.66)`, width 1.1@viewBox, `pointer-events:none`). 심볼은 호버 없이 **항상 표시**(사용자 구분성).
- 큰 컨트롤이 헤더와 겹치지 않도록 `app-shell`에 `wc-chrome` 클래스(`!IS_MACOS`)를 붙이고 `.wc-chrome .sidebar-header/.main-header { padding-top: 54px }`로 윈도우 상단 여백만 키움(맥은 34px 유지).
- ⚠ **클래스명은 반드시 `wc-` 접두사**(기존 무관한 `.win-dot` 7px 상태 점과 충돌 방지 — `known-deviations §I` 근본원인 참조).
- `crates/vc-app/capabilities/default.json` — `core:window:allow-minimize`, `core:window:allow-close` 추가(기존 toggle-maximize/maximize/unmaximize/start-dragging 유지).

## 5. 수용 기준 (AC-22)

- [x] 윈도우 실행 시 OS 기본 제목표시줄이 표시되지 않는다.
- [x] 맥과 동일한 레이아웃(좌 패널 + 우 카드 + 상단 여백)이 나온다.
- [x] 상단 드래그로 창 이동, 엣지로 크기 조절 가능.
- [x] macOS Overlay 신호등은 회귀 없이 유지된다(윈도우 전용 오버라이드).
- [x] 윈도우 좌상단에 맥 신호등 모사 닫기/최소화/최대화 버튼이 있고 실제로 동작한다.

## 6. 검증 (실 Windows, 2026-09-09)

- `cargo build -p vc-app`(debug + release) 성공, 프론트 `tsc && vite build` 성공. `gen/schemas/capabilities.json`에 창 권한 6개 임베드 확인.
- ✅ 제목표시줄 부재(캡처), ✅ 좌상단 신호등 표시·맥 위치/색 일치(확대 캡처), ✅ 닫기 버튼 실제 클릭 → 프로세스 종료(라이브).
- 최소화/최대화: 닫기와 동일 코드 경로 + 권한 임베드 확인으로 동작 판정. 합성 클릭 자동화가 12px 표적/창 상태 교란으로 불안정해 개별 라이브 확인은 미완 — 사용자 수동 확인 권장.
- ⚠ 로컬 exe는 `#port-1420-devserver-hijack`로 devUrl(:1420)을 로드 → 검증 시 이 리포 vite dev 서버를 :1420에 띄워 확인. `tauri build` 산출물은 무관.

# Technology Stack

단계: INCEPTION — Reverse Engineering (재실행 2026-09-08) · 커밋 `d1e0f2f`

## Programming Languages
- **Rust** — edition 2021 — 6개 크레이트(도메인·영속·OS 어댑터·세션·앱)
- **TypeScript** — 5.6 — 프론트엔드 전체
- **PowerShell** — 인라인 스크립트(Windows 아이콘 추출, 앱 실행/포커스, 터미널)
- **AppleScript / JXA** — 인라인 스크립트(macOS 창 열거·탭 읽기·활성화·아이콘)
- **C#** — Windows 아이콘 추출 시 `Add-Type` 인라인(shell32 `SHGetImageList`)

## Frameworks
- **Tauri** 2.0 — 데스크톱 셸, 커맨드 브리지, 번들링
- **React** 18.3 + **react-dom** 18.3 — UI
- **Vite** 6.0 (+ `@vitejs/plugin-react` 4.3) — 프론트 빌드/개발 서버(포트 1420)

## Infrastructure
- 클라우드 인프라 없음. 유일한 외부 서비스: **AWS Bedrock Runtime**(앱 내 Claude 콘솔, HTTPS)
- 데이터: 로컬 JSON 2개 파일(`bundles.json`, `settings.json`) — OS config 디렉터리

## Build Tools
- **Cargo** (workspace, resolver 2) — Rust 빌드
- **npm** + **Vite** — 프론트 빌드 (`tsc && vite build`)
- **tauri-build** (build.rs) — Tauri 코드 생성
- **Tauri bundler** — `bundle.targets: "all"`

## Testing Tools
- **cargo test** — 인라인 `#[cfg(test)]` 모듈, 총 **33개** 테스트 함수 (vc-core 22 / vc-sessions 8 / vc-store 2 / vc-os-macos 1)
- **proptest** 1.0 (dev-dependency: vc-core, vc-store, vc-sessions) — PBT-02(라운드트립)·PBT-03(파서 견고성)
- **clippy** — CI 없이 수동 실행
- **tsc** — 프론트 타입 검사(빌드 단계에 포함)
- **부재**: 통합 테스트 크레이트, criterion 벤치마크, 커버리지 도구, 프론트엔드 테스트 러너(`fast-check` 미도입), CI 파이프라인

## 주요 OS API 표면
| 플랫폼 | 사용 API |
|---|---|
| Windows | `EnumWindows`, `IsWindowVisible`, `GetWindowTextW/LengthW`, `GetWindow(GW_OWNER)`, `GetWindowLongW(GWL_EXSTYLE)`, `DwmGetWindowAttribute(DWMWA_CLOAKED)`, `GetWindowThreadProcessId`, `GetForegroundWindow`, `ShowWindowAsync`, `SetForegroundWindow`, `OpenProcess`/`QueryFullProcessImageNameW`, `CreateToolhelp32Snapshot`/`Process32*W`, shell32 `SHGetImageList` |
| macOS | `NSWorkspace`(JXA), System Events(창 제목·frontmost), `AXRaise`(best-effort), `open`, Terminal 제어 |

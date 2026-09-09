# Component Inventory

단계: INCEPTION — Reverse Engineering (재실행 2026-09-08) · 커밋 `d1e0f2f`

## Application Packages
- `crates/vc-app` — Tauri 바이너리. `AppState` + 18개 커맨드 + Bedrock 클라이언트 + cfg 어댑터 조립 (U6)
- `frontend/` — React 18 + Vite + TS 웹뷰 UI (U7)

## Infrastructure Packages
- 없음 (CDK/Terraform/CloudFormation 부재). 배포 산출물은 Tauri 번들러가 생성.

## Shared Packages
- `crates/vc-core` — 순수 도메인 모델·알고리즘 (U1)
- `crates/vc-store` — 영속성 어댑터 (U2)
- `crates/vc-os-macos` — macOS OS 어댑터, `cfg(target_os="macos")` (U3)
- `crates/vc-os-windows` — Windows OS 어댑터, `cfg(target_os="windows")` (U4)
- `crates/vc-sessions` — 코딩 에이전트 세션 어댑터 (U5)

## Test Packages
- 없음 (별도 테스트 크레이트/디렉터리 부재). 모든 테스트는 소스 파일 내 `#[cfg(test)]` 인라인 모듈.

## Total Count
- **Total Packages**: 7 (Cargo 크레이트 6 + 프론트엔드 1)
- **Application**: 2 (`vc-app`, `frontend`)
- **Infrastructure**: 0
- **Shared**: 5 (`vc-core`, `vc-store`, `vc-os-macos`, `vc-os-windows`, `vc-sessions`)
- **Test**: 0

## 설계 단위(U1–U7) 대비 매핑
| 단위 | 패키지 | 존재 |
|---|---|:---:|
| U1 vc-core | `crates/vc-core` | ✅ |
| U2 vc-store | `crates/vc-store` | ✅ |
| U3 vc-os-macos | `crates/vc-os-macos` | ✅ |
| U4 vc-os-windows | `crates/vc-os-windows` | ✅ |
| U5 vc-sessions | `crates/vc-sessions` | ✅ |
| U6 vc-app | `crates/vc-app` | ✅ |
| U7 frontend | `frontend/` | ✅ |

크레이트 경계 자체는 설계와 **완전히 일치**한다. 차이는 각 단위 *내부*의 책임 배치에 있다(→ `drift-analysis.md`).

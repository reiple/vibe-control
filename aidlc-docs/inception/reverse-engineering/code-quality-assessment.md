# Code Quality Assessment

단계: INCEPTION — Reverse Engineering (재실행 2026-09-08) · 커밋 `d1e0f2f`

> ⚠ **검증 한계**: 이 분석 세션은 WSL/Linux 환경이며 `cargo`가 설치되어 있지 않다. 따라서 아래 수치는 **정적 분석(파일·grep·읽기)** 결과이며, `cargo build/test/clippy` 결과는 **이 세션에서 재실행되지 않았다**. 빌드·테스트 통과 여부는 `aidlc-state.md`/`audit.md`에 기록된 실 Windows 환경의 이전 실행 기록에 근거한다.

## Test Coverage

- **Overall**: **Fair** — 도메인 코어는 테스트가 있으나, 커버리지 도구가 없어 수치 미측정
- **테스트 함수 총계**: **33개** (정적 카운트)

| 크레이트 | 테스트 함수 | 비고 |
|---|---:|---|
| vc-core | 22 | bundle 2, matching 2, window 2, restore 2, evaluate 2, normalize 6, migrate 4 + proptest 2 |
| vc-sessions | 8 | 파서/판정 6 + proptest 2 (`prop_parser_robust`, `prop_lines_robust`) |
| vc-store | 2 | 임시 디렉터리 격리 사용 |
| vc-os-macos | 1 | macOS 게이트 |
| vc-os-windows | **0** | 782 LOC, 테스트 없음 |
| vc-app | **0** | 795 LOC, 테스트 없음 |
| frontend | **0** | 957 LOC App.tsx, 테스트 러너 자체 없음 |

- **Unit Tests**: 있음(인라인 `#[cfg(test)]`)
- **Integration Tests**: **없음** — `tests/` 디렉터리 부재. 설계가 요구한 "OS 어댑터 모킹 통합 테스트"는 어댑터가 트레이트가 아니어서 모킹 자체가 불가능한 구조
- **PBT**: proptest 2곳 — `vc-core/src/migrate/mod.rs`(`prop_roundtrip_stable`=PBT-02, `prop_parser_robust`=PBT-03), `vc-sessions/src/lib.rs`(`prop_parser_robust`, `prop_lines_robust`). **`ProptestConfig` 미설정 → 기본 256 케이스**(설계 요구는 1000회)
- **E2E**: 수동 체크리스트(`build-and-test/`)만 존재, 체크박스 전부 미완료 상태

## Code Quality Indicators

- **Linting**: clippy를 수동 실행한 기록은 있으나 **설정 파일·CI 강제 없음**(`clippy.toml`/`rustfmt.toml`/`.github/` 모두 부재). 프론트는 ESLint 미설정(`tsc`만)
- **Code Style**: **Consistent** — 명명·모듈 구성·에러 처리 패턴이 크레이트 전반에서 일관됨
- **Documentation**: **Good** — 공개 타입·함수에 doc comment가 충실하며, 다수 주석이 FR/AC/NFR ID를 직접 인용해 추적성이 높다(예: `RunningWindow`의 "FR-2.8 / AC-20"). 주석이 설계 의도와 트레이드오프를 남긴 점이 특히 강점

## Technical Debt

| # | 항목 | 위치 | 영향 |
|---|---|---|---|
| TD-1 | 도메인 코어 미배선 — `matching`/`restore`/`evaluate`/`normalize`가 vc-app에서 호출되지 않음 | `vc-app/src/lib.rs:13` import 목록 | 테스트된 도메인 로직이 런타임 동작에 기여하지 않음. 매칭·상태판정 기능 공백 |
| TD-2 | 포트 트레이트 부재 → cfg 기반 구체 타입 직접 호출 | `vc-app/src/lib.rs` 자유 함수 전반 | 어댑터 모킹 불가 → 통합 테스트 불가 |
| TD-3 | `save_settings`가 비원자적 `fs::write` | `vc-store/src/lib.rs:108` | 저장 중 크래시 시 설정 파일 손상 가능 |
| TD-4 | `save()` 실패 시 `.tmp` 파일 미정리 | `vc-store/src/lib.rs:80` | 임시 파일 누수 |
| TD-5 | `WorkBundle::add_resource`에 중복 방지 불변식 없음 | `bundle.rs:82` | 불변식이 `vc-app` 커맨드 인라인에만 존재 → 다른 경로로 우회 가능 |
| TD-6 | `deny_unknown_fields`·깊이/크기 제한 없음 | `vc-core/src/models/*`, `migrate/mod.rs` | 설계가 명시한 SECURITY-13 강화가 미구현 |
| TD-7 | `security.csp: null` | `tauri.conf.json:25` | 설계가 요구한 CSP 미적용 |
| TD-8 | 미사용 의존성 `sha2` | `vc-core/Cargo.toml:13` | 공급망 표면 불필요 확대 |
| TD-9 | `WinWindowEnumerator::list_running()` 미사용 공개 API | `vc-os-windows/src/lib.rs:307` | 죽은 코드 |
| TD-10 | `reqwest` 주석이 "Anthropic Messages API"로 표기(실제는 Bedrock) | `vc-app/Cargo.toml` | 오해 유발 주석 |
| TD-11 | 프론트 DTO 타입이 백엔드와 수동 미러링 | `frontend/src/types.ts` | 계약 드리프트 위험(자동 생성 없음) |
| TD-12 | `App.tsx` 957 LOC 단일 컴포넌트(스플래시·패널·카드·콘솔·모달 전부) | `frontend/src/App.tsx` | 변경 위험·테스트 곤란 |
| TD-13 | CI/커버리지/의존성 감사 파이프라인 부재 | 리포 전역 | 회귀 방지 자동화 없음 |
| TD-14 | 1초 무조건 폴링(가시성/리사이즈 절약 없음) | `App.tsx:349` | NFR-Pf3의 절약 규정 미구현 |

## Patterns and Anti-patterns

### Good Patterns
- **최소 blast radius 리팩터링**: G1 수정 시 `raw_windows()`만 교체하고 상위 계약 전부 보존 — 실제 회귀 없이 결함 해소
- **의존성 없는 네이티브 FFI**: `windows`/`winapi` 크레이트 없이 필요한 심볼만 `#[link]` — 1초 폴링에 적합한 무-서브프로세스 경로
- **락 밖에서 느린 작업**: `get_app_icon`이 캐시 확인 → 락 해제 → 추출 → 락 재획득. 실패(`None`)도 캐싱
- **부분 실패 지속**: `restore_bundle`이 실패해도 계속 진행하고 `RestoreReport`로 종합 (AC-11 충족)
- **비밀 취급**: Bedrock 토큰이 UI로 절대 반환되지 않고(`ClaudeStatus`는 존재 여부만), 에러 메시지에도 미포함
- **손상 허용 파싱**: `parse_session_bytes`가 임의 바이트에도 패닉하지 않고 부분 결과 반환 (AC-18)
- **드래그 중 폴링 유예**: `draggedApp.current` 가드로 라이브 갱신과 HTML5 DnD 공존
- **추적 가능한 주석**: 코드 주석이 FR/AC 번호를 직접 인용

### Anti-patterns
- **Anemic core / 우회된 도메인**: 도메인 알고리즘이 존재하고 테스트까지 되지만 오케스트레이션이 사용하지 않음 (TD-1). 테스트 통과가 실제 동작을 보증하지 않는 상태
- **Compile-time cfg를 DI 대신 사용**: 어댑터 교체·모킹 불가 (TD-2)
- **God component**: `App.tsx` 단일 파일에 모든 UI 관심사 (TD-12)
- **Shell-out 의존**: macOS 전량 + Windows 아이콘/실행이 `osascript`/PowerShell 서브프로세스 — 지연·인용 규칙·환경 의존. (Windows 열거만 FFI로 탈출 성공)
- **불변식의 위치 이탈**: 도메인 규칙이 애플리케이션 계층 인라인 조건문으로 존재 (TD-5)
- **설정 저장의 비대칭**: 묶음은 원자적, 설정은 비원자적 (TD-3)

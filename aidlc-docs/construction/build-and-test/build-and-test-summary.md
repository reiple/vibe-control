# Build & Test 요약

## 개요
vibe-control는 7개 Cargo 크레이트로 구성된 Tauri 데스크톱 앱입니다. 이 문서는 전체 빌드 및 테스트 전략을 요약합니다.

## 테스트 계층
| 계층 | 도구 | 대상 | 문서 |
|---|---|---|---|
| 빌드 | cargo build | 모든 크레이트 | build-instructions.md |
| 단위 | cargo test --lib | Domain 로직 (vc-core 중심) — **실측 33개 테스트 함수**: vc-core 22 / vc-sessions 8 / vc-store 2 / vc-os-macos 1 · **vc-app·vc-os-windows·frontend는 0개** | unit-test-instructions.md |
| 통합 | cargo test --test | 단위 간 상호작용 — ⛔ **미구현**. `tests/` 디렉터리가 없고, 어댑터가 트레이트가 아니라 컴파일타임 `cfg` 디스패치라 **모킹 지점 자체가 존재하지 않는다**(`#A1`, `drift-analysis.md#D-24`) | integration-test-instructions.md |
| PBT | proptest | 라운드트립/파서 견고성 | pbt-test-instructions.md |
| E2E | 수동 체크리스트 | 실제 OS 창/브라우저/세션 | (수동, 아래 참조) |

## 빌드 순서 (Foundation-First)
```
vc-core (기반) → vc-store → vc-os-macos ‖ vc-os-windows → vc-sessions → vc-app → frontend
```

## 확장 준수 검증

> ⚠ **실측 갱신 2026-09-08 (정합화 재실행)** — "필수/차단"으로 적혀 있던 항목의 **실제 이행 상태**를 반영한다(`inception/reverse-engineering/drift-analysis.md`).

| 규칙 | 검증 방법 | 요구 | **실제 (커밋 `d1e0f2f`)** |
|---|---|---|---|
| SECURITY-05 (입력 정규화) | normalize 단위 테스트 + PBT | 필수 | ⚠ **부분** — `normalize_*`는 구현·테스트되었으나 `vc-app`이 **호출하지 않음**(`#A4`). 셸 인용은 적용됨 |
| SECURITY-13 (안전한 역직렬화) | `deny_unknown_fields` + 손상 파일 테스트 | 필수 | ⏸ **미이행** — `deny_unknown_fields` 0건, 깊이·크기 상한 없음. 손상 파일 테스트는 존재 (`#H5-a`) |
| SECURITY-15 (페일세이프) | Store 오류 처리 + 부분 실패 지속 | 필수 | ✅ 이행 |
| SECURITY-04 (CSP) | `tauri.conf.json` | 적용 | ⏸ **미이행** — `csp: null` (`#H5-b`) |
| PBT-02 (라운드트립) | proptest **1000회** | 차단 | ⚠ **케이스 수 미달** — 테스트는 존재하나 `ProptestConfig` 미설정으로 기본 **256회** (`#H5-c`) |
| PBT-03 (파서 견고성) | proptest 손상 입력 | 차단 | ⚠ 동상 — vc-core 1건 + vc-sessions 2건 존재, 256회 |
| PBT-08 (시드/CI) | 시드 보존 + CI 통합 | 차단 | ⏸ **미이행** — CI 자체 부재 (`#H5-d`) |
| PBT-09 (프레임워크) | proptest + fast-check | 차단 | ⚠ **부분** — proptest ✅ / 프론트 `fast-check` 미도입 (`#H5-g`) |

> ✅ **확정 (2026-09-08)**: 위 ⏸/⚠ 항목은 **승인된 면제(waiver)** 로 처리되었다 — `known-deviations.md#H-5`(수용 리스크·재검토 트리거 포함). 확장 설정도 조정되었다: **Security = Full + 예외 2건(SECURITY-04, SECURITY-13 강화 조항)**, **PBT = Advisory(권고)**. 따라서 이들은 더 이상 빌드/테스트 게이트의 **차단 사유가 아니다**. 단 PBT-02/03/07 테스트 자체는 **계속 유지·통과해야 한다**(면제된 것은 실행횟수·시드/CI·프론트 프레임워크뿐).

## E2E 수동 체크리스트 (실제 OS)
- [ ] **부팅 스플래시 (FR-13 / AC-21, 신설 2026-09-08)**
  - [ ] 앱 실행 직후 스플래시가 뜨고, 그 아래 UI를 클릭해도 아무 동작이 없다
  - [ ] 묶음·Claude 상태·첫 실행앱 목록이 모두 채워진 뒤 스플래시가 걷힌다
  - [ ] 빠른 기동에서도 번쩍임 없이 최소 650ms 유지된다
  - [ ] (페일세이프) 백엔드가 지연되어도 8초 내에 반드시 걷힌다
  - [ ] ⚠ **고DPI 회귀 확인**: 고DPI Windows에서 스플래시 카드가 확대되어 보이는 알려진 결함(`known-deviations.md#H2`) 상태를 기록한다
- [ ] **창 단위 펼침/접기 (FR-2.8 / AC-20)**
  - [ ] 창이 여러 개인 앱(Edge/Chrome/카카오톡)이 `▸ n`으로 표시되고 클릭 시 창 목록이 펼쳐진다
  - [ ] 개별 창을 클릭하면 **정확히 그 창**이 최전면에 온다
  - [ ] 포커스된 창에만 녹색 점이 표시된다
  - [ ] 창을 닫으면 다음 갱신에서 목록에서 사라진다
- [ ] **폴링 절약 (FR-7.5/7.6 · NFR-Pf3, 2026-09-08 구현)**
  - [ ] 앱 창을 최소화하거나 다른 창으로 완전히 가리면 실행앱 폴링이 멈춘다(작업 관리자에서 CPU/호출 감소 확인)
  - [ ] 다시 앱으로 돌아오면 **즉시** 목록이 갱신된다(최대 1초 기다리지 않음)
  - [ ] 창 크기를 드래그로 조절하는 동안 목록이 튀거나 버벅이지 않는다
  - [ ] 드래그로 리소스를 등록하는 동안 목록이 교체되지 않는다(회귀 확인)
- [ ] 앱 실행 → 현재 열린 창/앱 목록 캡처
- [ ] 번들 저장 → 앱 재시작 → 번들 로드 확인
- [ ] 번들 복원 → 앱/창/폴더 재오픈 확인
- [ ] 브라우저 탭 캡처 (Safari/Chrome/Edge)
- [ ] Claude Code 세션 대화 표시 확인
- [ ] 권한 요청 흐름 (Accessibility/Automation) 확인

## 성능 목표
| 항목 | 목표 |
|---|---|
| 창 열거 | < 500ms |
| 번들 저장 | < 100ms |
| 번들 로드 | < 100ms |
| 세션 파싱 | < 1s per session |

## 완료 기준

| 기준 | 상태 (2026-09-08 재검증) |
|---|---|
| 전체 빌드 성공 (경고 0) | ✅ 기록됨 (`cargo build --workspace`, `clippy --all-targets` 0 경고 — 실 Windows) |
| 단위 테스트 100% 통과 | ✅ 기록됨 (33개) |
| **커버리지 ≥90%** | ⏸ **미검증** — 커버리지 도구 부재. vc-app/vc-os-windows/frontend는 테스트 0개 (`#H5-f`) |
| PBT-02/03 차단 규칙 통과 | ⚠ 통과하나 **실행 횟수 256 < 요구 1000** (`#H5-c`) |
| E2E 체크리스트 수동 검증 완료 | ⏸ 체크박스 미완료 상태로 남아 있음 |
| 성능 목표(창 열거<500ms 등) | ⏸ **미측정** — 벤치마크 하네스 없음 (`#H5-e`) |

> ⚠ **본 세션 검증 한계**: 정합화 재실행 환경(WSL/Linux)에 `cargo`가 없어 위 ✅ 항목을 **재실행하지 못했다**. 이전 실 OS 실행 기록의 인용이다.

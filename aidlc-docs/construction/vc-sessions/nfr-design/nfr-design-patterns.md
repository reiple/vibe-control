# NFR 설계 패턴 — U5 vc-sessions (as-built)

단계: CONSTRUCTION — U5, NFR Design · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

| 패턴 | 적용 | 목적 |
|---|---|---|
| **Lenient line parser** | JSONL을 줄 단위로 처리하고 실패 줄은 스킵 — 전체 실패로 승격하지 않음 | AC-18, SECURITY-13/15 |
| **Total function** | `parse_session_bytes`가 `Result`가 아니라 `SessionSnapshot`을 직접 반환 — 실패 개념 자체를 제거 | 패닉·오류 전파 경로 축소 |
| **Three-state instead of boolean** | `SessionCompletion`에 `Unknown`을 두어 단정 회피 | FR-12.4 |
| **Registry (plugin point)** | `SessionProviderRegistry` + `Box<dyn CodingSessionProvider>` | FR-12.1 도구 확장성 |
| **Read-only by construction** | 쓰기 API 미제공 | FR-12.6, AC-19 |
| **Property-based robustness** | 임의 입력 proptest 2건 | PBT-03 |

**미적용(백로그)**: 파일 크기 상한/스트리밍(대용량 세션 로그), 변경 감지(mtime) 기반 재파싱 회피, `deny_unknown_fields`.

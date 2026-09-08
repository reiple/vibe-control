# NFR 요구사항 — U5 vc-sessions (as-built)

단계: CONSTRUCTION — U5, NFR Requirements · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

## 성능
- **목표**: 세션 1건 파싱 < 1s (Build&Test 목표). 세션 스냅샷은 카드가 보일 때 + `sessionNonce` 변경 시에만 조회하므로 폴링 부하는 카드 수에 비례한다.
- **상한**: 캡처 시 최신 **8건**으로 절단(상한 자체는 U6의 `MAX_CAPTURED_SESSIONS`).
- **미측정**: 벤치마크 없음 → `#H5-e`.

## 신뢰성 (이 단위의 핵심 NFR)
- **패닉 금지**: `parse_session_bytes`는 **임의 바이트열에서도 패닉하지 않고** 항상 유효한 `SessionSnapshot`을 반환한다. 손상·부분 JSONL 라인은 건너뛰고 나머지를 보존한다.
- **추측 금지**: 판별 불가한 완료여부는 `Unknown`, 확보하지 못한 대화는 비워 둔다(FR-12.4/12.7).
- **읽기 전용**: 쓰기 API가 아예 없어 AC-19가 **구조적으로** 보장된다.

## 보안
| 규칙 | 처리 |
|---|---|
| **NFR-S1** | 로컬 파일만 읽는다. 이 크레이트에 네트워크 의존성 없음(`Cargo.toml`: vc-core, serde, serde_json, dirs) |
| **SECURITY-13** | 안전 역직렬화 — 라인 단위 파싱 + 실패 라인 스킵. ⏸ 단, `deny_unknown_fields`·크기/깊이 상한은 미적용(`#H5-a`) |
| SECURITY-03 / FR-12.9 | 대화 내용을 로그로 출력하지 않는다 |

## 확장 준수
| 규칙 | 상태 |
|---|---|
| **PBT-03 파서 견고성** | ✅ `prop_parser_robust`(임의 바이트) + `prop_lines_robust`(임의 라인) |
| PBT-02 | N/A — 이 단위는 직렬화 왕복 경계가 아님 |
| PBT-07 생성기 품질 | ✅ 정상/부분/손상 라인 혼합 생성 |
| PBT-02/03 **1000회** | ⏸ 미충족 — `ProptestConfig` 미설정(기본 256) `#H5-c` |
| PBT-08 시드/CI | ⏸ 미충족 `#H5-d` |
| SECURITY-13 강화 | ⏸ 미충족 `#H5-a` |
| 커버리지 | 테스트 8개 — 이 프로젝트에서 vc-core 다음으로 양호 |

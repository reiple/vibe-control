# 코드 생성 계획 — U5 vc-sessions (as-built)
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — 이 계획은 사전 계획이 아니라 **as-built 역작성**이다. `[x]`는 현행 코드(커밋 `d1e0f2f`)에서 실제로 확인된 항목, `[ ]`는 계획 대비 **미이행으로 남은 항목**이다.

### Step 1 구조
- [x] `crates/vc-sessions/` + `Cargo.toml`(vc-core, serde, serde_json, dirs; dev: proptest)

### Step 2 계약
- [x] `trait CodingSessionProvider { tool_id, discover, read_snapshot }`
- [x] `SessionInfo` / `SessionSnapshot` / `Turn`
- [x] `SessionProviderRegistry { new, register, provider }` — 도구 확장점 (FR-12.1)
- [ ] 원 설계상 포트 위치는 **vc-core**였음 — 실제로는 이 크레이트 → `#A1`

### Step 3 Claude Code provider
- [x] `~/.claude/projects/**/*.jsonl` 탐색(`dirs`)
- [x] `parse_session_bytes(&[u8]) -> SessionSnapshot` — 라인 단위, 손상 라인 스킵, **패닉 없음·항상 유효 반환**
- [x] 마지막 질문 추출 / 완료여부 3상태 판정(Unknown 포함)
- [x] `resume_info(session_ref) -> Option<(cwd, id)>`
- [x] 쓰기 API 미제공 → AC-19 구조적 보장

### Step 4 테스트
- [x] 파싱·판정 단위 테스트 6
- [x] PBT: `prop_parser_robust`(임의 바이트) · `prop_lines_robust`(임의 라인) — PBT-03/PBT-07
- [ ] `ProptestConfig` 1000회 → ⏸ `#H5-c`
- [ ] 시드 보존/CI → ⏸ `#H5-d`

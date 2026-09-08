# 기능 설계 — U5 vc-sessions (as-built)

단계: CONSTRUCTION — U5, Functional Design · 소급 생성
> ⚠ **소급 생성 문서 (2026-09-08)** — 이 산출물은 원 워크플로에서 `aidlc-state.md`에 "(auto)"로 완료 표시되었으나 **실물이 존재하지 않았다**(`drift-analysis.md#D-26`). 사용자 결정 **Q5=B**에 따라 **현행 코드(커밋 `d1e0f2f`)를 근거로 소급 작성**한다. 즉 이 문서는 *사전 설계*가 아니라 **구현된 사실의 설계 기술(as-built)** 이며, 원 설계 의도와의 차이는 `known-deviations.md`에 남아 있다.

## 1. 책임

코딩 에이전트의 **로컬 세션 파일을 읽기 전용으로** 열람해 (전체 대화 · 마지막 질문 · 답변 완료여부)를 제공한다. 네트워크 호출 없음(NFR-S1). 도구별 확장은 provider 추가로 이뤄진다(FR-12.1).

## 2. 계약

```rust
pub trait CodingSessionProvider {
    fn tool_id(&self) -> &str;
    fn discover(&self) -> Result<Vec<SessionInfo>>;
    fn read_snapshot(&self, session_ref: &str) -> Result<SessionSnapshot>;
}
```

| 타입 | 필드 | 비고 |
|---|---|---|
| `SessionInfo` | label, session_ref, tool_id 등 | `discover()`가 최신순으로 반환 |
| `SessionSnapshot` | `conversation: Vec<Turn>`, `last_question: Option<String>`, `completion: SessionCompletion`, `available: bool` | `available=false`면 UI가 상태를 숨김 |
| `Turn` | role, content | 요구사항 v1.1에서 **인앱 렌더는 제외**(FR-12.3 축소)되었으나 계약에는 유지 — 되돌릴 수 있게 |
| `SessionCompletion` | Waiting / NotWaiting / **Unknown** | FR-12.4: 판별 불가 시 단정하지 않고 Unknown |

`SessionProviderRegistry`: `new()`가 기본 provider(Claude Code)를 등록, `register()`로 확장, `provider(tool_id)`로 조회. **본 프로젝트에서 포트/DI 형태가 유일하게 살아있는 지점**이다.

## 3. ClaudeCodeSessionProvider

- **세션 위치**: `~/.claude/projects/<프로젝트>/*.jsonl` (`dirs`로 홈 해석)
- **파싱**: `parse_session_bytes(&[u8]) -> SessionSnapshot` — 줄 단위 JSONL. **손상·부분 라인은 건너뛰고 계속** 진행하며, 어떤 입력에도 패닉하지 않는다(FR-12.7, AC-18, SECURITY-13). 반환값은 항상 유효한 스냅샷.
- **마지막 질문**: 마지막 assistant 턴에서 사용자 응답을 기다리는 질문 라인을 추출.
- **완료여부**: 마지막 턴의 role/형태로 Waiting / NotWaiting 판정, 애매하면 Unknown.
- **`resume_info(session_ref) -> Option<(cwd, id)>`**: 세션 파일 경로에서 작업 디렉터리와 세션 UUID를 복원 — `vc-app`이 `claude --resume <id>` 명령과 터미널 포커스 힌트를 만들 때 사용.
- **쓰기 없음**: 이 크레이트는 세션 파일을 열기만 하며 수정·삭제 API가 없다(FR-12.6, AC-19 — 구조적으로 보장).

## 4. 비즈니스 규칙

| 규칙 | 내용 | 근거 |
|---|---|---|
| BR-S1 | 확보하지 못한 대화 내용을 추측해 채우지 않는다 | FR-12.7, cf. FR-9.7 |
| BR-S2 | 판별 불가한 완료여부는 Unknown (기본값 단정 금지) | FR-12.4 |
| BR-S3 | 손상 입력에서 오류를 던지기보다 **가능한 범위를 보존**한다 | AC-18, SECURITY-15 |
| BR-S4 | 대화 내용을 로그로 출력하지 않는다 | FR-12.9, SECURITY-03 |
| BR-S5 | 세션 파일에 **쓰지 않는다** | FR-12.6, AC-19 |

## 5. 캡처 상한

`vc-app`이 캡처 시 `MAX_CAPTURED_SESSIONS = 8`로 최신순 절단한다(NFR-Pf2 탐색 상한). 상한 자체는 U6에 위치.

## 6. 테스트 (as-built)

`crates/vc-sessions/src/lib.rs` 인라인 8개 — 파싱/판정 6 + proptest 2(`prop_parser_robust` 임의 바이트, `prop_lines_robust` 임의 라인). PBT-03 대응.

## 7. 원 설계와의 차이
- 원 설계는 포트 `P4`를 **vc-core**에 두었으나 실제 트레이트는 **vc-sessions**에 있다 → `known-deviations.md#A1`.
- PBT-03의 위치가 설계(`vc-core/tests/pbt_*.rs`)와 다르다 → `#D5`(2026-09-08 정정 반영).

# 기능 설계 — 세션 제어 클러스터 (Context Control · 인앱 터미널/PTY · CloudWatch 사용량)

단계: CONSTRUCTION — 사후 문서화(post-hoc) · 유닛: U2(Status Query) / U3(Command Delivery) / vc-app
상태: **구현 완료** (origin/main 병합 클러스터 — 커밋 `f4540b9`·`4bca8d1`·`3cd305f`)
정합화 유래: **Integration Drift Audit + delta 정합화(2026-09-09)**. 전체 Reverse Engineering 재실행이 아니라, origin/main 병합 후 코드에 존재하나 요구/설계 문서에 없던 델타만 감사·문서화한다. RE 스냅샷(커밋 `d1e0f2f`)은 이 클러스터 병합 **이전**이므로 RE 산출물은 손대지 않는다.
정합화 기준 워크트리: **vc-integration**(브랜치 `bolt/tab-window-integration`) — 배선 판정·검증 수치는 모두 이 워크트리의 실제 코드/실행 결과 기준.
요구 대응: `requirements.md` FR-15 / FR-16 / FR-17 / NFR-S4 · AC-24/25/26
이탈 등록: `known-deviations.md#F2`(요약의 Bedrock 외부 전송) · A2(커맨드 인벤토리 현행화) · E4(미배선 래퍼)

> **주의 — NFR-S1/NFR-S4 관계**: FR-15의 작업 요약은 세션 대화 발췌를 AWS Bedrock으로 **외부 HTTPS 전송**하므로 NFR-S1(세션 데이터 자동 외부 전송 금지)의 예외다. 이 전송은 **사용자 명시 동의가 없으면 0건**이며(단일 초크 포인트 `should_dispatch`), NFR-S4·`known-deviations.md#F2`로 규정한다. §5 참조. 인앱 터미널/PTY(FR-16)·CloudWatch 사용량(FR-17)은 이 예외와 무관하다 — 앱이 담당하는 PTY 제어·입출력 전달은 전부 로컬이고, CloudWatch는 정수 토큰 수만 회수한다. (단 PTY 안에서 도는 claude-CLI 자체의 모델 호출·인증은 CLI 정책을 따르며 앱 관할 밖 — FR-16.4)

---

## 1. 목적 / 범위

병합으로 함께 도입된 세 갈래를 하나의 as-built 설계로 묶는다(구조 불필요 확대 방지):

1. **Context Control (FR-15)** — 세션 실행 상태 5-상태 판별, 출처(Provenance) 표기 작업 요약(Bedrock, 동의 게이트), 명령 전달 결정 코어(busy-protection).
2. **인앱 세션 터미널 / 대화형 PTY (FR-16)** — claude-CLI를 앱 내부 PTY로 구동, 외부 PowerShell 터미널, xterm.js 렌더.
3. **계정 CloudWatch 사용량 (FR-17)** — 당일 계정 전체 Bedrock 토큰 사용량 조회.

FR-12(로컬 세션 파일 읽기 전용 열람)를 확장하되 대체하지 않는다.

## 2. 구성요소

| 위치 | 역할 | 갈래 |
|---|---|---|
| `crates/vc-core/src/claude_status.rs` | `SessionRunState`(5-상태) · `Provenance` · `WorkItem`/`WorkSummary` · `IDLE_THRESHOLD=30s` · 순수 `derive_run_state` | FR-15 |
| `crates/vc-core/src/summarize.rs` | 포트 `WorkSummarizer`/`ProcessLivenessProbe`, `SummarizationRequest`/`SummaryTurn`, `RESUMMARIZE_THROTTLE_SECS=30`. "이 크레이트에서 LLM 호출은 결코 일어나지 않는다" | FR-15 |
| `crates/vc-core/src/deliver.rs` | U3 순수 결정 코어 `select_delivery_target` — 모호/외부/Working 세션에는 전달 금지 | FR-15 |
| `crates/vc-core/src/models/analysis_cache.rs` | `AnalysisCache`(BTreeMap) → `analysis-cache.json`(비핵심 캐시, NFR-1.2) | FR-15 |
| `crates/vc-app/src/status_query.rs` | U2 어댑터: `SystemProbe`(liveness), `CloudWorkSummarizer`(Bedrock), 순수 게이트 `should_dispatch`, `build_session_status`, `build_summary_prompt`, `outcome_from_reply`. `MAX_SUMMARY_TURNS=40` | FR-15 |
| `crates/vc-app/src/pty.rs` | 대화형 claude-CLI PTY(`portable_pty`+`vt100`), 선택 프롬프트 감지(`DetectedPrompt`), `ROWS=45`/`COLS=120`. PTY 제어·입출력은 전부 로컬(§12 유지) | FR-16 |
| `crates/vc-app/src/term.rs` | 외부 PowerShell 터미널, 세션==터미널, PID 레지스트리. 로컬 | FR-16 |
| `crates/vc-app/src/usage_cw.rs` | `AccountUsage{input,output,calls}`, `fetch_today(region,since,now)` — CloudWatch `GetMetricData` SEARCH, 8초 타임아웃 | FR-17 |
| `crates/vc-app/src/lib.rs` | `AppState` + 신규 Tauri 커맨드(§4), 모듈 선언 `claude`/`pty`/`status_query`/`term`/`usage_cw` | 전체 |
| `frontend/src/{GroupTerminal.tsx, App.tsx, api.ts}` | 대화형 PTY 소비(xterm.js) + 사용량 미터. Context Control·외부 터미널 래퍼는 §6 참조 | FR-16/FR-17 |

## 3. 도메인 계약 (순수 코어)

- **`derive_run_state(completion, is_running, since_last_activity) -> SessionRunState`**: `Waiting`→`WaitingForUser`(실행 중), 프로세스 없음→`Inactive`, liveness 불명→`Unknown`, 활동 후 30초 경과→`Idle`, 그 외 진행→`Working`. 순수·전역(패닉 없음).
- **`should_dispatch(consent, key_present, throttle_entry, now, has_new_content, in_flight_already) -> bool`**: 4개 가드 AND — `consent_allows(consent,key_present)` ∧ `!in_flight_already` ∧ `should_resummarize(...)`. 이 함수가 **유일한 프라이버시 초크 포인트**로, "무동의 ⇒ 외부 호출 0" "불변 ⇒ 호출 0"을 정적으로 참으로 만든다.
- **`select_delivery_target(...)`**: 모호/외부(foreign)/`Working` 세션에는 전달 대상을 반환하지 않는다(busy-protection, FR-15.6).
- **`outcome_from_reply` / `parse_summary_json`**: 오류·파싱 불가·빈 응답 → `Insufficient`로 강등(부분 추측 금지).

## 4. Tauri 커맨드 (신규분)

> ⚠ 줄 번호는 구조적으로 낡는다 — 심볼 이름을 1차 기준으로 참조하라. 전체 커맨드 인벤토리 수치(현행 **53**, `lib.rs`의 `generate_handler!` 등록 직접 카운트)는 `aidlc-state.md` 통합 감사 섹션과 `known-deviations.md#A2`에 현행화한다.

| 커맨드 | 역할 | 갈래 |
|---|---|---|
| `get_context_claude_status` | 세션 실행 상태·작업 요약 조회 | FR-15 |
| `get_summarization_consent` / `set_summarization_consent` | 요약 동의 조회/설정(기본 비동의, `settings.session_summary_consent`) | FR-15.5 |
| `send_command_to_context_claude` / `send_command_to_session` | 대상 세션으로 명령 전달(busy-protection) | FR-15.6 |
| `claude_usage` | 당일 계정 Bedrock 토큰 사용량(CloudWatch→로컬 폴백) | FR-17 |
| `start_new_coding_session` / `delete_coding_session` | 코딩 세션 생성/삭제(로컬 파일) | FR-16 |
| `open_session_terminal` / `open_new_session_terminal` / `send_to_session_terminal` / `close_session_terminal` / `session_terminal_open` / `list_open_session_terminals` | 외부 세션 터미널 개폐·전송·목록(term.rs) | FR-16 |
| `start_interactive_session` / `start_new_interactive` / `submit_interactive_line` / `interactive_screen` / `send_interactive_key` / `send_interactive_text` / `resize_interactive` / `stop_interactive_session` | 대화형 PTY 세션(pty.rs) | FR-16 |

## 5. 프라이버시 / 동의 (Security Baseline 재평가)

| 항목 | 처리 |
|---|---|
| 동의 게이트 | 요약 전송은 `should_dispatch`의 4가드 통과 시에만. 무동의 ⇒ 외부 호출 0건(단위 테스트로 불변 검증). 동의는 로컬 설정에만 저장 |
| 전송 데이터 | 최신 최대 40턴 + 직전 요약만(최소 발췌). 회수는 요약 JSON 뿐. 토큰·프롬프트·응답 원문은 UI/로그 미노출(NFR-S3와 동일) |
| 전송 구간 | HTTPS only(bedrock-runtime), 토큰 `Authorization: Bearer`(SECURITY-01/12) |
| 터미널/PTY | 앱이 담당하는 PTY 제어·세션 입출력 전달은 전부 로컬 — 앱은 세션 데이터를 외부로 보내지 않는다. PTY 안에서 도는 claude-CLI 자체의 모델 호출·인증은 CLI 정책을 따르며 앱 관할 밖(FR-16.4) |
| CloudWatch | 정수 토큰 수만 회수. 표준 AWS 자격증명(`cloudwatch:GetMetricData`) 필요, 없으면 로컬 폴백. 8초 타임아웃으로 UI 미지연 |

## 6. 검증 (§검증 — 실행 기준, PASS/보류)

> 이 절이 구현 현황·검증 결과·orphaned 상태의 **정본**이다(요구 문서는 요구·인수 기준만 둔다). 검증 원칙: **테스트 수는 grep이 아니라 실제 `cargo test`/프론트 빌드 실행 결과로 기록**하며, 검증하지 않은 항목을 PASS로 적지 않는다. 배선(WIRED/PARTIAL/ORPHANED) 판정은 vc-integration의 `App.tsx`·`GroupTerminal.tsx` 실제 `invoke` 호출 기준이다.

**실행 결과(2026-09-09, vc-integration 워크트리):**
- `cargo test --workspace` → **120 passed / 0 failed** (vc-app 18, vc-core 77, vc-os-windows 5, vc-sessions 12, vc-store 8; vc-os-macos는 Windows에서 0 — macOS 전용, 미실행).
- 프론트엔드 `tsc` **EXIT 0** + `vite build` **EXIT 0**(`dist/assets/index-Dll3aGdj.js` 488.42 kB, gzip 135.31 kB; 43 modules).

**갈래별 상태(배선 판정 — vc-integration 실측 caller 수):**

| 갈래 | 백엔드 | 단위 검증 | 프론트 배선 | end-to-end |
|---|---|---|---|---|
| FR-15 Context Control | 구현 완료 | ✅ `should_dispatch`(무동의 0호출·스로틀)·`derive_run_state`(5-상태)·`outcome_from_reply`(강등)·`select_delivery_target`(busy-protection) 단위 통과 | ❌ **미배선(ORPHANED)** — `getContextClaudeStatus`/`sendCommandToContextClaude`/`sendCommandToSession`/`get·setSummarizationConsent` api.ts 래퍼가 `App.tsx`·`GroupTerminal.tsx`에서 **0회 참조** | ❌ 미검증 |
| FR-16 인앱 터미널/PTY | 구현 완료 | ✅ 관련 vc-app 단위 통과 | ⚠ **부분 배선(PARTIALLY WIRED)** — 아래 경로별 표 참조 | 대화형 PTY 경로만 배선 존재(실사용 런타임 확인은 사용자 환경) |
| FR-17 CloudWatch 사용량 | 구현 완료 | (I/O 경로 — 단위 테스트 없음, 로직 소규모) | ✅ **배선(WIRED)** — `claudeUsage` 3회 참조, 사용량 미터 상시 렌더 | ✅ 배선 경로 존재(실 계정 자격증명 유무는 사용자 환경) |

**FR-16 경로별 배선(PARTIALLY WIRED 세부):**

| 경로 | 커맨드/래퍼 | vc-integration caller 수 | 판정 |
|---|---|---|---|
| 대화형 PTY(핵심 루프) | `startInteractiveSession`(4)·`interactiveScreen`(4)·`submitInteractiveLine`(3)·`startNewInteractive`(2)·`sendInteractiveText`(2)·`resizeInteractive`(2)·`onPtyOutput`(2) | 다수 참조 | ✅ **WIRED** (`GroupTerminal.tsx` xterm.js) |
| 대화형 PTY(미사용 래퍼) | `sendInteractiveKey`(0)·`stopInteractiveSession`(0) | 0 | 미배선(래퍼만 존재) |
| 외부 세션 터미널(term.rs) | `openSessionTerminal`/`openNewSessionTerminal`/`sendToSessionTerminal`/`closeSessionTerminal`/`sessionTerminalOpen`/`listOpenSessionTerminals` | 전부 0 | ❌ **ORPHANED** |
| 코딩 세션 생성/삭제 | `startNewCodingSession`/`deleteCodingSession` | 전부 0 | ❌ **ORPHANED** |

**정직성 노트**: FR-15(Context Control)는 백엔드가 구현·단위 검증되었으나 **소비 UI가 없다(ORPHANED)**. FR-16은 **대화형 PTY만 배선**되고 외부 터미널·세션 생성/삭제 경로는 미배선(PARTIALLY WIRED). FR-17(사용량)은 래퍼 3회 참조 + 미터 상시 렌더로 **배선(WIRED)** 이다. 터미널 중심 재설계(커밋 `4bca8d1`)가 Context Control 소비 UI를 대체했다. 미배선 경로는 기존 `known-deviations.md#E3`(orphaned api 래퍼) 계열과 동일 성격이며, **end-to-end PASS로 기록하지 않는다**.

> ⚠ **코드-문서 불일치(주석) — 정합화 보고 대상, 코드 무수정**: `crates/vc-app/src/lib.rs`의 `RunEvent::Exit` 핸들러(현행 커밋 기준 라인 2791 부근) 주석 `// none — the PTY path is unused by the current UI.`는 **현행 integrated 코드와 불일치하는 stale comment**다. 실제로는 `GroupTerminal.tsx`가 `startInteractiveSession`→`pty::start`(`claude --resume`)로 대화형 PTY를 구동하므로(위 표 WIRED) "PTY path가 현재 UI에서 미사용"은 사실이 아니다. 본 정합화는 **문서 정합화이므로 주석은 수정하지 않고** 여기와 최종 보고에 명시한다.

## 7. Requirement → Design → Code → Validation 추적

| 요구 | 설계(본 문서) | 코드 | 검증 |
|---|---|---|---|
| FR-15.1 5-상태 | §3 `derive_run_state` | `vc-core/claude_status.rs` | 단위 ✅(§6) · UI ORPHANED ❌ |
| FR-15.2 출처 표기 | §3 `WorkSummary`/`Provenance` | `vc-core/claude_status.rs` | 단위 ✅ · UI ORPHANED ❌ |
| FR-15.3/NFR-S4 동의 게이트 전송 | §3 `should_dispatch`·§5 | `vc-app/status_query.rs`(`CloudWorkSummarizer`,`should_dispatch`) | 단위 ✅(무동의 0호출) · UI ORPHANED ❌ |
| FR-15.4 실패 강등 | §3 `outcome_from_reply` | `vc-app/status_query.rs` | 단위 ✅ |
| FR-15.5 동의 설정 | §4 커맨드 | `get·set_summarization_consent` | 백엔드 존재, UI ORPHANED ❌ |
| FR-15.6 명령 전달 | §3 `select_delivery_target` | `vc-core/deliver.rs` | 단위 ✅ · UI ORPHANED ❌ |
| FR-16.1/16.2 대화형 PTY | §2·§4 | `vc-app/pty.rs` | 배선 ✅(§6 핵심 루프) |
| FR-16.3 외부 터미널 | §2·§4 | `vc-app/term.rs` | 백엔드 존재, UI ORPHANED ❌ |
| FR-16.4 세션 생성/삭제(로컬) | §4 커맨드 | `start_new_coding_session`/`delete_coding_session` | 백엔드 존재, UI ORPHANED ❌ |
| FR-17.1~17.3 사용량 | §2·§5 | `vc-app/usage_cw.rs` | 배선 ✅(§6, `claudeUsage` 3회·미터 상시) |

## 8. 미결 / 후속 (열린 항목)

- FR-15(Context Control) 소비 UI 배선 — 별도 프론트 작업(백로그). 배선 전까지 end-to-end 미검증으로 유지.
- FR-16 미배선 경로(외부 세션 터미널 term.rs · `start_new_coding_session`/`delete_coding_session`) UI 배선 — 백로그.
- FR-17 CloudWatch 경로 단위/통합 테스트 부재(I/O 경계 — 완료 기준상 통합 테스트 면제와 동일 사유).
- `lib.rs` `RunEvent::Exit` 주석의 stale "PTY path is unused" 문구 정리(코드 백로그 — 이번 문서 정합화 범위 밖).

## 9. 관련
- 요구: `inception/requirements/requirements.md`(FR-15/16/17, NFR-S4, AC-24/25/26)
- 상태·통합 감사: `aidlc-docs/aidlc-state.md`(통합 감사 섹션·Decision/Integration 기록)
- 이탈: `known-deviations.md#F2`(요약 외부 전송)·A2(커맨드 인벤토리)·E4/E3(orphaned 래퍼)
- 인접: `construction/vc-app/claude-console/design.md`(같은 Bedrock 경로), `functional-design/orchestration.md`(vc-app 커맨드 as-built)

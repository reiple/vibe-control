# 기능 설계 — U6 vc-app 오케스트레이션 (as-built)

단계: CONSTRUCTION — U6, Functional Design · 소급 생성
> ⚠ **소급 생성 문서 (2026-09-08)** — 이 산출물은 원 워크플로에서 `aidlc-state.md`에 "(auto)"로 완료 표시되었으나 **실물이 존재하지 않았다**(`drift-analysis.md#D-26`). 사용자 결정 **Q5=B**에 따라 **현행 코드(커밋 `d1e0f2f`)를 근거로 소급 작성**한다. 즉 이 문서는 *사전 설계*가 아니라 **구현된 사실의 설계 기술(as-built)** 이며, 원 설계 의도와의 차이는 `known-deviations.md`에 남아 있다.
> 별도 문서: 앱 내 Claude 콘솔은 `../claude-console/design.md` 참조.
>
> 🔗 **정합화 노트 (2026-09-09 · Integration Drift Audit, vc-integration 기준)** — 아래 §1·§3의 "18개 커맨드" 표는 커밋 `d1e0f2f` 기준의 **역사적 as-built 스냅샷이며 수정하지 않는다**. 그 이후 origin/main 병합으로 세션 제어 커맨드군(Context Control·인앱 터미널/PTY·CloudWatch 사용량)과 창/탭·권한 커맨드가 추가되어 현재 커맨드 수는 **53개**(현행 `lib.rs`의 `generate_handler!` 등록 직접 카운트)로 늘었다. 세션 제어 신규 커맨드군의 설계·목록·추적·검증은 `../session-control/design.md`에, 현행 커맨드 인벤토리 수치는 `aidlc-state.md` 통합 감사 섹션과 `known-deviations.md#A2`에 둔다. 본 문서는 병합 이전 스냅샷으로 보존한다.

## 1. 책임

프론트엔드의 유일한 백엔드 경계. 상태(`AppState`)를 보유하고, 도메인·영속·세션·OS 어댑터를 조립해 **18개 Tauri 커맨드**로 노출한다.

## 2. AppState

```rust
pub struct AppState {
    store: Box<dyn BundleStore + Send>,   // 유일한 DI 지점
    bundles: Vec<WorkBundle>,             // 메모리 사본 (권위는 store)
    settings: AppSettings,                // 레이아웃 4필드는 현재 미사용(FR-8.10 연기)
    icon_cache: HashMap<String, Option<String>>, // None도 캐시 (NFR-Pf4)
}
```
- `new()`: store 생성 → `load()` → `load_settings().unwrap_or_default()`. **손상된 설정이 앱을 벽돌로 만들지 않는다**(SECURITY-15).
- 전역 공유: `type SharedState = Mutex<AppState>`. 락 획득 실패는 `"state lock poisoned"` 오류로 변환(패닉 없음).

## 3. 커맨드 표면 (18)

| 그룹 | 커맨드 | 동기/비동기 | 요구사항 |
|---|---|---|---|
| 묶음 | `get_bundles`, `create_bundle`, `delete_bundle`, `add_app_resource`, `save_bundles`* | sync | FR-1.1(생성/삭제), FR-1.2, FR-3.1/3.4 |
| 캡처 | `capture_current`* | async | (고아 — `#E3`) |
| 실행 목록 | `list_running_apps` | **async** | FR-2.1/2.2/2.8 |
| 활성화 | `activate_window`, `activate_app`, `restore_bundle` | **async** | FR-4.1/4.2, FR-5.1~5.4 |
| 아이콘 | `get_app_icon` | sync (락 밖 추출) | FR-2.3, §13.5, NFR-Pf4 |
| 세션 | `get_session_snapshot`, `resume_coding_session`, `activate_coding_session` | **async** | FR-12.3(개정)/12.8 |
| Claude 콘솔 | `claude_status`, `set_claude_api_key`, `set_claude_model`, `send_claude_message` | 앞 3개 sync / 마지막 async | NFR-S3 |

\* 프론트가 호출하지 않는 고아 커맨드.

**비동기 규칙(핵심)**: OS를 건드리는 커맨드는 반드시 `async` — 동기 커맨드는 Tauri의 메인 스레드에서 실행되어 UI를 얼린다. 이는 2026-09-08 Windows 프리즈 사고의 재발 방지 결정이다(`audit.md`).

## 4. 어댑터 디스패치

`open_app` / `focus_window` / `open_path` / `open_url` / `resume_session` / `activate_session_terminal` / `enumerate_running_apps_detailed` / `enumerate_running_windows` / `extract_app_icon` — 각각 `#[cfg(target_os="macos")]` / `#[cfg(target_os="windows")]` / 그 외(오류 또는 빈 값) 3분기의 자유 함수.

`type AppWindows = (String, Option<String>, Vec<(String, String, bool)>)` — 어댑터↔앱 간 창 그룹 계약.

## 5. 비즈니스 규칙 (as-built)

| 규칙 | 내용 | 위치 | 근거 |
|---|---|---|---|
| BR-A1 | 같은 묶음에 동일 `(AppLaunch, descriptor)`를 두 번 등록하지 않는다 | `add_app_resource` 인라인 | FR-3.4, AC-3 |
| BR-A2 | 복원은 **저장 순서대로**, 실패해도 중단하지 않고 `RestoreReport`로 종합 | `restore_bundle` | FR-5.2/5.3/5.4, AC-11 |
| BR-A3 | 복원 시 **코딩 세션은 건너뛴다**(`skipped`) — 이미 열린 터미널이 있어도 새로 띄우는 부작용을 막기 위함. 재개는 개별 커맨드로만 | `restore_bundle` | FR-12.8 |
| BR-A4 | 재실행 대상은 `reopen_info` 우선, 없으면 `descriptor` | `reopen_resource` | FR-4.3, FR-9.3 |
| BR-A5 | 아이콘 추출은 **락을 놓고** 수행하고, 실패(`None`)도 캐시해 재시도하지 않는다 | `get_app_icon` | NFR-Pf1/Pf4 |
| BR-A6 | Bedrock 키는 **UI로 반환하지 않는다**. `ClaudeStatus`는 존재 여부·출처·모델·리전만 | `claude_status_of` | NFR-S3, SECURITY-12 |
| BR-A7 | 네트워크 호출 전에 락을 해제한다(std `MutexGuard`는 `Send`가 아니며, 다른 커맨드를 막아서도 안 됨) | `send_claude_message` | NFR-Pf1 |
| BR-A8 | 셸 명령에 넣는 경로는 플랫폼별로 인용한다(`shell_quote` / `ps_single_quote`) | `resume_session` | SECURITY-05, NFR-S2 |

## 6. 원 설계와의 차이 (요약)
서비스 S1–S7 · `TauriCommandBridge` · `RefreshScheduler` · 이벤트 emit이 **존재하지 않으며**, 도메인 `matching`/`restore`/`evaluate`/`normalize`가 **호출되지 않는다** → `known-deviations.md#A2`, `#A3`, `#A4`. 이는 [코드백로그]로 유지된다.

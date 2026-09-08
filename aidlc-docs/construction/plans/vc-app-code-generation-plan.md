# 코드 생성 계획 — U6 vc-app (as-built)
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — 이 계획은 사전 계획이 아니라 **as-built 역작성**이다. `[x]`는 현행 코드(커밋 `d1e0f2f`)에서 실제로 확인된 항목, `[ ]`는 계획 대비 **미이행으로 남은 항목**이다.

### Step 1 구조
- [x] `crates/vc-app/` — `main.rs`, `lib.rs`, `claude.rs`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, 아이콘
- [x] Cargo: tauri 2.0, reqwest 0.13(no-default + json + default-tls), serde, serde_json, 내부 5개 크레이트
- [ ] `reqwest` 주석이 "Anthropic Messages API"로 오기 → `#H4`

### Step 2 상태
- [x] `AppState { store: Box<dyn BundleStore + Send>, bundles, settings, icon_cache }`
- [x] `Mutex<AppState>`를 Tauri `manage()`로 등록, 락 poisoning → `CommandError`
- [x] 손상 설정 `unwrap_or_default()` (SECURITY-15)

### Step 3 커맨드 (18)
- [x] 묶음: `get_bundles`, `create_bundle`, `delete_bundle`, `add_app_resource`, `save_bundles`\*
- [x] 캡처: `capture_current`\*
- [x] 실행/활성화: `list_running_apps`, `activate_app`, `activate_window`, `restore_bundle`
- [x] 아이콘: `get_app_icon` (락 밖 추출 + negative cache)
- [x] 세션: `get_session_snapshot`, `resume_coding_session`, `activate_coding_session`
- [x] Claude: `claude_status`, `set_claude_api_key`, `set_claude_model`, `send_claude_message`
- [x] **OS 접촉 커맨드는 전부 `async`** (UI 프리즈 방지)
- [ ] 리소스 **제거**/**편집**/묶음 **이름변경** 커맨드 → `#D-33`, `#D-31`, `#D-32`
- [ ] `get_settings`/`update_settings`(레이아웃 영속) → `#D-39`
- \* 프론트 미호출(고아) → `#E3`

### Step 4 어댑터 조립
- [x] `open_app`/`focus_window`/`open_path`/`open_url`/`resume_session`/`activate_session_terminal`/`enumerate_running_*`/`extract_app_icon` — cfg 3분기
- [x] `shell_quote`(mac) / `ps_single_quote`(win) 인용 헬퍼
- [ ] 포트 트레이트 기반 DI → `#A1`, `#A2`
- [ ] `vc_core::{matching, restore, evaluate, normalize}` 배선 → `#A4`

### Step 5 Bedrock 콘솔
- [x] `claude.rs::send_message(token, region, model, messages)` — HTTPS POST, 오류에 토큰 미포함
- [x] 키 해석 체인 + `ClaudeStatus`(값 미반환)
- 상세: `../vc-app/claude-console/design.md`

### Step 6 보안/미이행
- [ ] `tauri.conf.json` **CSP** → ⏸ `#H5-b`
- [ ] 입력 정규화(`normalize_*`) 배선 → `#A4` (SECURITY-05 부분 미충족)
- [ ] **테스트 0개 / 795 LOC** → ⏸ `#H5-f`

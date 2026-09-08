# API Documentation

단계: INCEPTION — Reverse Engineering (재실행 2026-09-08) · 커밋 `d1e0f2f`

## REST APIs

앱은 HTTP 서버를 노출하지 않는다. **아웃바운드 호출 1개**만 존재한다.

### Bedrock InvokeModel (아웃바운드)
- **Method**: POST
- **Path**: `https://bedrock-runtime.{region}.amazonaws.com/model/{encoded_model}/invoke`
- **Purpose**: 앱 내 Claude 프롬프트 콘솔의 응답 생성
- **Auth**: `Authorization: Bearer <Bedrock API key>` (SigV4 없음)
- **Request**: `{"anthropic_version":"bedrock-2023-05-31","max_tokens":4096,"messages":[{"role","content"}]}`
- **Response**: `{"content":[{"type":"text","text":"…"}]}` → `type=="text"` 블록을 개행 결합
- **Defaults**: model `global.anthropic.claude-opus-4-8`, region `ap-northeast-2`, timeout 120s
- **위치**: `crates/vc-app/src/claude.rs:63` `send_message`

## Internal APIs

### Tauri 커맨드 경계 (frontend ↔ vc-app) — **18개**

| # | 커맨드 | async | 시그니처 | 프론트 호출 |
|---:|---|:---:|---|:---:|
| 1 | `get_bundles` | | `() -> Vec<WorkBundle>` | ✅ |
| 2 | `save_bundles` | | `(bundles: Vec<WorkBundle>) -> ()` | ❌ 고아 |
| 3 | `capture_current` | ✅ | `(name: String) -> WorkBundle` | ❌ 고아 |
| 4 | `get_session_snapshot` | ✅ | `(tool_id, session_ref) -> SessionSnapshot` | ✅ |
| 5 | `restore_bundle` | ✅ | `(bundle: WorkBundle) -> RestoreReport` | ✅ |
| 6 | `resume_coding_session` | ✅ | `(session_ref) -> ()` | ✅ |
| 7 | `activate_coding_session` | ✅ | `(session_ref) -> ()` | ✅ |
| 8 | `list_running_apps` | ✅ | `() -> Vec<RunningApp>` | ✅ |
| 9 | `activate_app` | ✅ | `(target: String) -> ()` | ✅ |
| 10 | **`activate_window`** | ✅ | `(handle: String) -> ()` | ✅ |
| 11 | `get_app_icon` | | `(bundle_id: String) -> Option<String>` | ✅ |
| 12 | `create_bundle` | | `(name) -> Vec<WorkBundle>` | ✅ |
| 13 | `delete_bundle` | | `(bundle_id) -> Vec<WorkBundle>` | ✅ |
| 14 | `add_app_resource` | | `(bundle_id, name, target) -> Vec<WorkBundle>` | ✅ |
| 15 | `claude_status` | | `() -> ClaudeStatus` | ✅ |
| 16 | `set_claude_api_key` | | `(key: String) -> ClaudeStatus` | ✅ |
| 17 | `set_claude_model` | | `(model: String) -> ClaudeStatus` | ✅ |
| 18 | `send_claude_message` | ✅ | `(messages: Vec<ChatMsg>) -> String` | ✅ |

> 등록 위치: `crates/vc-app/src/lib.rs` `invoke_handler(tauri::generate_handler![…])`.
> 에러는 모두 `CommandError { message: String }`로 직렬화된다.

### vc-core 공개 함수

| 함수 | 시그니처 | vc-app 사용 |
|---|---|:---:|
| `match_signature` | `(&ResourceIdentity) -> MatchSignature` | ❌ |
| `distinct_key` | `(&ResourceIdentity) -> DistinctKey` | ❌ |
| `matching::window::matches` | `(&ResourceIdentity, &RunningItem) -> MatchResult` | ❌ |
| `plan_reopen` | `(&Resource) -> ReopenAction` | ❌ |
| `plan_bundle_activation` | `(&WorkBundle) -> Vec<ActivationStep>` | ❌ |
| `evaluate_status` | `(&Resource, bool, bool, bool) -> ResourceStatus` | ❌ |
| `is_noise` | `(&str) -> bool` | ❌ |
| `evaluate` | `(&[Resource], &[String], bool) -> Vec<StatusSnapshot>` | ❌ |
| `normalize_url` / `normalize_path` / `normalize_app_id` | `(&str) -> Result<String>` | ❌ |
| `migrate::load_and_migrate` / `serialize` | `(&[u8]) -> Result<Vec<WorkBundle>>` / `(&[WorkBundle]) -> Result<Vec<u8>>` | ✅ (vc-store 경유) |

### vc-store

```rust
pub trait BundleStore {
    fn load(&self) -> Result<Vec<WorkBundle>>;
    fn save(&self, bundles: &[WorkBundle]) -> Result<()>;
    fn load_settings(&self) -> Result<AppSettings>;
    fn save_settings(&self, settings: &AppSettings) -> Result<()>;
}
```

### vc-sessions

```rust
pub trait CodingSessionProvider {
    fn tool_id(&self) -> &str;
    fn discover(&self) -> Result<Vec<SessionInfo>>;
    fn read_snapshot(&self, session_ref: &str) -> Result<SessionSnapshot>;
}
```
- `SessionProviderRegistry::{new, register, provider}`
- `ClaudeCodeSessionProvider::resume_info(session_ref) -> Option<(cwd, id)>`
- `parse_session_bytes(&[u8]) -> SessionSnapshot` (손상 허용, 패닉 없음)

### OS 어댑터 (동일 형태의 mac/win 쌍)

| 어댑터 | 메서드 |
|---|---|
| `*WindowEnumerator` | `list_running() -> Vec<String>`(win은 **미사용**), `list_running_apps() -> Vec<(String, Option<String>)>`, `list_running_windows() -> Vec<AppWindows>` |
| `*BrowserTabReader` | `read_tabs() -> Vec<(title, url)>` — **Windows는 스텁(빈 벡터)** |
| `*Launcher` | `open_app`, `focus_window(handle)`, `open_path`, `open_url`, `run_in_terminal`, `activate_terminal` (+ Windows 전용 private `focus_existing_window`) |
| `*IconReader` | `icon_data_uri(target) -> Option<String>` |

> `AppWindows = (String, Option<String>, Vec<(String, String, bool)>)` — (앱 이름, bundle id, [(handle, title, is_focused)]).
> **존재하지 않음**: `PermissionChecker`(mac/win 모두), 트레이/전역 단축키 훅(win).

## Data Models

### WorkBundle
- **Fields**: `id: Uuid`, `name: String`, `resources: Vec<Resource>`
- **Relationships**: 1—N Resource
- **Validation**: **없음** — `add_resource`는 `()`를 반환하고 중복 검사 없이 push. 중복 방지는 `vc-app::add_app_resource` 안에 인라인으로만 존재

### Resource
- **Fields**: `id: Uuid`, `display_name: String`, `kind: ResourceKind`, `identity: ResourceIdentity`, `status: ResourceStatus` (`#[serde(skip)]`), `order: u32`
- **Validation**: 없음(생성 시 status=Unknown, order=0 → 묶음 추가 시 재부여)

### ResourceIdentity
- **Fields**: `kind`, `descriptor: String`(안정·영속), `hint: Option<String>`(비영속), `reopen_info: Option<String>`
- **Validation**: **없음** — `normalize_*`가 생성 경로에서 호출되지 않음

### AppSettings
- **Fields**: `panel_width: f32`, `card_height: f32`, `window_x/y: Option<i32>`, `window_width/height: Option<u32>`, `claude_api_key/claude_model/claude_region: Option<String>` (Claude 3필드는 `#[serde(default)]`)
- **Validation**: 없음. 레이아웃 필드는 **어떤 커맨드도 읽거나 쓰지 않음**(기본값만 유지)

### RunningApp / RunningWindow (DTO)
- `RunningApp { name: String, bundle_id: Option<String>, windows: Vec<RunningWindow> }`
- `RunningWindow { handle: String, title: String, is_focused: bool }` — `handle`은 불투명 토큰(Windows: HWND 10진 문자열 / macOS: `name\u{1f}title`)

### SessionSnapshot / SessionInfo / Turn (DTO)
- `SessionSnapshot { conversation: Vec<Turn>, last_question: Option<String>, completion: SessionCompletion, available: bool }` — `conversation`은 프론트로 전달되나 **렌더되지 않음**
- `SessionCompletion`: `Waiting` | `NotWaiting` | `Unknown`

### RestoreReport / ClaudeStatus / ChatMsg (DTO)
- `RestoreReport { opened: Vec<String>, failed: Vec<String>, skipped: Vec<String> }`
- `ClaudeStatus { configured: bool, source: "settings"|"env"|"none", model: String, region: String }` — 키 값은 **절대 미반환**
- `ChatMsg { role: String, content: String }`

### 영속 스키마 (`bundles.json`)
```json
{ "version": 1, "bundles": [ { "id": "...", "name": "...", "resources": [...] } ] }
```
- 미래 버전(>1)은 오류 반환. `deny_unknown_fields` **미적용**, 깊이·크기 제한 **없음**.

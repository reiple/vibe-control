# 컴포넌트 메서드 (Component Methods)

단계: INCEPTION — Application Design
표기: 러스트풍 의사 시그니처(개념 수준). **상세 비즈니스 규칙·에러 분기는 CONSTRUCTION의 Functional Design에서 확정.**
참조: `components.md`

> 반환 타입 `Result<T>`는 페일세이프 오류 처리(SECURITY-15)를 전제. 포트 메서드는 어댑터가 구현하며 테스트 시 모의로 대체.

---

## Domain Core

### C1. DomainModel (타입, 메서드는 최소)
```rust
enum ResourceKind { WindowRef, BrowserTab, Folder, AppLaunch, Url, CodingSession }
enum ResourceStatus { Active, Inactive, PermissionRequired, Unknown }

struct WorkBundle { id: BundleId, name: String, resources: Vec<Resource> } // resources는 정렬 순서 보존
struct Resource {
    id: ResourceId,
    display_name: String,      // FR-6.1 편집 가능(식별정보 불변)
    kind: ResourceKind,
    identity: ResourceIdentity,// 안정 식별 + 재실행 정보
    status: ResourceStatus,
    order: u32,
}
struct AppSettings { panel_width: f32, card_height: f32, window_rect: Option<Rect>, /* ... */ }

impl WorkBundle {
    fn add_resource(&mut self, r: Resource) -> Result<()>; // 중복 식별 거부(AC-3)
    fn remove_resource(&mut self, id: ResourceId) -> Result<()>;
    fn reorder(&mut self, order: Vec<ResourceId>) -> Result<()>;
}
```

### C2. IdentityMatcher
```rust
fn matches(saved: &ResourceIdentity, running: &RunningItem) -> bool;      // 동일 창/탭 판정 (AC-2)
fn is_duplicate(bundle: &WorkBundle, candidate: &ResourceIdentity) -> bool;// 등록 중복 (AC-3)
fn distinct_key(id: &ResourceIdentity) -> DistinctKey;                     // 유사 항목 구분 (AC-4/5/8)
```

### C3. RestorePlanner
```rust
fn plan_reopen(res: &Resource) -> ReopenAction;          // 폴더/문서/URL/앱/세션별 재실행 방법 (AC-9)
fn plan_bundle_activation(b: &WorkBundle) -> Vec<ActivationStep>; // 정렬 순서 계획 (AC-11)
```

### C4. StatusEvaluator
```rust
fn evaluate(
    saved: &[Resource],
    running: &[RunningItem],
    perm: PermissionState,
    sessions: &[SessionSnapshot],
) -> Vec<(ResourceId, ResourceStatus)>;                   // FR-7.1~7.3, FR-12.7
fn is_noise(item: &RunningItem) -> bool;                  // 보조/시스템 창 제외 (AC-7)
```

### C5. StoreMigration
```rust
fn current_version() -> u32;
fn load_and_migrate(raw: &[u8]) -> Result<StoreState>;    // 누락 필드 허용, 버전 변환 (FR-11.2~11.4)
fn serialize(state: &StoreState) -> Result<Vec<u8>>;      // 라운드트립 보장 (PBT-02)
```

---

## Ports (트레이트)

### P1. WindowEnumerator
```rust
trait WindowEnumerator {
    fn list_running(&self) -> Result<Vec<RunningItem>>;   // 앱/창 제목/아이콘핸들/선택여부/그룹키
}
```

### P2. WindowActivator
```rust
trait WindowActivator {
    fn focus(&self, target: &WindowTarget) -> Result<ActivationOutcome>; // 최전면 (AC-2)
    fn reopen(&self, action: &ReopenAction) -> Result<WindowTarget>;     // 재실행 후 새 창 참조 (AC-9)
}
```

### P3. BrowserTabReader
```rust
trait BrowserTabReader {
    fn read_tabs(&self) -> Result<Vec<TabItem>>;          // 제목+전체주소, 개별식별 (AC-7/8)
    fn open_url(&self, url: &Url) -> Result<()>;          // 전체 주소 재실행 (AC-9)
}
```

### P4. CodingSessionProvider / Registry
```rust
trait CodingSessionProvider {
    fn tool_id(&self) -> ToolId;                          // "claude-code" 등
    fn discover(&self) -> Result<Vec<SessionRef>>;        // 로컬 세션 발견
    fn read_snapshot(&self, s: &SessionRef) -> Result<SessionSnapshot>;
    //  SessionSnapshot { conversation, last_question, completion: Waiting|NotWaiting|Unknown, status }
    //  손상/부분 파일 허용(패닉 금지, 추측 금지) — AC-18, PBT-03
}
struct SessionProviderRegistry { /* register(provider); providers(); */ }
```

### P5. BundleStore
```rust
trait BundleStore {
    fn load(&self) -> Result<StoreState>;                 // 없으면 기본 상태
    fn save(&self, state: &StoreState) -> Result<()>;     // 원자적 temp→swap (SECURITY-15)
}
```

### P6. PermissionChecker
```rust
trait PermissionChecker {
    fn state(&self) -> PermissionState;                   // Granted|Denied|NotApplicable
    fn guidance(&self) -> PermissionGuidance;             // 안내 텍스트/설정 경로 (AC-13)
}
```

### P7. IconProvider
```rust
trait IconProvider { fn icon_for(&self, app: &AppId) -> Result<IconHandle>; } // 고해상도 (AC-15)
```

### P8. RefreshScheduler
```rust
trait RefreshScheduler {
    fn on_tick(&self, cb: TickCallback);                  // 주기 트리거
    fn set_active(&self, visible: bool, resizing: bool);  // 절약 (NFR-Pf2)
    // 진행 중이면 다음 tick 무시 → 중복 방지 (NFR-Pf3)
}
```

---

## Application Services (요지 — `services.md` 상세)

```rust
// RunningInventoryService
fn running(&self, filter: Option<Query>) -> Result<GroupedInventory>;   // 그룹핑+검색 (FR-2)

// BundleService
fn create(&self, name: String) -> Result<BundleId>;
fn rename(&self, id: BundleId, name: String) -> Result<()>;
fn delete(&self, id: BundleId) -> Result<()>;                            // 확인은 UI (FR-1.3)
fn edit_resource(&self, ...) -> Result<()>;                              // 표시명/재실행주소 (FR-6)

// RegistrationService
fn register_running(&self, bundle: BundleId, item: RunningRef) -> Result<()>; // dedup (AC-3)
fn register_dropped(&self, bundle: BundleId, payload: DropPayload) -> Result<()>; // 파일/폴더/앱/URL, 검증(SECURITY-05)

// ActivationService
fn activate_item(&self, res: ResourceId) -> Result<ActivationOutcome>;
fn activate_bundle(&self, id: BundleId) -> Result<ActivationReport>;     // 순서·부분실패·요약 (AC-11)

// StatusService
fn refresh(&self) -> Result<()>;                                        // 평가 후 이벤트 방출 (FR-7)

// CodingSessionService
fn snapshot(&self, res: ResourceId) -> Result<SessionSnapshot>;         // 대화/질문/완료여부 (AC-16/17)

// SettingsService
fn get(&self) -> AppSettings; fn update(&self, s: AppSettings) -> Result<()>; // 유지 (AC-14)
```

## Bridge
```rust
// TauriCommandBridge: 커맨드 → 서비스, 변경 → emit_event(...)
// 입력 경계 검증(SECURITY-05): 경로/URL은 문자열 명령 조합 없이 인자로 전달
```

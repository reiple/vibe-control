# 작업 단위 (Unit of Work)

단계: INCEPTION — Units Generation, Part 2
참조: `application-design.md`, `unit-of-work-plan.md`
배포 모델: 단일 Tauri 데스크톱 앱(플랫폼별 빌드). 단위 = Cargo 워크스페이스의 크레이트/모듈. 독립 배포 서비스가 아니라 개발·설계·테스트 경계.

> ⚠ **구현 현황(2026-09-08 정합화)** — 크레이트 경계(U1–U7)는 코드와 대체로 일치하나, 각 단위의 **내부 책임 기술은 원 설계 의도**다. 실제로는 포트 트레이트 P1–P8이 vc-core에 정의되지 않았고(U1), U6의 서비스 S1–S7·브리지·스케줄러는 struct 없이 `AppState`+Tauri 커맨드로 평면화됐으며, U4 Windows 열거는 UI Automation이 아니라 PowerShell `Get-Process` 기반이다. 확인된 차이 전량: **[`known-deviations.md`](../../known-deviations.md)**.

---

## 코드 조직 전략 (Greenfield)

```
vibe-control/                     # 워크스페이스 루트
├── Cargo.toml                    # [workspace] members
├── crates/
│   ├── vc-core/                  # U1 Domain Core (순수, no I/O)
│   ├── vc-store/                 # U2 Persistence (BundleStore/JsonBundleStore, migration)
│   ├── vc-os-macos/              # U3 macOS 어댑터 (cfg target_os="macos")
│   ├── vc-os-windows/            # U4 Windows 어댑터 (cfg target_os="windows")
│   ├── vc-sessions/              # U5 Coding Session (Provider+Registry, Claude Code)
│   └── vc-app/                   # U6 Services + Tauri Bridge (바이너리 + 서비스)
├── frontend/                     # U7 Frontend (웹 대시보드; Tauri webview)
└── src-tauri/ (또는 vc-app 내)   # Tauri 설정/엔트리 (U6에 귀속)
```

- **포트 트레이트 위치**: `vc-core`가 포트 트레이트를 정의(도메인이 요구하는 능력). 어댑터 크레이트가 이를 구현 → 의존 방향은 어댑터 → core.
- **플랫폼 게이트**: `vc-os-macos`/`vc-os-windows`는 각 타깃에서만 컴파일. `vc-app`이 cfg로 해당 어댑터를 선택 조립.
- **프론트↔코어**: `vc-app`의 Tauri 커맨드/이벤트 경계로만 통신(상태 단일 소스=코어).

---

## 단위 정의

### U1. vc-core (도메인 코어)
- **책임**: DomainModel(C1)·IdentityMatcher(C2)·RestorePlanner(C3)·StatusEvaluator(C4)·StoreMigration 규칙(C5) + **포트 트레이트(P1-P8) 정의**.
- **특성**: 순수(no I/O), 단위 테스트·PBT 집중 대상.
- **의존**: 없음.
- **테스트**: 도메인 단위 + PBT-02(라운드트립 규칙)·PBT-03(파싱 계약).

### U2. vc-store (영속성)
- **책임**: `BundleStore` 구현(JsonBundleStore) — 단일 JSON, 원자적 temp→swap, 버전 태깅, `StoreMigration` 실행. 설정(AppSettings) 저장 포함.
- **의존**: vc-core(모델·포트·마이그레이션 규칙).
- **확장/보안**: SECURITY-13(안전 역직렬화)·SECURITY-15(페일세이프)·PBT-02(라운드트립).

### U3. vc-os-macos (macOS 어댑터)
- **책임**: WindowEnumerator/Activator(접근성), BrowserTabReader(Safari/Chrome), PermissionChecker, IconProvider 구현.
- **의존**: vc-core(포트/모델). 빌드 게이트: `target_os = "macos"`.
- **관련**: FR-10.1~10.7 · AC-5/6/10/13/15.

### U4. vc-os-windows (Windows 어댑터)
- **책임(원 설계)**: WindowEnumerator/Activator(UI Automation, 최상위 창), BrowserTabReader(Edge/Chrome), IconProvider, 트레이/전역 단축키 지원 훅. PermissionChecker=NotApplicable.
- **구현 현황(2026-09-08)**: 창 열거는 **UI Automation이 아니라 PowerShell `Get-Process`** — `MainWindowHandle != 0 && MainWindowTitle` 필터로 사용자가 띄운 가시 창 앱만 열거(`known-deviations.md#B1`). 활성화는 `WinLauncher`(실행 중이면 기존 창 포커스, 아니면 `Start-Process`). 아이콘은 `WinIconReader`로 **구현됨**(커밋 `6039456`, `#B4`). **미구현**: 브라우저 탭 읽기(`#B3`, 스텁), 트레이/전역 단축키(`#B5`).
- **의존**: vc-core. 빌드 게이트: `target_os = "windows"`.
- **관련**: FR-10.8~10.13.

### U5. vc-sessions (코딩 세션)
- **책임**: `CodingSessionProvider` 트레이트 구현체 + `SessionProviderRegistry`. ClaudeCodeSessionProvider(로컬 세션 파일 읽기 전용·손상 허용 파싱). 신규 도구는 구현체 추가로 확장.
- **의존**: vc-core(포트/모델).
- **확장/보안**: SECURITY-13·NFR-S1(로컬 전용)·PBT-03(파서 견고성).
- **관련**: FR-12 · AC-16/17/18/19.

### U6. vc-app (서비스 + Tauri 브리지, 바이너리)
- **책임**: Application Services(S1-S7) 오케스트레이션 + TauriCommandBridge(커맨드/이벤트) + cfg 기반 어댑터 조립 + RefreshScheduler. 입력 검증 경계(SECURITY-05).
- **의존**: vc-core, vc-store, vc-sessions, (cfg) vc-os-macos / vc-os-windows.
- **관련**: FR-1~FR-7, FR-11(오케스트레이션), 전 스토리의 유스케이스 조합.

### U7. frontend (다크 대시보드 UI)
- **책임**: 좌 실행 패널/우 카드, 검색, 드래그 등록, 상태 표시, 세션 뷰어, 반응형·조절 가능한 레이아웃. 상태는 코어 이벤트 구독(얇은 뷰).
- **의존**: vc-app(Tauri 커맨드/이벤트 계약)만.
- **관련**: FR-8, EPIC-8 · AC-14/15.

---

## 단위 요약표

| 단위 | 크레이트/디렉터리 | 핵심 컴포넌트 | 빌드 게이트 |
|---|---|---|---|
| U1 도메인 코어 | crates/vc-core | C1-C5, 포트 P1-P8 정의 | 전 플랫폼 |
| U2 영속성 | crates/vc-store | JsonBundleStore, Migration | 전 플랫폼 |
| U3 macOS 어댑터 | crates/vc-os-macos | P1/P2/P3/P6/P7(mac) | macos |
| U4 Windows 어댑터 | crates/vc-os-windows | P1/P2/P3/P7(win) | windows |
| U5 코딩 세션 | crates/vc-sessions | P4 + Registry + ClaudeCode | 전 플랫폼 |
| U6 서비스+브리지 | crates/vc-app | S1-S7, Bridge, Scheduler | 전 플랫폼(cfg 조립) |
| U7 프론트엔드 | frontend/ | DashboardUI | 전 플랫폼 |

---

## 검증
- 모든 컴포넌트(C1-C5, P1-P8, S1-S7, B1, F1)가 정확히 한 단위에 귀속 — 확인.
- 순환 의존 없음(의존은 항상 core 방향; 상세 `unit-of-work-dependency.md`).
- 플랫폼 어댑터 분리로 병렬 개발·플랫폼별 빌드 가능 — 확인.
- 모든 스토리 배정은 `unit-of-work-story-map.md` 참조.

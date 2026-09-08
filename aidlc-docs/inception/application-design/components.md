# 컴포넌트 정의 (Components)

단계: INCEPTION — Application Design
아키텍처: **헥사고날(포트&어댑터)** · 상태 단일 소스 = **러스트 코어** · OS 기능 = **기능별 분리 트레이트** · 코딩 세션 = **도구별 어댑터 + 레지스트리**
참조: `requirements.md`(FR-1..FR-12), `stories.md`(EPIC-1..EPIC-12)

> 본 문서는 컴포넌트의 **책임과 인터페이스(포트)** 를 정의합니다. 메서드 시그니처는 `component-methods.md`, 오케스트레이션은 `services.md`, 의존 관계는 `component-dependency.md` 참조. 상세 비즈니스 규칙은 CONSTRUCTION의 Functional Design에서 정의합니다.

> ⚠ **구현 현황(2026-09-08 정합화)** — 이 문서는 **원 설계 의도**다. 실제 코드는 여러 지점에서 다르다: 포트 P1–P8은 vc-core에 트레이트로 정의되지 않았고(P4는 vc-sessions, P5는 vc-store에만; OS 포트는 트레이트 없이 구체 struct), 서비스 S1–S7·`TauriCommandBridge`·`RefreshScheduler`는 별도 struct가 아니라 `AppState`+Tauri 커맨드로 평면화되어 있으며, 어댑터 이름(`*Launcher`/`*IconReader`)·플랫폼 열거 방식도 다르다. 확인된 차이 전량과 백로그는 **[`known-deviations.md`](../../known-deviations.md)** 참조.

---

## 계층 개요

```
[ Frontend (Web, 얇은 뷰) ]
        |  Tauri commands / events
[ Command/Event Bridge ]
        |
[ Application Services (오케스트레이션) ]
        |  ports (traits)
[ Domain Core (순수, I/O 없음) ]
        ^
        |  ports 구현
[ Adapters (플랫폼/인프라) ] -- macOS / Windows / JSON store / 코딩 세션 provider
```

- **Domain Core**: 순수 로직(모델·식별·복원 계획·상태 판정). 외부 I/O 없음 → 단위 테스트·PBT 대상.
- **Ports**: 도메인이 요구하는 능력을 트레이트로 선언(열거/활성화/탭/세션/저장).
- **Adapters**: 포트를 플랫폼별로 구현(테스트 시 모의로 대체).
- **Application Services**: 유스케이스 오케스트레이션(포트 조합, 부분 실패 처리, 이벤트 방출).
- **Bridge/Frontend**: Tauri 커맨드/이벤트 경계와 다크 대시보드 뷰.

---

## 1. Domain Core 컴포넌트

### C1. DomainModel (모델)
- **책임**: 핵심 엔티티/값 타입 정의. 순수 데이터 + 불변식.
  - `WorkBundle`(묶음: id, 이름, 정렬된 리소스 목록)
  - `Resource`(리소스 공통: id, 표시 이름, 종류, 상태, 정렬 순서)
  - `ResourceKind`(열거): `WindowRef` · `BrowserTab` · `Folder` · `AppLaunch` · `Url` · `CodingSession`
  - `ResourceIdentity`(각 종류별 안정 식별 정보 + 재실행 정보; PID/핸들은 **보조 식별자**로만)
  - `ResourceStatus`(열거): `Active` · `Inactive` · `PermissionRequired` · `Unknown`
  - `AppSettings`(패널 너비, 카드 높이, 창 위치/크기 등)
- **관련**: FR-1, FR-3, FR-6, FR-11 · **불변식**: 묶음 내 리소스 식별 중복 불가(FR-3.4/AC-3).

### C2. IdentityMatcher (식별·중복판정)
- **책임**: 실행 중 창/탭과 저장 리소스의 동일성 판정, 등록 시 중복 여부 판정, 유사 항목(같은 앱 다른 창/같은 URL 다른 탭/다른 경로 폴더) 구분.
- **관련**: FR-2.4, FR-3.4, FR-3.5, FR-4.1, FR-9.2 · AC-2/3/4/5/8.

### C3. RestorePlanner (복원 계획)
- **책임**: 닫힌 항목의 재실행 방법 결정(문서/폴더/URL/앱/세션), 전체 활성화 시 **정렬 순서** 계획 수립. 실제 실행은 서비스가 어댑터로 수행.
- **관련**: FR-4.3/4.4, FR-5.1/5.2, FR-9.3 · AC-9/11.

### C4. StatusEvaluator (상태 판정)
- **책임**: 실행 중 목록 + 권한 상태 + 세션 정보를 입력받아 각 저장 리소스의 `ResourceStatus` 도출. 노이즈(보조/시스템 창) 판정 규칙 적용.
- **관련**: FR-2.7, FR-7.1/7.2/7.3, FR-9.6, FR-12.7 · AC-7.

### C5. StoreMigration (버전 마이그레이션)
- **책임**: 저장 스키마 버전 태깅, 하위호환 로딩(누락 필드 허용), 버전 간 변환. 라운드트립 불변식 보장.
- **관련**: FR-11.2/11.3/11.4/11.7 · **PBT-02(라운드트립)**, **SECURITY-13(안전 역직렬화)**.

---

## 2. Ports (트레이트 — 기능별 분리)

### P1. WindowEnumerator
- **책임**: 현재 실행 중이며 사용자에게 보이는 창/앱을 열거(앱·창 제목·아이콘 핸들·선택 여부). 보조/시스템 창 제외 힌트 제공.
- **관련**: FR-2.1~2.4, FR-2.7, FR-10.8/10.9.

### P2. WindowActivator
- **책임**: 특정 창을 최전면으로, 닫힌 항목을 재실행(폴더/문서/앱/URL), 창 상태(최소/최대) 가능한 보존.
- **관련**: FR-2.6, FR-4.1~4.4, FR-10.5/10.6/10.11/10.12 · AC-2/6/9.

### P3. BrowserTabReader
- **책임**: 지원 브라우저의 탭(제목·전체 주소) 수집, 탭 개별 식별, 백그라운드 주소 미제공 시 추측 금지.
- **관련**: FR-9.1~9.7, FR-10.7 · AC-7/8.

### P4. CodingSessionProvider (+ SessionProviderRegistry)
- **책임**: 도구별(우선 Claude Code) 로컬 세션 파일을 **읽기 전용**으로 파싱 → 전체 대화·마지막 질문·답변 완료여부·세션 상태. 레지스트리로 도구 어댑터 등록/확장.
- **관련**: FR-12.1~12.9 · AC-16/17/18/19 · **PBT-03(파서 견고성)**, **SECURITY-13**, **NFR-S1(로컬 전용)**.

### P5. BundleStore
- **책임**: 작업 묶음·리소스·설정을 **원자적**으로 저장/로드(임시 파일 → 교체), 버전 정보 포함.
- **관련**: FR-11.1/11.5/11.6/11.8 · **SECURITY-15(페일세이프)**.

### P6. PermissionChecker
- **책임**: 플랫폼 권한 상태 조회(macOS 접근성) 및 실제 상태 재확인.
- **관련**: FR-10.1/10.2/10.3 · AC-13.

### P7. IconProvider
- **책임**: 앱/브라우저의 실제 아이콘을 고해상도로 제공.
- **관련**: FR-2.3, FR-8.7 · AC-15.

### P8. RefreshScheduler
- **책임**: 주기적 갱신 트리거, 가시성/리사이즈에 따른 절약, 진행 중 갱신 중복 방지.
- **관련**: FR-7.4/7.5/7.6 · NFR-Pf1/Pf2/Pf3.

---

## 3. Adapters (포트 구현)

### A1. macOS 어댑터군
- `MacWindowEnumerator`(접근성 API), `MacWindowActivator`, `MacBrowserTabReader`(Safari/Chrome), `MacPermissionChecker`, `MacIconProvider`.
- **관련**: FR-10.1~10.7 · AC-5/6/10/13/15.

### A2. Windows 어댑터군
- `WinWindowEnumerator`(UI Automation, 최상위 창), `WinWindowActivator`, `WinBrowserTabReader`(Edge/Chrome), `WinIconProvider`. 권한 요청 불필요.
- **관련**: FR-10.8~10.13.

### A3. ClaudeCodeSessionProvider
- **책임**: Claude Code 로컬 세션 파일을 읽어 대화/마지막 질문/완료여부 추출(읽기 전용, 손상 허용 파싱). 레지스트리에 등록.
- **관련**: FR-12.1~12.9 · AC-16/17/18/19.

### A4. JsonBundleStore
- **책임**: 단일 JSON 파일 저장소. 임시 파일 기록 후 원자적 교체, 버전 태깅, `StoreMigration` 연동.
- **관련**: FR-11.1~11.7 · SECURITY-15, PBT-02.

---

## 4. Application Services (오케스트레이션) — `services.md` 상세
- `RunningInventoryService`, `BundleService`, `RegistrationService`, `ActivationService`, `StatusService`, `CodingSessionService`, `SettingsService`.

## 5. Bridge / Frontend
### B1. TauriCommandBridge
- **책임**: 프론트엔드 요청을 서비스 유스케이스에 매핑, 상태 변경을 이벤트로 방출. 입력 검증 지점(SECURITY-05).
### F1. DashboardUI (얇은 뷰)
- **책임**: 다크 대시보드(좌 실행 패널 / 우 카드), 검색, 드래그 등록, 상태 표시, 세션 뷰어. 상태는 코어 구독.
- **관련**: FR-8.*, EPIC-8 · AC-14/15.

---

## 커버리지 (FR → 컴포넌트)
| FR 그룹 | 주요 컴포넌트 |
|---|---|
| FR-1 묶음 관리 | C1, BundleService, P5/A4 |
| FR-2 실행 중 표시 | P1/A1·A2, RunningInventoryService, C4 |
| FR-3 리소스 등록 | C2, RegistrationService, B1(검증) |
| FR-4 개별 활성화 | C2/C3, P2/A1·A2, ActivationService |
| FR-5 전체 활성화 | C3, ActivationService(부분 실패) |
| FR-6 항목 편집 | C1, BundleService |
| FR-7 상태 표시/갱신 | C4, StatusService, P8 |
| FR-8 화면/UI | F1, SettingsService, P7 |
| FR-9 브라우저 | P3/A1·A2, C2 |
| FR-10 OS별 | A1(macOS)/A2(Windows), P6 |
| FR-11 데이터 | C5, P5/A4 |
| FR-12 코딩 세션 | P4/A3, CodingSessionService |

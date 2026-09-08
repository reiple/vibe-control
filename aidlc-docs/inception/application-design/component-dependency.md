# 컴포넌트 의존 관계 (Component Dependency)

단계: INCEPTION — Application Design
참조: `components.md`, `services.md`
원칙: 의존 방향은 **바깥 → 안쪽**(Frontend → Bridge → Services → Ports ← Adapters). 도메인 코어는 무엇에도 의존하지 않음. 서비스는 어댑터가 아닌 **포트 트레이트**에 의존(의존성 역전).

---

## 의존 매트릭스 (행이 열에 의존)

| 소비자 \ 제공자 | DomainCore | Ports(P1-P8) | Adapters | Services | Bridge |
|---|:---:|:---:|:---:|:---:|:---:|
| **Frontend (F1)** | · | · | · | · | ● |
| **Bridge (B1)** | · | · | · | ● | — |
| **Services (S1-S7)** | ● | ● | · | · | · |
| **Adapters (A1-A4)** | ○ | ● (구현) | — | · | · |
| **Ports (P1-P8)** | ○ | — | · | · | · |
| **DomainCore (C1-C5)** | — | · | · | · | · |

● 직접 의존 · ○ 타입(모델) 참조만 · (·) 없음 · (—) 자기/역방향 아님
- Adapters는 Ports 트레이트를 **구현**하고 DomainCore의 값 타입을 참조.
- Services는 Ports를 **주입**받아 사용(구체 Adapter를 모름) → 테스트 시 모의 주입.

---

## 서비스 → 포트/도메인 의존

| 서비스 | 사용 포트 | 사용 도메인 |
|---|---|---|
| S1 RunningInventory | P1, P3, P7 | C2, C4(is_noise) |
| S2 BundleService | P5 | C1, C5 |
| S3 RegistrationService | P1, P3, P5 | C2 |
| S4 ActivationService | P2, P3, P4 | C2, C3 |
| S5 StatusService | P8, P1, P3, P6, P4 | C4 |
| S6 CodingSessionService | P4 | — |
| S7 SettingsService | P5 | C1 |

---

## 어댑터 → 포트 구현

| 포트 | macOS | Windows | 공통/기타 |
|---|---|---|---|
| P1 WindowEnumerator | MacWindowEnumerator(접근성) | WinWindowEnumerator(UI Automation) | — |
| P2 WindowActivator | MacWindowActivator | WinWindowActivator | — |
| P3 BrowserTabReader | MacBrowserTabReader(Safari/Chrome) | WinBrowserTabReader(Edge/Chrome) | — |
| P4 CodingSessionProvider | — | — | ClaudeCodeSessionProvider(+Registry) |
| P5 BundleStore | — | — | JsonBundleStore |
| P6 PermissionChecker | MacPermissionChecker | Win(항상 Granted/NotApplicable) | — |
| P7 IconProvider | MacIconProvider | WinIconProvider | — |
| P8 RefreshScheduler | — | — | TimerScheduler(공통) |

---

## 통신 패턴
- **Frontend ↔ Core**: Tauri **command**(요청/응답) + **event**(상태 푸시). 상태 단일 소스는 코어(질문 답변 확정).
- **Service ↔ Port**: 동기/비동기 트레이트 호출. OS 자동화는 블로킹 가능 → StatusService는 스케줄러로 오프로드, 앱 창 활성화 시 불필요한 탭 탐색 회피(AC-10).
- **Adapter ↔ OS**: macOS 접근성 API / Windows UI Automation / 브라우저 자동화 식별자 / 로컬 파일 읽기(세션).

---

## 데이터 흐름 (텍스트)

```
[등록] Running/Drop -> RegistrationService -> IdentityMatcher(dedup) -> BundleStore.save -> event
[갱신] Scheduler -> StatusService -> (Enumerator/TabReader/Perm/Session) -> StatusEvaluator -> event
[활성화] UI -> ActivationService -> RestorePlanner -> (Activator/open_url/session) -> ActivationReport -> event
[세션] UI -> CodingSessionService -> Registry.provider -> read_snapshot(읽기전용) -> SessionSnapshot
[저장] any change -> DomainModel 검증 -> StoreMigration.serialize -> JsonBundleStore(temp->swap)
```

---

## 경계별 확장/보안 지점
- **B1 Bridge**: 입력 검증 경계 — 경로/URL은 인자 전달, 문자열 명령 조합 금지(SECURITY-05).
- **A4 JsonBundleStore + C5 StoreMigration**: 안전 역직렬화·원자적 저장(SECURITY-13/15), 라운드트립(PBT-02).
- **A3 ClaudeCodeSessionProvider**: 읽기 전용·손상 허용 파싱·로컬 전용(SECURITY-13, NFR-S1), 파서 견고성(PBT-03).
- **P4 Registry**: 신규 도구 어댑터 추가만으로 확장(FR-12.1, 기존 코드 불변).

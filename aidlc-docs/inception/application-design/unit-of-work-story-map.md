# 스토리 → 단위 매핑 (Unit of Work Story Map)

단계: INCEPTION — Units Generation, Part 2
참조: `stories.md`(EPIC-1..12, US-*, JS-1..3), `unit-of-work.md`
표기: **Primary**=해당 단위가 스토리의 핵심 구현. **Support**=협력 단위.

---

## EPIC → 단위 매핑

| EPIC | Primary 단위 | Support 단위 |
|---|---|---|
| EPIC-1 작업 묶음 관리 | U6(BundleService) | U1, U2, U7 |
| EPIC-2 실행 중 표시 | U6(RunningInventory) | U1, U3/U4, U7 |
| EPIC-3 리소스 등록 | U6(Registration) | U1, U3/U4, U2, U7 |
| EPIC-4 개별 활성화 | U6(Activation) | U1, U3/U4, U5, U7 |
| EPIC-5 전체 활성화 | U6(Activation) | U1, U3/U4, U5, U7 |
| EPIC-6 항목 편집 | U6(BundleService) | U1, U2, U7 |
| EPIC-7 상태 표시/갱신 | U6(Status) | U1, U3/U4, U5, U7 |
| EPIC-8 화면/UI | U7(Dashboard) | U6(Settings), U1 |
| EPIC-9 브라우저 처리 | U3/U4(BrowserTabReader) | U1, U6 |
| EPIC-10 OS별 동작 | U3(macOS)/U4(Windows) | U1, U6 |
| EPIC-11 데이터/영속성 | U2(store) | U1 |
| EPIC-12 코딩 세션 | U5(sessions) | U1, U6, U7 |

---

## US → 단위 (상세)

| 스토리 | Primary | Support |
|---|---|---|
| US-1.1 / US-1.2 / US-1.3 | U6 | U1, U2, U7 |
| US-2.1 / US-2.2 / US-2.3 / US-2.4 / US-2.5 | U6 | U1, U3/U4, U7 |
| US-3.1 / US-3.2 / US-3.3 / US-3.4 | U6 | U1, U3/U4, U2, U7 |
| US-4.1 / US-4.2 | U6 | U1, U3/U4, U5 |
| US-5.1 / US-5.2 | U6 | U1, U3/U4 |
| US-6.1 / US-6.2 / US-6.3 | U6 | U1, U2 |
| US-7.1 / US-7.2 | U6 | U1, U3/U4, U5, U7 |
| US-8.1 / US-8.2 / US-8.3 | U7 | U6, U1 |
| US-9.1 / US-9.2 / US-9.3 | U3/U4 | U1, U6 |
| US-10.1 / US-10.2 | U3(macOS) | U1, U6 |
| US-10.3 / US-10.4 | U4(Windows) | U1, U6 |
| US-11.1 / US-11.2 | U2 | U1 |
| US-12.1 / US-12.2 / US-12.3 / US-12.4 | U5 | U1, U6, U7 |

---

## 관통 여정(JS) → 단위

| 여정 | 관여 단위 |
|---|---|
| JS-1 첫 실행 온보딩 | U3(macOS 권한), U6, U7, U1 |
| JS-2 아침 컨텍스트 복원 | U6, U3/U4, U5, U2, U1, U7 |
| JS-3 대기 세션 확인 후 전환 | U5, U6, U3/U4, U7, U1 |

---

## 배정 완전성 검증
- **모든 EPIC(1-12)** 및 **모든 US(US-1.1 ~ US-12.4)** 가 하나 이상의 단위에 Primary 배정됨 — 확인.
- **모든 JS(1-3)** 가 단위 조합에 매핑됨 — 확인.
- 미배정 스토리 없음 — 확인.
- 각 단위가 최소 1개 스토리를 담당(U1은 전 스토리의 도메인 기반으로 상시 Support) — 확인.

## 단위별 스토리 부하(설계/구현 참고)
| 단위 | Primary 스토리 수(대략) |
|---|---|
| U6 vc-app | 최다 (EPIC-1~7 오케스트레이션) |
| U7 frontend | EPIC-8 + 전 화면 표시 |
| U3/U4 어댑터 | EPIC-9/10 |
| U5 sessions | EPIC-12 |
| U2 store | EPIC-11 |
| U1 core | 전 스토리 도메인 기반(Support 상시) |

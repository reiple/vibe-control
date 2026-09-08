# NFR 설계 패턴 — U6 vc-app (as-built)

단계: CONSTRUCTION — U6, NFR Design · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

| 패턴 | 적용 | 목적 |
|---|---|---|
| **Async-at-the-OS-boundary** | OS를 건드리는 커맨드는 전부 `async` | NFR-Pf1 (UI 프리즈 방지) |
| **Lock-free slow path** | `get_app_icon`: 캐시 확인 → 락 해제 → 추출 → 락 재획득 → 삽입. 중복 추출은 무해(동일 키·동일 값) | NFR-Pf1/Pf4 |
| **Negative caching** | 아이콘 없음(`None`)도 캐시 | NFR-Pf4 |
| **Extract-then-release** | `send_claude_message`가 키/리전/모델만 복사 후 락 해제 | NFR-Pf1 + `Send` 제약 |
| **Fail-open on config, fail-closed on action** | 설정 로드 실패는 기본값으로 진행, 활성화 실패는 오류로 보고 | NFR-R1 / SECURITY-15 |
| **Partial-success report** | `RestoreReport { opened, failed, skipped }` | FR-5.4, AC-11 |
| **Write-only secret** | 키는 저장만 되고 절대 읽혀 나오지 않음 | NFR-S3, SECURITY-12 |
| **Quote-at-boundary** | 셸 인용 헬퍼를 경계에서 적용 | SECURITY-05 |
| **cfg 3-way dispatch** | macos / windows / 그 외(명시적 오류·빈 값) | 크로스 컴파일 안전성 |

**미적용(백로그)**: 이벤트 emit(`status_delta`/`activation_report`), `RefreshScheduler`, 포트 트레이트 DI, 입력 정규화 배선, CSP.

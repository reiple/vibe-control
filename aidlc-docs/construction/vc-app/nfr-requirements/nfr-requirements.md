# NFR 요구사항 — U6 vc-app (as-built)

단계: CONSTRUCTION — U6, NFR Requirements · 소급 생성
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — as-built 기술. 원 설계 의도와의 차이는 `../../../known-deviations.md` 참조.

## 성능/반응성 (이 단위의 핵심 NFR)
- **NFR-Pf1 — 갱신이 입력을 방해하지 않을 것**: OS를 건드리는 커맨드(`list_running_apps`, `activate_*`, `restore_bundle`, 세션 커맨드)는 **모두 `async`**. 동기 커맨드는 Tauri 메인 스레드에서 실행되어 창을 얼린다 — 2026-09-08 Windows 프리즈 사고의 직접 원인이었고, 이 규칙이 그 재발 방지책이다.
- **락 보유 최소화**: `get_app_icon`은 캐시 확인 후 **락을 놓고** 추출한다. `send_claude_message`는 키/리전/모델만 뽑고 락을 해제한 뒤 네트워크 호출(std `MutexGuard`는 `Send`가 아니므로 await를 넘길 수도 없다).
- **NFR-Pf4 아이콘 캐시**: 프로세스 수명 동안 `HashMap`, `None`도 캐시해 실패를 재시도하지 않는다. 재시작 시 재추출되어 앱 업데이트로 바뀐 아이콘을 자가 치유한다.
- ⏸ **NFR-Pf3 폴링 절약 미구현** — 가시성/리사이즈 조건 없음(`#D-37`, FR-7.5). 프론트와 공동 책임.

## 신뢰성
- `AppState::new()`가 손상된 설정에 `unwrap_or_default()` → **앱이 벽돌이 되지 않는다**.
- 락 poisoning을 패닉이 아닌 `CommandError`로 변환.
- `restore_bundle`은 항목 실패를 수집만 하고 계속 진행(`RestoreReport`) — AC-11.
- 모든 커맨드가 `Result<_, CommandError>` — 사용자에게 스택트레이스를 노출하지 않는다(SECURITY-09).

## 보안
| 규칙 | 처리 | 상태 |
|---|---|---|
| **NFR-S3 / SECURITY-12** | Bedrock 토큰: 하드코딩·로그·UI 반환 금지. `ClaudeStatus`는 존재여부·출처·모델·리전만 노출. 키 해석 = 설정 → `AWS_BEARER_TOKEN_BEDROCK` → `ANTHROPIC_API_KEY` | ✅ |
| **SECURITY-01 (transit)** | Bedrock 호출은 HTTPS 전용 | ✅ |
| **SECURITY-05 / NFR-S2** | 셸 명령 인용(`shell_quote`/`ps_single_quote`), 모델 id URL 인코딩 | ⚠ 부분 — vc-core `normalize_*`를 **호출하지 않는다**(`#A4`). 드래그로 들어온 `target`은 정규화 없이 어댑터로 전달됨 |
| **SECURITY-04 (CSP)** | `tauri.conf.json`의 `csp: null` | ⏸ 미충족 `#H5-b` |
| **SECURITY-13** | `deny_unknown_fields`/크기·깊이 상한 | ⏸ 미충족 `#H5-a` |
| SECURITY-15 | 모든 OS 호출·파일 I/O 오류 처리 | ✅ |

## 확장 준수
| 규칙 | 상태 |
|---|---|
| SECURITY-01/03/09/11/12/15 | ✅ |
| SECURITY-04 / SECURITY-13 | ✅ **승인된 면제** (2026-09-08, `#H-5`) — 차단 사유 아님. 재검토 트리거는 waiver 표 참조 |
| SECURITY-05 | ⚠ 부분 (정규화 미배선) |
| 커버리지 | ⏸ **테스트 0개 / 795 LOC** `#H5-f` |
| 통합 테스트(모킹) | ⛔ **구조적 불가** — 어댑터가 트레이트가 아니라 cfg 디스패치(`#A1`, `#D-24`) |

## Capability / 권한 표면
`capabilities/default.json` = `["core:default"]` — 추가 플러그인 권한 없음(최소 권한). 창은 `main` 하나.

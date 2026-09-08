# Build & Test 요약

## 개요
vibe-control는 7개 Cargo 크레이트로 구성된 Tauri 데스크톱 앱입니다. 이 문서는 전체 빌드 및 테스트 전략을 요약합니다.

## 테스트 계층
| 계층 | 도구 | 대상 | 문서 |
|---|---|---|---|
| 빌드 | cargo build | 모든 크레이트 | build-instructions.md |
| 단위 | cargo test --lib | Domain 로직 (vc-core 중심) | unit-test-instructions.md |
| 통합 | cargo test --test | 단위 간 상호작용 | integration-test-instructions.md |
| PBT | proptest | 라운드트립/파서 견고성 | pbt-test-instructions.md |
| E2E | 수동 체크리스트 | 실제 OS 창/브라우저/세션 | (수동, 아래 참조) |

## 빌드 순서 (Foundation-First)
```
vc-core (기반) → vc-store → vc-os-macos ‖ vc-os-windows → vc-sessions → vc-app → frontend
```

## 확장 준수 검증
| 규칙 | 검증 방법 | 상태 |
|---|---|---|
| SECURITY-05 (입력 정규화) | normalize 단위 테스트 + PBT | 필수 |
| SECURITY-13 (안전한 역직렬화) | deny_unknown_fields + 손상 파일 테스트 | 필수 |
| SECURITY-15 (파일 권한) | Store 경로 검증 | 필수 |
| PBT-02 (라운드트립) | proptest 1000회 | 차단 규칙 |
| PBT-03 (파서 견고성) | proptest 손상 입력 | 차단 규칙 |

## E2E 수동 체크리스트 (실제 OS)
- [ ] 앱 실행 → 현재 열린 창/앱 목록 캡처
- [ ] 번들 저장 → 앱 재시작 → 번들 로드 확인
- [ ] 번들 복원 → 앱/창/폴더 재오픈 확인
- [ ] 브라우저 탭 캡처 (Safari/Chrome/Edge)
- [ ] Claude Code 세션 대화 표시 확인
- [ ] 권한 요청 흐름 (Accessibility/Automation) 확인

## 성능 목표
| 항목 | 목표 |
|---|---|
| 창 열거 | < 500ms |
| 번들 저장 | < 100ms |
| 번들 로드 | < 100ms |
| 세션 파싱 | < 1s per session |

## 완료 기준
- 전체 빌드 성공 (경고 0)
- 단위 테스트 100% 통과, 커버리지 ≥90%
- PBT-02/03 차단 규칙 통과
- E2E 체크리스트 수동 검증 완료

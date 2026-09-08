# 원자적 저장 알고리즘 — U2 vc-store

---

## Save 알고리즘 (원자성 보장)

```
fn save(bundles: &[WorkBundle]) -> Result<()>:
  1. 저장 경로 결정: OS별 사용자 디렉터리 + "bundles.json"
  2. 임시 경로 계산: "{saved_path}.tmp"
  3. 직렬화: migrate::serialize(bundles) → bytes
  4. 임시 파일 쓰기: fs::write(temp_path, bytes)
     - 실패 → Err, 임시 파일 정리
  5. 원자적 교체: fs::rename(temp_path, saved_path)
     - 이 시점에서만 최종 파일 갱신
     - 실패 → Err, 임시 파일은 이미 쓰여짐(다음 실행 시 정리)
  6. 성공 반환
```

## Load 알고리즘

```
fn load() -> Result<Vec<WorkBundle>>:
  1. 저장 경로 + "bundles.json" 시도 열기
  2. 파일 미존재 → Ok(Vec::new())
  3. 파일 읽기: fs::read(path)
  4. 역직렬화: migrate::load_and_migrate(bytes)
     - 손상 → Err(Corrupt)
     - 미래 버전 → Err(Corrupt, "unsupported version")
  5. Ok(bundles) 반환
```

## 보호 메커니즘

| 시나리오 | 보호 |
|---|---|
| 저장 중 전원 꺼짐 | 임시 파일만 손상. 최종 파일은 이전 버전 유지(rename 전 중단). |
| 저장 실패(디스크 부족) | 임시 파일 정리. 최종 파일 무손상. |
| 메모리 상태 ≠ 파일 | temp→rename이 원자적이므로 부분 상태 불가능. |
| 동시 접근(2 프로세스) | rename(OS 단위 원자)로 only 한 프로세스 쓰기 성공. |

## SECURITY-15 (Failsafe)

- 저장 실패 시: 메모리 상태 = 파일 상태 (오류 전까지 모두 동일)
- 로드 실패 시: 기존 메모리 상태 유지 또는 빈 상태(선택)
- 손상 감지: 버전/스키마 엄격 검증 → 오류 반환, 초기화 아님

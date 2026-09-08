# NFR 요구사항 — U2 vc-store

---

## 신뢰성 (Reliability)
- **원자적 저장**: temp→rename 원칙. 저장 중 실패해도 기존 파일 무손상(SECURITY-15).
- **부분 실패 허용**: 로드 실패 → 빈 묶음 반환 또는 기존 메모리 유지.
- **자동 복구**: 임시 파일 남음 → 다음 실행 시 정리.

## 성능 (Performance)
- **로드 시간**: 수백 개 묶음 로드 <100ms (JSON 파싱).
- **저장 시간**: <200ms (직렬화 + 파일 쓰기).
- **버전 마이그레이션**: <50ms (단계별 변환).

## 보안 (Security) — Full 적용
- **SECURITY-13**: serde `deny_unknown_fields` + 깊이/크기 제한.
- **SECURITY-15**: 원자적 저장으로 부분 상태 불가능.

## 저장소 경로
- **macOS**: `~/.config/vibe-control/` 또는 `~/Library/Application Support/vibe-control/`
- **Windows**: `%APPDATA%/vibe-control/`
- **생성**: 디렉터리 부재 시 자동 생성 (권한 체크).

## PBT (Partial — PBT-02 blocking)
- **라운드트립**: serialize(bundles) → load() = bundles (모든 필드 보존).

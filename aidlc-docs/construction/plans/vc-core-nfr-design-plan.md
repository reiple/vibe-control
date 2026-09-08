# NFR 설계 계획 — U1 vc-core (NFR Design Plan)

단계: CONSTRUCTION — U1 vc-core, NFR Design
참조: nfr-requirements.md(성능/신뢰성/보안/테스트), functional-design(pure 알고리즘)
성격: 순수 코어이므로 **인프라 컴포넌트(큐/캐시/회로차단기) N/A**. 대신 **알고리즘 최적화 패턴·에러 처리 설계·PBT 구조·보안 검증 흐름**에 집중.

이 계획서는 (1) 산출물 체크리스트와 (2) 설계 결정 질문(embedded `[Answer]:`)으로 구성됩니다.

---

## A. NFR 설계 결정 질문 (NFR Design Questions)

### Question 1 — 에러 처리 계층화
에러 타입과 전파 규칙을 어떻게 계층화할까요?

A) **3계층**: 내부 논리 에러(예: Ambiguous 매칭) → 공개 에러 타입(thiserror enum) → 호출자는 Result로 처리. 로그는 구조화(slog/tracing 선택사항). **(권장)**

B) **2계층**: 단순 enum 에러만

[Answer]: A

### Question 2 — 캐싱 전략
MatchSignature 캐싱은 필요할까요, 아니면 매번 계산할까요?

A) **캐싱 없음(현재)**: 연산 저렴(정규화만), 측정 후 필요 시 도입. PBT 테스트 단순성 유지. **(권장)**

B) **선구축 HashMap 캐시**: MatchSignature→ResourceId 맵, 영속 업데이트(복잡도 ↑)

[Answer]: A

### Question 3 — 보안 검증 조합
입력 검증 방식은?

A) **함수별 책임**: MatchSignature는 정규화 전담, matches는 매칭만, evaluate는 상태만. 각 경계마다 검증. **(권장)**

B) **단일 검증기**: 모든 입력을 사전 검증 함수로 통과

[Answer]: A

### Question 4 — 테스트 구조/모듈화
단위 테스트·PBT·벤치마크를 어떻게 구성할까요?

A) **모듈별**: `src/match_sig.rs#[cfg(test)]`, `src/matching.rs#[cfg(test)]`, ... + `tests/pbt_roundtrip.rs`, `benches/perf_matching.rs`. **(권장)**

B) **중앙 테스트 디렉터리**: `tests/` 안에 모두

[Answer]: A

---

## B. NFR 설계 산출물 체크리스트 (생성 시 수행)

- [x] `nfr-design-patterns.md` — 성능/신뢰성/보안 패턴(정규화·에러·PBT 검증·캐싱 전략)
- [x] `logical-components.md` — 모듈 구조·계층화·인터페이스 경계
- [x] 패턴 vs FR/AC/Security/PBT 매핑 확인

---

## C. 참고 (로컬 라이브러리 특성)
- 인프라 패턴(큐/캐시/회로차단기/부하 분산): N/A.
- 확장성/가용성 패턴: N/A.
- 핵심: **신뢰성(패닉 금지, 부분 실패 지속)·보안(입력 검증, 안전 역직렬화)·테스트(PBT, 벤치마크)**.

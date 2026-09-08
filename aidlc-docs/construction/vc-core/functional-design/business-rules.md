# 비즈니스 규칙 — U1 vc-core

단계: CONSTRUCTION — U1 vc-core, Functional Design

---

## BR. 결정/검증 규칙

| ID | 규칙 | 근거 |
|---|---|---|
| BR-1 | 묶음 이름은 트리밍 후 비어있지 않아야 함 | FR-1.1 |
| BR-2 | 묶음 내 리소스는 MatchSignature 중복 불가(등록 시 거부) | FR-3.4 / AC-3 |
| BR-3 | 표시 이름 변경은 identity의 안정 서술자·MatchSignature에 영향 없음 | FR-6.1/6.3 |
| BR-4 | 유사 항목(같은 앱 다른 창 / 같은 URL 다른 탭 / 다른 경로 폴더)은 서로 다른 서술자 → 별도 항목 | FR-3.5 / AC-4/5/8 |
| BR-5 | 창 매칭은 강한 키 필수 일치, 제목 단독 매칭 금지, 모호 시 Ambiguous | FR-2.4/4.1/9.2 / AC-2 |
| BR-6 | 앱만 일치하고 대상 창 불일치 → 매칭 실패(활성화 성공 아님) | FR-4.2 |
| BR-7 | 재실행은 종류별 저장 정보로만(경로/전체URL/앱). 백그라운드 탭 주소 미제공 시 추측 금지 | FR-4.3/9.3/9.7 / AC-9 |
| BR-8 | 전체 활성화 스텝은 order 오름차순 보존 | FR-5.1 / AC-11 |
| BR-9 | 상태 우선순위: PermissionRequired > Active > Inactive > Unknown | FR-7.1/7.2/7.3 |
| BR-10 | 노이즈(시스템/보조 창, 탭 음소거·목록·설정)는 실행 목록·매칭에서 제외 | FR-2.7/9.6 / AC-7 |
| BR-11 | 세션 completion은 provider 제공값 사용, 코어 재분류/추측 금지(3-상태) | FR-12.4/12.5 / AC-16/18 |
| BR-12 | URL 정규화는 스킴(HTTP/HTTPS) 보존, 로컬/사설망은 HTTP 허용 | FR-9.4/9.5 |
| BR-13 | 휘발성 힌트(PID/HWND/window number)는 영구 식별자로 저장/신뢰하지 않음 | FR-11(보조 식별) |

## 에러 분류 (technology-agnostic)
| 분류 | 의미 | 처리 원칙 |
|---|---|---|
| `Validation` | 규칙 위반(빈 이름, 중복 등록) | 거부 + 사유 반환, 상태 불변 |
| `NotFound` | 매칭/재실행 대상 없음 | Inactive/미발견으로 표기, 계속 진행 |
| `Ambiguous` | 매칭 후보 다수 | 자동 승격 안 함, 상위 결정 요청 |
| `Corrupt` | 역직렬화/파싱 손상 | 안전 실패(덮어쓰기 금지), 가능한 범위 보존 |
| `PermissionDenied` | 플랫폼 권한 부족 | PermissionRequired 상태, 안내 유도 |
- 모든 공개 API는 `Result`로 페일세이프(SECURITY-15). 패닉 금지(특히 파싱/역직렬화).

## PBT 계약 (Partial 대상)
| ID | Property | 대상 |
|---|---|---|
| PBT-02 | 임의의 유효 StoreState `s`에 대해 `load(serialize(s)) == s`(라운드트립) | StoreMigration/직렬화 계약 |
| PBT-03 | 임의의 바이트열/부분 손상 입력에 대해 파싱은 패닉하지 않고 `Ok(partial)` 또는 `Err(Corrupt)` 반환(추측된 값 생성 금지) | 세션 스냅샷 파싱 계약, 역직렬화 |
| PBT-02b | 서로 다른 안정 서술자는 서로 다른 MatchSignature(충돌 없음) 생성 | MatchSignature |

> 생성기(PBT-07) 품질: WorkBundle/Resource/URL/경로/세션 스냅샷에 대한 대표·경계 입력 생성. 축소/재현(PBT-08) 지원. 프레임워크(PBT-09)는 U2/U5 NFR 단계에서 확정(러스트 `proptest` 등 후보).

## 보안 규칙 (Security Full — 코어 관련분)
- **SECURITY-05(주입 방지)**: 경로/URL/앱ID는 값으로 취급, 코어는 명령 문자열을 조합하지 않음(실행은 어댑터, 인자 전달).
- **SECURITY-13(안전 역직렬화)**: 엄격 스키마·크기/깊이 제한·미래 버전 안전 실패.
- **SECURITY-15(페일세이프)**: 오류 시 기존 상태 보존, 부분 실패는 상위에서 지속 처리.

## FR/AC 커버리지 확인
- FR-1..FR-12의 순수 규칙 요소가 BR-1..BR-13에 매핑 — 확인.
- AC-2/3/4/5/7/8/9/11/16/18의 판정 근거가 규칙으로 표현 — 확인.
- Frontend 규칙 N/A(U1 순수 코어).

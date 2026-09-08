# 비즈니스 로직 모델 — U1 vc-core

단계: CONSTRUCTION — U1 vc-core, Functional Design
성격: 순수 알고리즘(결정적, I/O 없음). 어댑터가 공급한 스냅샷을 입력으로 사용.

---

## L1. MatchSignature 계산 (IdentityMatcher)
```
fn match_signature(id: &ResourceIdentity) -> MatchSignature
  - kind별 안정 서술자만 사용(표시 이름·힌트 제외)
  - 정규화: 경로는 정규 경로, URL은 스킴/호스트/경로/쿼리/포트 보존 정규화, title_signature는 시그니처에서 제외
  - 결정적 해시/튜플 → 동일 서술자면 항상 동일 값
```
- **용도**: 중복 판정(AC-3), 유사 항목 구분(AC-4/5/8).
- **불변식(PBT-03 인접)**: 같은 서술자 → 같은 시그니처; 다른 서술자 → 다른 시그니처(충돌 회피).

## L2. 창 매칭 (계층 매칭)
```
fn matches(saved: &ResourceIdentity, running: &RunningItem) -> MatchResult
  1) 강한 키 비교: app_id + role/class (+ Folder는 absolute_path, Tab은 full_url+discriminator) 필수 일치
     - 불일치 → NoMatch
  2) 강한 키 일치가 다수면 title_signature로 tie-break (보조)
  3) 여전히 다수면 is_focused/힌트로 tie-break, 그래도 모호하면 Ambiguous(첫 후보 선택 안 함 → 상위에서 결정)
  - 제목만으로는 매칭하지 않음(추측 금지)
```
- **관련**: FR-2.4, FR-4.1, FR-9.2 · AC-2/8. 앱만 일치하고 창 불일치 → NoMatch(FR-4.2).

## L3. 복원 계획 (RestorePlanner)
```
fn plan_reopen(res) -> ReopenAction
  WindowRef  -> 연결 문서/폴더 있으면 OpenPath, 없으면 LaunchApp(app_id)
  BrowserTab -> OpenUrl(full_url)              # 전체 주소, 스킴 보존 (AC-9)
  Folder     -> OpenPath(absolute_path)
  AppLaunch  -> LaunchApp(app_id)
  Url        -> OpenUrl(full_url)
  CodingSession -> FocusLinkedWindow 또는 ShowSessionDetail(대체)  # FR-12.8

fn plan_bundle_activation(bundle) -> Vec<ActivationStep>
  - resources를 order 오름차순 정렬 → 각 리소스에 대해 [매칭 시 Focus, 아니면 plan_reopen] 스텝 생성
  - 순서 보존 (AC-11)
```

## L4. 상태 판정 (StatusEvaluator) — 우선순위 규칙
```
fn evaluate(saved, running[], perm, sessions[]) -> [(ResourceId, ResourceStatus)]
  for res in saved:
    if requires_permission(res.kind, perm):        # macOS 접근성 Denied 등
        -> PermissionRequired
    else if matches_any(res, running) == Match:
        -> Active
    else if session_res && !session.available:
        -> Inactive (또는 PermissionRequired if 접근 불가 사유)
    else if has_reopen_info(res):
        -> Inactive
    else:
        -> Unknown
```
- **우선순위**: PermissionRequired > Active > Inactive > Unknown (확정 결정).
- **노이즈 규칙**: `is_noise(item)` → 보조/시스템 창, 탭 음소거/목록/설정 창 제외(AC-7, FR-2.7/9.6).
- Ambiguous 매칭은 Active로 승격하지 않고 Unknown 유지(추측 금지).

## L5. 세션 판정 (코어는 권위 위임)
```
fn session_status(snapshot) -> (SessionCompletion, ResourceStatus)
  - completion은 provider가 제공한 값 그대로(Waiting/NotWaiting/Unknown), 코어는 재분류/추측 안 함
  - available=false → status Inactive/PermissionRequired
```
- **관련**: FR-12.4/12.5/12.7 · AC-16/18.

## L6. 마이그레이션 (StoreMigration) — 순수 변환 규칙
```
fn load_and_migrate(parsed_versioned) -> StoreState
  - version < current: 단계적 up-migration 함수 체인 적용, 누락 필드는 기본값(하위호환)
  - 알 수 없는 미래 버전: 안전 실패(오류 반환), 손상 데이터로 덮어쓰지 않음

fn serialize(state) -> Bytes    # U2가 파일 I/O 담당; 여기선 형태 변환 계약만
```
- **불변식(PBT-02)**: 임의의 유효 StoreState에 대해 `load(serialize(s)) == s` (라운드트립).
- **SECURITY-13**: 역직렬화는 크기/깊이 제한·엄격 스키마, 실행 가능한 형태 미허용.

---

## 핵심 시나리오 (순수 로직 관점)
- **등록 중복 방지**: `is_duplicate` = bundle 내 어떤 리소스와 MatchSignature 동일? (AC-3)
- **유사 항목 개별화**: 서로 다른 서술자 → 서로 다른 시그니처 → 별도 항목(AC-4/5/8)
- **전체 활성화 부분 실패**: plan_bundle_activation은 순서 스텝만 산출; 실패 지속·요약은 U6 서비스가 수행(코어는 순수)

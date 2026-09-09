# 코드 생성 계획 — U7 frontend (as-built)
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — 이 계획은 사전 계획이 아니라 **as-built 역작성**이다. `[x]`는 현행 코드(커밋 `d1e0f2f`)에서 실제로 확인된 항목, `[ ]`는 계획 대비 **미이행으로 남은 항목**이다.

### Step 1 스캐폴드
- [x] Vite + React 18 + TS, `npm run build` = `tsc && vite build`
- [x] `index.html` — 프리마운트 배경색 인라인 (FR-13.7)
- [ ] ESLint 설정 / 테스트 러너 → ⏸ `#H5-g`

### Step 2 타입·API
- [x] `types.ts` — 백엔드 DTO 미러(`WorkBundle`, `Resource`, `RunningApp`, `RunningWindow`, `SessionSnapshot`, `RestoreReport`, `ChatMsg`, `ClaudeStatus`)
- [x] `api.ts` — invoke 래퍼 17개 + `inTauri()` 가드(브라우저 미리보기에서 빈 값 반환)
- [ ] `Resource.status` 필드 → `#D-30` (FR-7.1~7.3 연기)
- [ ] 타입 자동 생성(현재 수동 미러 → 계약 드리프트 위험)

### Step 3 부팅 (FR-13)
- [x] `Splash` 컴포넌트(EP-133 모티프, `role="progressbar"`/`aria-busy`)
- [x] `Promise.allSettled([getBundles, claudeStatus, refreshRunning])`
- [x] 최소 650ms / 하드캡 8000ms / 페이드 400ms / 멱등 `finish()`
- [ ] 고DPI 초기 프레임 확대 보정 → `#H2`(보류)

### Step 4 실행 앱 패널
- [x] 앱 그룹 렌더 + 아이콘 1회 + 검색 필터
- [x] `expandedApps`(이름 키) 펼침/접기, `▸ n` / `▾ n`
- [x] 클릭 분기: >1 펼침 / ==1 창 활성화 / ==0 앱 활성화
- [x] 창 하위 행 + `is_focused` 녹색 점 (FR-2.4)
- [x] 앱 행만 draggable, 폴링이 드래그에 양보
- [x] **폴링 절약(2026-09-08)** — `document.hidden` 스킵 / `visibilitychange` 즉시 갱신 / resize 400ms 유예 / `pollInFlight` 중첩 방지 (FR-7.5·7.6, NFR-Pf3)

### Step 5 묶음 카드
- [x] 카드 그리드(2열/1열), 드롭 등록, Restore + `RestoreReport` 표시, 삭제 확인 2단계
- [x] `SessionStatus` 인라인(완료 칩 + 마지막 질문)
- [ ] 리소스 제거/편집/이름변경 UI → `#D-33`, `#D-31`, `#D-32`
- [ ] 저장 리소스 상태 점·흐림 → `#D-30`

### Step 6 Claude 콘솔
- [x] 하단 콘솔(트랜스크립트·모델 선택·키 모달·연결 표시), 키 프리필 금지

### Step 7 스타일
- [x] `styles.css` — 다크 대시보드, `resize: horizontal/vertical`, 스플래시, 로컬 폰트 번들
- [ ] 레이아웃 값 영속 → `#D-39`

# 요구사항 확인 질문 (Requirements Verification Questions)

`REQUIREMENTS.ko.md`는 기능/화면/데이터/인수 조건 측면에서 매우 상세합니다. 다만 **구현 방향을 결정하는 기술적 선택**과 **확장(extension) 적용 여부**가 아직 명시되어 있지 않습니다. 아래 질문에 각 `[Answer]:` 태그 뒤에 알파벳(A, B, C ...)으로 답해 주세요. 보기와 맞는 항목이 없으면 마지막 `X) Other`를 선택하고 설명을 적어 주세요. 모두 작성하신 뒤 "완료" 또는 "done"이라고 알려 주시면 다음 단계로 진행하겠습니다.

---

## Question 1
데스크톱 앱을 어떤 기술 스택으로 구현할까요? (요구사항의 macOS 접근성 API, Windows UI Automation, 브라우저 자동화 식별자 접근이 프레임워크 선택에 큰 영향을 줍니다.)

A) Electron (Node.js + 웹 프론트엔드) — 크로스플랫폼, 네이티브 접근은 Node 애드온/자식 프로세스(AppleScript, PowerShell, UIAutomation)로 처리

B) Tauri (Rust 코어 + 웹 프론트엔드) — 경량, 네이티브 접근은 Rust 크레이트로 처리

C) 운영체제별 완전 네이티브 (macOS: Swift/AppKit, Windows: C#/WinUI 또는 C++) — 별도 코드베이스

D) Other (please describe after [Answer]: tag below)

[Answer]: B

## Question 2
이번 개발 반복(iteration)에서 우선 완성할 대상 운영체제는 무엇인가요? (요구사항 5장은 OS별 실제 환경에서 별도 빌드/검증을 요구합니다. 현재 개발 환경은 macOS입니다.)

A) macOS를 먼저 완성하고 Windows는 이후 반복에서 진행

B) Windows를 먼저 완성하고 macOS는 이후 반복에서 진행

C) 처음부터 macOS와 Windows를 동시에 대상으로 개발

D) Other (please describe after [Answer]: tag below)

[Answer]: C

## Question 3
이번 범위에서 지원할 브라우저는 어디까지인가요? (요구사항은 macOS의 Safari/Chrome를 명시하고, Windows는 OS 자동화 식별자 기반 탭 구분을 언급합니다.)

A) macOS: Safari + Chrome / Windows: Edge + Chrome (각 OS 기본·대표 브라우저)

B) 위 항목 + Chromium 계열 전반(Brave, Arc 등) 추가 지원

C) 위 항목 + Firefox 추가 지원

D) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 4
작업 묶음/리소스 데이터의 로컬 저장 방식은 무엇으로 할까요? (요구사항 8장: "임시 파일을 먼저 작성한 뒤 교체", "이전 버전 데이터에 일부 필드가 없어도 읽기 가능"을 요구합니다.)

A) 단일 JSON 파일 (임시 파일 원자적 교체 + 버전 필드로 마이그레이션) — 요구사항의 원자적 저장·부분 필드 허용과 가장 직접적으로 부합

B) SQLite 임베디드 데이터베이스

C) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 5
자동 테스트 범위를 어느 수준으로 잡을까요? (요구사항 12장 완료 기준: "자동 테스트, 정적 검사, 각 OS 실행 확인 통과")

A) 핵심 도메인 로직(작업 묶음/리소스 모델, 저장·마이그레이션, URL·경로 정규화) 단위 테스트 중심 + 정적 검사

B) 위 항목 + OS 어댑터(창 열거·활성화·브라우저 탐색) 통합 테스트를 모킹 기반으로 추가

C) 위 항목 + 실제 OS 환경 대상 E2E/수동 검증 체크리스트까지 포함

D) Other (please describe after [Answer]: tag below)

[Answer]: C

---

# 확장(Extension) 적용 여부

아래는 AI-DLC 확장 규칙의 적용 여부를 결정하는 질문입니다. 이 프로젝트는 **외부 서버 전송이 없는 로컬 전용 데스크톱 앱**(요구사항 148)이라는 점을 참고해 선택해 주세요.

## Question 6: Security Extensions
이 프로젝트에 보안(SECURITY) 확장 규칙을 적용할까요?

A) 예 — 모든 SECURITY 규칙을 차단(blocking) 제약으로 적용 (프로덕션급 애플리케이션 권장)

B) 아니오 — 모든 SECURITY 규칙을 건너뜀 (PoC, 프로토타입, 실험용 프로젝트에 적합)

X) Other (please describe after [Answer]: tag below)

[Answer]: A

## Question 7: Resiliency Extensions
이 프로젝트에 회복탄력성(Resiliency) 베이스라인을 적용할까요?

**이 확장의 성격.** 활성화하면 **AWS Well-Architected Framework(신뢰성 기둥)** 기반의 **설계 단계 모범 사례**(내결함성, 고가용성, 관찰 가능성, 복구성 등 15개 실천 영역)를 요구사항·설계·코드에 반영합니다. 이는 프로덕션 준비 완료나 가용성/RTO/RPO 보장을 의미하지 않으며 좋은 출발점을 제공하는 것입니다. (참고: 이 프로젝트는 클라우드/서버가 없는 로컬 데스크톱 앱이라 상당 부분이 N/A일 수 있습니다.)

A) 예 — 회복탄력성 베이스라인을 설계 단계 지침으로 적용 (비즈니스 크리티컬 워크로드 권장)

B) 아니오 — 회복탄력성 베이스라인을 건너뜀 (PoC, 프로토타입, 빠른 반복이 중요한 실험용 프로젝트에 적합)

X) Other (please describe after [Answer]: tag below)

[Answer]: B

## Question 8: Property-Based Testing Extension
이 프로젝트에 속성 기반 테스트(PBT) 규칙을 적용할까요?

A) 예 — 모든 PBT 규칙을 차단 제약으로 적용 (비즈니스 로직, 데이터 변환, 직렬화, 상태 저장 컴포넌트가 있는 프로젝트 권장 — 예: URL/경로 정규화, 저장 직렬화 왕복)

B) 부분 — 순수 함수와 직렬화 왕복(round-trip)에만 PBT 규칙 적용 (알고리즘 복잡도가 제한적인 프로젝트에 적합)

C) 아니오 — 모든 PBT 규칙을 건너뜀 (단순 CRUD, UI 전용, 얇은 통합 계층에 적합)

X) Other (please describe after [Answer]: tag below)

[Answer]: B

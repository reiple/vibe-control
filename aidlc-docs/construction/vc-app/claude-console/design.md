# 기능 설계 — 앱 내 Claude 프롬프트 콘솔 (AWS Bedrock)

단계: CONSTRUCTION — 사후 문서화(post-hoc) · 유닛: U6 vc-app
상태: **구현 완료** (커밋 `a163d8d` "Add in-app Claude prompt console wired to AWS Bedrock")
정합화: 이 기능은 원 요구사항/설계에 없던 신규 추가분으로, `known-deviations.md#F1`에 이탈로 등록되고 본 문서로 정식 문서화된다.

> **주의 — NFR-S1 관계**: 원 NFR-S1은 "작업 묶음 데이터를 외부 서버로 전송하지 않음"이었다. 본 기능은 사용자가 입력한 프롬프트를 AWS Bedrock으로 **외부 HTTPS 전송**한다. `requirements.md`의 NFR-S1을 "코딩 세션/작업 묶음 **데이터**의 로컬 전용"으로 한정하고, 본 콘솔의 네트워크 송신을 별도 규정(NFR-S3, SECURITY 매핑)으로 분리했다. §5 보안 참조.

---

## 1. 목적 / 범위

- 사용자가 별도 도구 전환 없이 앱 하단 콘솔에서 Claude에게 직접 프롬프트를 보내고 답변을 받는다.
- 모델 도달 경로는 **Amazon Bedrock 런타임**(Claude Code가 `CLAUDE_CODE_USE_BEDROCK` + `AWS_BEARER_TOKEN_BEDROCK`로 도달하는 방식과 동일).
- 이 콘솔은 FR-12(로컬 세션 파일 읽기 전용 열람)와 **별개 기능**이다. FR-12는 여전히 로컬 전용이며 네트워크 호출을 하지 않는다.

## 2. 구성요소

| 위치 | 역할 |
|---|---|
| `crates/vc-app/src/claude.rs` | Bedrock 런타임 클라이언트. `send_message(token, region, model, messages)` 비동기 POST |
| `crates/vc-app/src/lib.rs` | `AppState`의 키/리전/모델 해석·세터, `ClaudeStatus`, 4개 Tauri 커맨드 |
| `crates/vc-core/src/models/settings.rs` | `AppSettings`에 `claude_api_key` / `claude_model` / `claude_region`(모두 `#[serde(default)] Option`) |
| `crates/vc-app/Cargo.toml` | `reqwest` 의존성(HTTPS) |
| `frontend/src/{App.tsx, api.ts, types.ts, styles.css}` | 하단 콘솔 UI, 연결/키 입력 모달, 모델 선택, `ChatMsg`/`ClaudeStatus` 타입 |

## 3. 클라이언트 계약 (claude.rs)

- **엔드포인트**: `POST https://bedrock-runtime.{region}.amazonaws.com/model/{model}/invoke`
  - `model`은 URL 경로 세그먼트로 인코딩(`encode_model`: `:` → `%3A`).
- **인증**: `Authorization: Bearer <token>` (Bedrock API 키 — SigV4 서명 불필요).
- **요청 바디**: `{ anthropic_version: "bedrock-2023-05-31", max_tokens: 4096, messages: [{role, content}...] }`
- **응답**: `content[]` 중 `type == "text"` 블록을 개행으로 결합해 반환.
- **기본값**: 모델 `global.anthropic.claude-opus-4-8`, 리전 `ap-northeast-2`, 타임아웃 120초, max_tokens 4096.
- **오류 처리**: 비성공 상태는 Bedrock의 `error.message`/`message`/`Message`를 추출해 사람이 읽는 문자열로 반환. **토큰은 오류 메시지·로그·UI 어디에도 노출하지 않음**. 빈 응답은 오류로 처리.

## 4. Tauri 커맨드

| 커맨드 | 역할 | 위치 |
|---|---|---|
| `claude_status` | 키 설정 여부·현재 모델·리전 등 상태 반환(키 값 자체는 반환 안 함) | `vc-app/src/lib.rs:468` |
| `set_claude_api_key` | Bedrock bearer 토큰을 로컬 설정에 저장 | `:478` |
| `set_claude_model` | 선호 모델 id 저장 | `:491` |
| `send_claude_message` | 대화 메시지 배열을 Bedrock에 전송, 답변 텍스트 반환 (async) | `:507` |

> 줄 번호는 2026-09-08 git pull 이후 기준. `ClaudeStatus` struct는 `:448`, 상태 산출 `claude_status_of`는 `:457`.

**키 해석 순서**(호출 시점): 로컬 설정(`claude_api_key`) → 환경변수 `AWS_BEARER_TOKEN_BEDROCK`. 리전: 설정(`claude_region`) → `AWS_REGION` → 앱 기본.

## 5. 보안 (Security Baseline 재평가)

| 항목 | 처리 |
|---|---|
| 비밀 취급 | 토큰 하드코딩 금지. 로컬 설정 파일(OS config 디렉터리, 리포 밖)에만 저장. `claude_status`는 키 존재 여부만 노출, **값 미반환**. 오류/로그에 토큰 미포함(SECURITY-03, SECURITY-12) |
| 전송 구간 | HTTPS only(bedrock-runtime.\<region\>.amazonaws.com) (SECURITY-01 transit) |
| 데이터 경계 | 콘솔로 전송되는 것은 **사용자가 명시적으로 입력한 프롬프트**뿐. 작업 묶음/세션 대화/문서 내용은 자동 전송하지 않음(NFR-S1 원칙 유지) |
| 입력 검증 | 모델 id는 URL 경로 인코딩 후 사용. 빈 토큰/빈 메시지 방어(SECURITY-05) |

## 6. 미결/후속 (열린 항목)

- 스트리밍 미지원(단발 invoke). 대화 히스토리 영속화 정책 미정(현재 세션 메모리).
- `Cargo.toml`의 `reqwest` 주석이 "Anthropic Messages API"로 표기되어 있으나 실제 대상은 Bedrock — 주석 정정 후보(코드 백로그).
- 사용자 문서(NFR-U1)에 콘솔 사용법·키 설정·데이터 전송 고지 추가 필요.

## 7. 관련
- 요구사항 개정: `inception/requirements/requirements.md`(NFR-S1/NFR-S3, SECURITY-01/12 매핑)
- 이탈 등록: `known-deviations.md#F1`

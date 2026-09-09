# 🟡 OPERATIONS PHASE — vibe-control

**Status**: Instantiated 2026-09-09. The AI-DLC framework ships OPERATIONS as a *placeholder*
(`operations/operations.md`: "the workflow currently ends after Build and Test in CONSTRUCTION").
This document instantiates it with the content that placeholder says it will *eventually* hold —
**deployment planning + production-readiness** — scoped to what actually applies to this project.

## Why this scope

vibe-control is a **local desktop application** (Tauri v2: Rust core + React/Vite frontend), not a
cloud service. Infrastructure Design was deliberately **SKIPPED** in the execution plan ("local
desktop app — no cloud infra"). Therefore the operational concerns that apply are:

| Generic Operations scope (from `operations.md`) | Applies here? | This project's form |
|---|---|---|
| Deployment planning & execution | ✅ | **Release & packaging** of signed desktop installers (macOS + Windows) — see `release-packaging.md` |
| Production readiness checklist | ✅ | **Go / no-go checklist** before a public build — see `production-readiness-checklist.md` |
| Maintenance & support workflows | ▲ partial | Known-deviations backlog is the maintenance queue (`../known-deviations.md`) |
| Monitoring & observability | ✖ | N/A — no server; the only telemetry surface is the in-app CloudWatch usage meter (client-side) |
| Incident response | ✖ | N/A — no live service to page on |

## Prerequisite state (verified 2026-09-09)

- **CONSTRUCTION complete**: all 7 units built; `cargo build --workspace` + `cargo test --workspace`
  green; clippy 0 warnings; Tauri app launches and is live-verified on real Windows. See
  `../aidlc-state.md` → Build & Test verification.
- **Toolchain (this machine)**: `cargo 1.95.0`, `node v26.8.1`. Tauri CLI is a **frontend
  devDependency** (`npm --prefix frontend exec tauri …`), not a global `cargo install` — see
  `../known-deviations.md#H1`.
- **Bundle config** (`crates/vc-app/tauri.conf.json`): `productName: vibe-control`, `version: 0.1.0`,
  `identifier: com.vibecontrol.desktop`, `bundle.active: true`, `bundle.targets: "all"`,
  `macOS.signingIdentity: "vibe-control-dev"`, `app.security.csp: null`.

## Release blockers surfaced while grounding this plan

1. **⚠️ WAIVED for the hackathon — Signing is dev-only, not distribution-grade.** `signingIdentity:
   "vibe-control-dev"` is a local self-signed identity; the shipped build is **ad-hoc** signed and not
   notarized (`spctl -a` → rejected, confirmed). Per the **hackathon decision (2026-09-09),
   code-signing certificates are out of scope** — this blocker is acknowledged and waived. The
   deliverable is the unsigned universal dmg; judges clear Gatekeeper once (see
   `production-readiness-checklist.md` §8). Only matters again if the app goes public later.
2. **✅ RESOLVED (2026-09-09) — `beforeBuildCommand` was empty.** It is now set to `npm --prefix
   ../frontend run build`, so `tauri build` auto-rebuilds the frontend before bundling and the
   embedded UI can no longer go stale. Verified by a build run. See `release-packaging.md#build`.

## Hackathon build — SHIPPED 2026-09-09

The hackathon/demo release track (GO) was executed on this Mac. Sequence: `npm install` (the
`@tauri-apps/cli` devDependency was declared but not installed) → frontend build → `tauri build
--target universal-apple-darwin` with `APPLE_SIGNING_IDENTITY=-` (ad-hoc; the named `vibe-control-dev`
identity is absent from this machine's keychain).

**Final deliverable — `~/Desktop/vibe-control_0.1.0_universal.dmg`** (18 MB), a **universal binary**
verified via `lipo -archs` → `x86_64 arm64`, so it runs on **both Apple Silicon and Intel** Macs.
Verified: codesign adhoc/universal, `Identifier=com.vibecontrol.desktop`, Info.plist v0.1.0, app
launches without crash, `spctl` rejected as expected (unsigned → judges clear quarantine once, see the
checklist §8). An earlier arm64-only dmg was superseded and removed from the Desktop.

The `beforeBuildCommand` hardening means every `tauri build` now auto-refreshes `frontend/dist` first,
so the embedded UI is guaranteed current. Windows installers must be built on a Windows host (no
cross-compile) and are out of scope this round.

## Deliverables in this phase

- `release-packaging.md` — how to produce distributable macOS + Windows builds, the signing gap, and
  the exact command sequence.
- `production-readiness-checklist.md` — a go / no-go checklist referencing the CONSTRUCTION E2E
  checklists, the security posture (NFR-S1/S3), and the open backlog.

## What this phase does NOT do

No code is changed here, and **nothing is built, signed, or published** by authoring these docs —
those are irreversible/outward-facing actions that require an explicit go decision (a distribution
certificate must be obtained first; see the checklist). This phase produces the *plan and gate*.

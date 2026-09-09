# Production Readiness Checklist — vibe-control

Go / no-go gate before cutting a distributable release. Status as of 2026-09-09. Items reference the
CONSTRUCTION artifacts (`../construction/build-and-test/`), the deviation record
(`../known-deviations.md`), and the release plan (`release-packaging.md`).

Legend: ✅ done · ⚠️ done-with-caveat · ⛔ blocker for public release · ⏳ open / backlog · N/A

## 1. Build & quality gates
- ✅ `cargo build --workspace` clean (release + debug).
- ✅ `cargo test --workspace` green — vc-core 24 · vc-os-windows 5 · vc-app 2 · vc-sessions 8 · vc-store 5.
- ✅ `cargo clippy --workspace --all-targets` — 0 warnings.
- ✅ Frontend `tsc --noEmit` clean · `vite build` succeeds.
- ✅ **Frontend can no longer go stale** — `beforeBuildCommand` set to `npm --prefix ../frontend run
  build` (2026-09-09, verified: `tauri build` auto-runs the frontend build before bundling). Was a ⚠️
  blocker (empty command); now resolved.

## 2. Functional acceptance (E2E)
- ✅ Core flows verified live on real Windows: per-window enumeration (AC-20), window/tab activation,
  browser-tab live view + individual-tab registration, bundle capture/restore, DnD, atomic save.
- ⏳ **Manual E2E checklist not fully run on macOS** — several bolts were verified on Windows only.
  Run `../construction/build-and-test/build-and-test-summary.md` E2E items (AC-20 window expansion,
  AC-21 splash) on **macOS** before claiming cross-platform readiness.
- ⏳ Runtime-verification gaps flagged in state (Edge background/foreground tab reads, tab-reorder
  activation accuracy, KakaoTalk per-window registration) — `aidlc-state.md` Bolts H3/H4 mark these
  `[ ]` pending real-environment confirmation. Close or explicitly waive before release.

## 3. Security & privacy (extension: Security Baseline = Full, 2 waivers)
- ✅ Local-first data (NFR-S1): bundle/session data never leaves the machine; single local JSON,
  atomic write (C1/C2 resolved).
- ⚠️ **Network egress disclosure (NFR-S3)**: the in-app Claude console (→ AWS Bedrock, HTTPS) and the
  CloudWatch usage meter send data off-machine using the user's own AWS credentials. Ship a plain
  disclosure in release notes / first-run text. (`known-deviations.md#F1`.)
- ⚠️ Secrets handling: AWS creds are read from `~/.aws/credentials`; the Claude API key field is
  excluded from the persisted `LayoutSettings` DTO. Confirm no key is written to logs/settings on
  release build.
- ⚠️ **CSP is `null`** (`app.security.csp`) — SECURITY-04 waiver (`known-deviations.md#H-5`). Acceptable
  while all assets are local/embedded; **must set a CSP before loading any remote content**.

## 4. Signing / distribution — ⚠️ WAIVED (hackathon)
> **Decision (2026-09-09):** this is a **hackathon** deliverable — code-signing certificates are
> explicitly **out of scope**. The blockers below are acknowledged and waived; the ad-hoc-signed
> build is the accepted deliverable. Revisit before any real public release.
- ⚠️ **macOS**: `signingIdentity` is the self-signed `vibe-control-dev`; the shipped build is
  **ad-hoc** signed (`APPLE_SIGNING_IDENTITY=-`), not notarized. `spctl` rejects it → judges on
  another Mac must bypass Gatekeeper once (see §8). Acceptable for the hackathon.
- N/A **Windows**: no Authenticode cert; Windows installers not built this round (needs a Windows
  host anyway — no cross-compile).
- N/A Auto-update feed — not in scope for 0.1.0.

## 4b. Distribution channel & CLI install — **[Bolt R1, 2026-09-09]** (FR-14 / AC-23)
- ✅ **Release pipeline authored** — `.github/workflows/release.yml`: tag-driven, builds
  macOS-universal `.dmg` + Windows `.msi`/`-setup.exe` on their native runners (no cross-compile),
  uploads to a GitHub Release, publishes when all assets are up.
- ✅ **One-line install authored** — `install.sh` (macOS, curl-pipe, clears quarantine) + `install.ps1`
  (Windows, `irm|iex`, silent install). Assets discovered via the Releases API (version-agnostic).
- ✅ **Pipeline exercised — v0.1.0 published (2026-09-09).** Tag `v0.1.0` triggered the workflow;
  **all four jobs green** (create-release / build-macos / build-windows / publish). The first run
  surfaced a real Windows blocker — WiX bundling failed with `Couldn't find a .ico icon` because
  `tauri.conf.json` `bundle.icon` listed only the png; fixed by adding `icon.ico`/`icon.icns` to the
  list (the `.ico` was already in the repo). Re-run published the release with 5 assets:
  `vibe-control_0.1.0_universal.dmg`, `..._x64_en-US.msi`, `..._x64-setup.exe`, `install.sh`,
  `install.ps1`. **Windows is distributable (confirmed)** — both `.msi` (WiX) and `-setup.exe` (NSIS)
  build and upload. `install.ps1` asset resolution dry-run-verified on Windows PowerShell (picks the
  `-setup.exe`, URL resolves).
- ⏳ **Full install E2E** — the one-line install has been verified to *resolve* the correct asset, but
  running it to completion (which modifies the machine) on a clean macOS + Windows box is left as a
  final manual check.
- ⚠️ Installers inherit the **unsigned/ad-hoc** posture (§4): macOS install clears Gatekeeper
  quarantine; Windows shows SmartScreen "Unknown publisher". Fine for internal/demo; a public release
  still needs the certs in §2/§4.

## 8. Running the hackathon build on another Mac (unsigned)
The delivered `~/Desktop/vibe-control_0.1.0_universal.dmg` is a **universal binary** (`lipo -archs` →
`x86_64 arm64`), so it runs on **both Apple Silicon and Intel** Macs. Because it is unsigned/not
notarized, a judge who *downloads* it must clear the quarantine flag once:
- **GUI:** right-click the app → **Open** → **Open** (confirms past Gatekeeper), or
- **Terminal:** `xattr -dr com.apple.quarantine "/path/to/vibe-control.app"` (verified 2026-09-09).

On macOS the app also requests **Automation + Accessibility** permission on first run (needed to read
windows / browser tabs); grant it in System Settings → Privacy & Security.

## 5. First-run / permissions UX (macOS)
- ⚠️ App requires **Automation + Accessibility** TCC grants (browser tab / per-window control). Grants
  persist across rebuilds thanks to the stable self-signed identity, but a **distribution-signed**
  build is a new identity → users grant fresh. Verify the first-run prompt/guidance still fires on a
  Developer-ID-signed build.

## 6. Versioning & traceability
- ✅ Version `0.1.0` and identifier `com.vibecontrol.desktop` set in `tauri.conf.json`.
- ⏳ Tag the release commit (`git tag v0.1.0`) at cut time so the binary maps to source.

## 7. Maintenance backlog (post-release queue)
- ⏳ Tracked in `../known-deviations.md`: P1 saved-resource status display (FR-7 `evaluate` wiring),
  P2 (D1 duplicate-identity invariant, item edit, bundle rename, user docs), P3 (B5 tray/global
  hotkey, B6, A1–A4, splash hi-DPI). This is the ongoing maintenance queue, not a release blocker.

---

## Go / no-go summary

| Release track | Verdict |
|---|---|
| **Hackathon / demo build** (ad-hoc signed, universal macOS) | ✅ **GO — shipped.** Build+test gates green; universal dmg on Desktop; signing waived per hackathon decision; run instructions in §8. |
| **Public distribution** (handed to arbitrary users) | **NO-GO / out of scope** — would require Apple Developer ID + notarization (macOS) and Windows Authenticode, plus closing §2 macOS E2E and the open H3/H4 runtime items. Deferred beyond the hackathon. |

**Status:** the hackathon track is **complete** — `~/Desktop/vibe-control_0.1.0_universal.dmg`
(universal x86_64+arm64, ad-hoc signed, launch-verified). No further operational step is required for
the hackathon. If this later goes public, the only remaining work is distribution signing/notarization
(certs) + the deferred verification items.

**Distribution automation (Bolt R1, 2026-09-09):** a tag-driven GitHub Releases pipeline
(`.github/workflows/release.yml`) + one-line installers (`install.sh` / `install.ps1`) are now in the
repo (FR-14). This replaces the manual "build locally, hand off the dmg" flow. **Not yet exercised** —
push a `v*` tag to run it (see §4b for the AC-23 verification steps). Still ad-hoc/unsigned; the
install scripts handle the quarantine/SmartScreen consequences.

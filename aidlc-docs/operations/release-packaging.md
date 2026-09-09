# Release & Packaging — vibe-control

How to turn the verified CONSTRUCTION build into distributable desktop installers for macOS and
Windows. Grounded in `crates/vc-app/tauri.conf.json` (verified 2026-09-09).

> **Platform rule:** Tauri does **not** cross-compile. You must build the macOS bundle **on macOS**
> and the Windows bundle **on Windows**. There is no single machine that emits both.

---

## <a name="build"></a>1. Build sequence (per target OS)

`beforeBuildCommand` is now set to `npm --prefix ../frontend run build` (hardened 2026-09-09,
verified — the build log shows `Running beforeBuildCommand …` → vite build → compile), so `tauri
build` **auto-rebuilds the frontend** into `frontend/dist` before bundling. The frontend can no
longer go stale. You still need `npm install` once (or after dependency changes) so the CLI + deps
are present.

```
# 0. One-time / after dependency changes: install frontend deps (incl. the tauri CLI)
npm --prefix frontend install

# Build + bundle the desktop app (run ON the target OS).
# beforeBuildCommand auto-runs `npm run build` first, so frontend/dist is always fresh.
npm --prefix frontend exec tauri build
#    -> release binary + installers under target/release/bundle/

# macOS UNIVERSAL build (runs on BOTH Intel + Apple Silicon) — used for the hackathon deliverable:
npm --prefix frontend exec tauri build -- --target universal-apple-darwin
#    requires both rustup targets: aarch64-apple-darwin + x86_64-apple-darwin
#    -> target/universal-apple-darwin/release/bundle/dmg/vibe-control_0.1.0_universal.dmg
```

`bundle.targets` is `"all"`, so on each host Tauri emits every installer type applicable to that host:

| Host OS | Artifacts under `target/release/bundle/` |
|---|---|
| **macOS** | `macos/vibe-control.app`, `dmg/vibe-control_0.1.0_<arch>.dmg` |
| **Windows** | `msi/vibe-control_0.1.0_x64_en-US.msi` (WiX), `nsis/vibe-control_0.1.0_x64-setup.exe` (NSIS) |

> **✅ Done (2026-09-09):** `beforeBuildCommand` was set to `npm --prefix ../frontend run build` in
> `tauri.conf.json`, mirroring `beforeDevCommand`. The `../frontend` path is correct because the CLI
> runs from `crates/vc-app` (cwd nuance per `known-deviations.md#H1`). Verified: a `tauri build` run
> logged `Running beforeBuildCommand 'npm --prefix ../frontend run build'` and rebuilt `frontend/dist`
> before bundling. `frontendDist`/`devUrl` unchanged → production embed + dev server unaffected.

---

## <a name="signing"></a>2. Signing & notarization — WAIVED for the hackathon

> **Hackathon decision (2026-09-09): code-signing certificates are out of scope.** The shipped build
> is **ad-hoc** signed (`APPLE_SIGNING_IDENTITY=-`, since the named `vibe-control-dev` identity is not
> in this machine's keychain). Judges clear Gatekeeper once — see
> `production-readiness-checklist.md` §8. The rest of this section is the *future* path for a real
> public release only.

The config's `macOS.signingIdentity: "vibe-control-dev"` is the **local self-signed dev identity**
(created so macOS Automation/Accessibility grants survive rebuilds — see the automation-permission
note in memory / `known-deviations.md`). It is **fine for local runs, invalid for distribution.**

### macOS
- **Today:** self-signed → other Macs show *"vibe-control is damaged / cannot be opened"* (Gatekeeper).
- **To distribute:** obtain an **Apple Developer ID Application** certificate, then:
  - Set `bundle.macOS.signingIdentity` to the Developer ID (or via `APPLE_SIGNING_IDENTITY` env).
  - **Notarize** the `.app`/`.dmg` (`xcrun notarytool submit … --wait`) and **staple**
    (`xcrun stapler staple`). Tauri can drive this when `APPLE_ID` / `APPLE_PASSWORD` (app-specific)
    / `APPLE_TEAM_ID` are set.
  - Consider enabling the **hardened runtime** and declaring the Automation/Accessibility usage —
    the app already requires those TCC permissions at first run.

### Windows
- **Today:** no code-signing cert at all → SmartScreen "Unknown publisher" warning.
- **To distribute:** obtain an **Authenticode / OV or EV code-signing certificate**; sign the
  `.msi`/`.exe` (Tauri `bundle.windows.certificateThumbprint` + `signCommand`, or post-build
  `signtool`). EV avoids the SmartScreen reputation cold-start.

> Until a real distribution certificate exists, treat any produced installer as **internal / testing
> only** and say so when handing it to anyone.

---

## 3. Versioning & identity

- **Version** `0.1.0` lives in `tauri.conf.json` (`version`). Bump it there per release; it flows
  into the bundle/installer filenames and OS "About" metadata.
- **Bundle identifier** `com.vibecontrol.desktop` — stable; do not change (it keys OS-level state,
  TCC permission grants, and the settings/data file location).
- Tag the release commit (`git tag v0.1.0`) so the shipped binary is traceable to source.

---

## 4. What ships vs. what stays local

- **Data**: bundle/session data is a single local JSON file (atomic write, `vc-store`); it stays on
  the user's machine (NFR-S1). Nothing to provision or migrate server-side.
- **Network egress**: the only outbound path is the **in-app Claude console → AWS Bedrock over
  HTTPS** (NFR-S3, `known-deviations.md#F1`) and the **CloudWatch account-usage meter**. Both require
  the user's own AWS credentials from `~/.aws/credentials`; neither is bundled. Call this out in
  release notes / privacy text — it is the one place user input leaves the machine.

---

## 5. Not in scope (no cloud infra)

No servers, containers, CDNs, or CI/CD are defined for this project (Infrastructure Design was
skipped). If auto-update is desired later, Tauri's updater plugin + a static release feed would be the
natural addition — out of scope for 0.1.0.

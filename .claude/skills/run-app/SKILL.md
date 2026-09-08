---
name: run-app
description: Build and launch the vibe-control Tauri desktop app on Windows. Use whenever asked to run, start, build, or screenshot the app, or to confirm a change works in the real window (not just tests).
---

# Running vibe-control

vibe-control is a **Tauri v2 desktop app**: a Rust workspace (`crates/*`,
binary crate `vc-app` → `vibe-control.exe`) plus a React/Vite frontend
(`frontend/`). The frontend is built to `frontend/dist` and embedded into
the Rust binary at compile time — so **no Tauri CLI is needed** to run it.

## Prerequisites (verified present on this machine)

- **Node.js / npm** — for the frontend build.
- **Rust MSVC toolchain** — `stable-x86_64-pc-windows-msvc`. `cargo` lives
  at `C:\Users\gayeon\.cargo\bin` and is **not on the bash PATH**, so every
  cargo command must prepend it (see below).
- **MSVC linker** — from Visual Studio 2022 (Community/Build Tools), already
  installed. No extra install needed.
- **WebView2** — present (ships with Windows 11).

If `cargo` is missing on a fresh machine: `winget install --id Rustlang.Rustup -e`
(installs the msvc toolchain by default). It relies on the VS2022 MSVC linker.

## Build and run

Cargo is not on PATH — prepend it in each shell:

```bash
export PATH="$PATH:/c/Users/gayeon/.cargo/bin"
```

1. **Frontend deps** (first time only):
   ```bash
   cd frontend && npm install
   ```
2. **Build the frontend → `frontend/dist`** (re-run after any frontend edit —
   this path has no hot reload):
   ```bash
   cd frontend && npm run build
   ```
3. **Build the Rust app** (first build ~3 min; incremental is fast):
   ```bash
   cd <repo root> && cargo build -p vc-app
   ```
4. **Launch** the actual window:
   ```bash
   ./target/debug/vibe-control.exe
   ```
   Run it in the background; it does not print to stdout on success.

## Verify it's really up

The binary is silent on success, so check the process rather than the log:

```bash
sleep 4; tasklist | grep -i "vibe-control"   # a PID line = window is up
```

An empty output log with a live PID means the WebView loaded fine. If the
process exits immediately, read the task output file for the panic/error.

## Notes

- **Frontend edits are NOT hot-reloaded** with this path — re-run `npm run build`
  then relaunch the exe. For hot-reload dev, install the Tauri CLI
  (`cargo install tauri-cli`, long compile) and use `cargo tauri dev`, which
  runs the vite dev server (`beforeDevCommand` in `crates/vc-app/tauri.conf.json`)
  on port 1420.
- The dev window is 1000×700, title "vibe-control", background `#d4d1c9`.

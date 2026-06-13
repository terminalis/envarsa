# Envarsa

A local-first library for your environment values.

Env values scatter across projects and machines, get silently duplicated, and die with a disk.
Envarsa gives them one home: capture a project's `.env` as a point-in-time snapshot, recall and
inspect it with values masked, and hand values back out — by clipboard or export — when you need
a project working again. No cloud, no telemetry, and — apart from an optional update check
against GitHub, off by default — no egress. The files never leave.

![Envarsa](screenshots/demo.png)

## Install

Grab an artifact from the
[latest release](https://github.com/terminalis/envarsa/releases/latest):

**Windows 10/11, x64:**

- **Installer** (`Envarsa_x.y.z_x64-setup.exe`) — per-user NSIS install, no admin prompt.
  Fetches the WebView2 runtime on the rare machine that lacks it.
- **Portable** (`envarsa_x.y.z_x64_portable.zip`) — unzip anywhere, run `envarsa.exe`. The
  `WebView2Loader.dll` beside it must stay there. No installer, no admin, nothing on PATH.

**Linux, x64:**

- **AppImage** (`Envarsa_x.y.z_amd64.AppImage`) — `chmod +x`, run. The webview (WebKitGTK) and
  OpenSSL travel inside the file: no install, no root, any distro with glibc 2.35+
  (Ubuntu 22.04+, Debian 12+, and peers).
- **Debian package** (`Envarsa_x.y.z_amd64.deb`) — `sudo apt install ./Envarsa_x.y.z_amd64.deb`;
  apt pulls the dependencies (WebKitGTK 4.1, libssl3) and adds the menu entry.

If the window comes up blank on first launch, a few GPU/driver and Wayland combinations trip
over WebKitGTK's DMABUF renderer — relaunch with `WEBKIT_DISABLE_DMABUF_RENDERER=1` set.

Three things to know up front:

- The Windows builds need the **WebView2 runtime** — preinstalled on Windows 11 and on any
  Windows 10 kept current. The portable build doesn't fetch it; if the app won't start, install
  it [from Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).
- The binaries are **unsigned**, so first launch on Windows shows a SmartScreen prompt —
  *More info* → *Run anyway*.
- **Portable means no install, not data-on-a-stick.** The store still lives in
  `%APPDATA%\com.envarsa.app` (Linux: `~/.local/share/com.envarsa.app`), not next to the
  executable — moving the folder doesn't move your data. Repoint the store in Settings if you
  want it somewhere you sync.

## What it is

Envarsa is a **local-first librarian**:

- It **organizes, copies, and exports**, and never injects variables into processes. The one
  way it writes into a project tree is an explicit, guarded export to a `.env*.local` file
  (never a git-committed example file like `.env.example`, where secrets would leak) — always
  initiated by you, on a path you choose.
- **Build values without a file.** Capture from a `.env`, paste them, or type them into the
  **structured editor** (keeping `#` comments) — a project always lives in the store, even when
  you never import or export a file. Import a `.env.example` for its comments and key labels,
  then write a `.env.local` beside it with your values filled in.
- **Capture is manual.** A snapshot is a moment in time; re-capture or edit to update. Envarsa
  never watches project files for drift.
- **A project's identity is its name**, chosen by you. Any stored path is a non-binding hint —
  nothing rebinds by path across machines.
- **Masking is structural, not heuristic.** An entry is an env var: its label (the key) is always
  visible, its value is always masked until you reveal it, one value at a time. Revealed values
  auto-hide after 30 seconds and whenever the window loses focus. Unparseable lines are masked
  too — a malformed line is as likely as any to hold a secret.
- **Reuse is detected and flagged, never linked.** When a key also lives in other projects, a
  badge shows where — and whether the value there is the same or different. Editing one place
  never propagates anywhere.

## The store file

Everything lives in **one portable file** (`envarsa.store`):

- **Plaintext, pretty-printed JSON by default.** You own the bytes — read them, diff them,
  back them up with anything. Format: `envarsa-store` v1, raw snapshot text embedded verbatim.
- **Durable by construction.** Every save is atomic (temp file + fsync + rename) and keeps a
  one-step `.bak` sibling. A corrupt store is never overwritten; the app offers the backup —
  restoring is only possible from that corrupt-store gate, never against a healthy session.
  Protection transitions rewrite the backup too: enabling encryption or changing the passphrase
  never leaves a `.bak` behind under the old (or no) protection.
- **Optional encryption at rest** with a passphrase — standard [age](https://age-encryption.org)
  format (scrypt). The promise survives the tool: `age -d envarsa.store > store.json` works with
  the reference CLI. There is **no recovery** from a lost passphrase; that is why it's opt-in and
  masking + full-disk encryption is the default posture.
- **Portability is manual and yours.** Export a copy from Settings — optionally age-encrypted
  with a transport passphrase — and import it on the other machine: its projects merge into the
  library there, with name conflicts flagged per project (replace yours, rename the incoming
  one, or skip it). Or keep the live file in a folder you sync yourself (Settings → Change location…).

Location: `%APPDATA%\com.envarsa.app\envarsa.store` by default
(Linux: `~/.local/share/com.envarsa.app/envarsa.store`); overridable in Settings
(persisted in `config.json`) or with the `ENVARSA_STORE_PATH` env var.

## Core loop

1. **Capture** — import a `.env` via file picker, paste, or drop the file on the window.
   Comments, blank lines, and even unparseable lines are kept byte-for-byte. No file? Build the
   project by hand in the **structured editor** instead (step 6) — either way a project lives in
   the store from the start.
2. **Recall** — find the project, see keys, comments, history. Duplicate keys are tagged
   `overridden`; shared keys carry a reuse badge.
3. **Hand back** — copy one value (it goes core → clipboard without ever rendering on screen),
   copy the whole block, or export a `.env` wherever *you* choose. Exports are byte-identical to
   what was captured. On Windows, copies are marked to stay out of the clipboard history (Win+V)
   and the cross-device cloud clipboard — Linux has no such flag (see the security note) — and
   Envarsa clears the clipboard after 30 seconds if it still holds what was copied.
4. **History** — every capture or save appends a snapshot. View older ones read-only, or bring
   one back as latest.
5. **Write a `.env.local`** — when you want the snapshot on disk, write it into a `.env*.local`,
   either at its remembered source directory or a folder you pick. **Merge** keeps the target's
   own comments, blank lines, and keys the snapshot doesn't provide (source-only keys are appended
   under `# Added by Envarsa`); **fresh** (overwrite) replaces the file with the snapshot's current
   values, re-serialized with minimal quoting — a needlessly-quoted `"plain-text"` comes back as a
   bare `plain-text`. A preview counts what will be added, updated, kept, or blanked before any
   byte is written. Or point Envarsa at a `.env.example`: it reads the file for its comments and
   key labels (never writes to it) and scaffolds a `.env.local` beside it, **blanking** any key the
   snapshot can't fill to `KEY=` so placeholders like `changeme` and `3000` never leak in. The
   write is atomic — temp file, rename — and leaves **no `.bak` in your project tree** (backups
   exist only for Envarsa's own store file). Only `.env*.local` paths are writable: the guard
   (`classify_name` in `envpath.rs`) blocks any `example`/`sample`/`template`/`dist` segment
   outright (those markers win even over a `.local` suffix), since they're committed and would leak
   secrets into git history. The target path is resolved and staged in Rust behind an opaque token —
   it never originates in or returns to the webview. Writing requires an unlocked store.
6. **Build or edit by hand** — a project can live in the store with no file at all. Open the
   **structured editor** empty, add keys, values, and `#` comments in a form, name it, and save —
   the result is a store-only snapshot (`via=edit`, no source path). Load an existing snapshot to
   change entries, toggle the `export` prefix, or add comments, then save as a new snapshot; the
   old one stays in history. Keys are validated before the snapshot is created — no empty keys, no
   whitespace, `#`, or quotes — and you're told why if one is rejected.

## Security posture, honestly

Masking is a *shoulder-surfing* defense, not encryption. The baseline secrets boundary is your
full-disk encryption, same as the plaintext `.env` files you already keep. Envarsa concentrates
values into one file — mitigated by masking, atomic saves with backup, and opt-in passphrase
encryption. The webview is sandboxed to the app's own commands (CSP `default-src 'self'`, no
plugin surface exposed to JS); values cross into the UI only on explicit per-value reveal, and
single-value copies never transit the UI at all. No registered command takes a filesystem path
from the webview: dropped files are read on the Rust side of the window event, and store imports
are keyed by an opaque token minted when you pick the file — preview and apply only accept that
token. Clipboard copies are cleared after 30 seconds; on Windows they are also excluded from
the clipboard history / cloud sync. Linux has no equivalent exclusion flag: a clipboard manager
(KDE's Klipper, CopyQ, GNOME clipboard extensions) records a copy the moment it lands, and the
30-second clear does not purge the manager's own history. Nothing the clipboard API offers
changes that today — if you run a clipboard manager, pause it or clear its history after
copying a secret.

The single deliberate exception to no-egress is the update check: a GET to `api.github.com` for
the latest release tag, made only when you click *Check for updates* or opt into the daily
automatic check (off by default). The reply is treated as untrusted input — nothing but a
semver-validated version number is ever used — and nothing downloads or installs itself; the
"get it" button just opens the releases page in your browser. All of it lives in one module
(`update.rs`), the app's entire network surface.

## Build

Prereqs on Windows: Rust (the `x86_64-pc-windows-gnu` toolchain works without Visual Studio —
see note), Node for the Tauri CLI, WebView2 runtime (ships with Windows 11). On Linux: Rust,
Node, and the Tauri system packages —
`sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev librsvg2-dev libssl-dev`.

```
npm install
npm run dev      # run the app (tauri dev)
npm run build    # Windows: release exe + NSIS installer · Linux: AppImage + .deb
npm run package:portable   # Windows: zip the exe + WebView2Loader.dll from the last build
```

One-time staging on a fresh Windows clone: `tauri.windows.conf.json` bundles
`target/release/WebView2Loader.dll` as a resource, and tauri-build refuses to build anything —
even `cargo test` — until that file exists. The DLL ships inside the `webview2-com-sys` crate;
the *Stage WebView2Loader.dll* step in `.github/workflows/release.yml` is the two-command recipe
(build `-p webview2-com-sys`, copy out of its `out/x64/`). Linux needs no staging.

Engine tests and the end-to-end selftest:

```
cd src-tauri && cargo test                    # parser, store, crypto
$env:ENVARSA_SELFTEST='1'                     # drives the real app through
$env:ENVARSA_STORE_PATH="$env:TEMP\st.envarsa"  # capture/reveal/copy/export/
.\target\debug\envarsa.exe                    # encrypt/lock/unlock/import, prints a report
```

(Linux, one line: `ENVARSA_SELFTEST=1 ENVARSA_STORE_PATH=/tmp/st.envarsa ./target/debug/envarsa`)

`ENVARSA_DEMO=1` seeds sample projects into an *empty* store (screenshots, trying it out).

> **windows-gnu note:** rustc must keep its self-contained linker — do **not** expose
> `x86_64-w64-mingw32-gcc` on PATH (a foreign GCC's CRT clashes with rustup's bundled MinGW
> objects). The resource step needs binutils' `windres`/`dlltool`/`as` plus an *unprefixed*
> `gcc` for `.rc` preprocessing; a stock MinGW-w64 with its prefixed aliases removed satisfies
> both constraints. MSVC toolchains need none of this.

## Layout

```
ui/            no-build frontend (ES modules, withGlobalTauri; mock.js = browser-preview stub)
src-tauri/     Rust core: envfile.rs (parse + serialize + merge), envpath.rs (.env*.local
               write guard), store.rs (atomic persistence), crypto.rs (age),
               commands.rs (the entire IPC surface), state.rs (session)
```

## License

MIT — see [LICENSE](LICENSE).

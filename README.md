# Envarsa

A local-first library for your environment values.

Env values scatter across projects and machines, get silently duplicated, and die with a disk.
Envarsa gives them one home: capture a project's `.env` as a point-in-time snapshot, recall and
inspect it with values masked, and hand values back out — by clipboard or export — when you need
a project working again. No cloud, no telemetry, and — apart from an optional update check
against GitHub, off by default — no egress. The files never leave.

![Envarsa](screenshots/demo.png)

## Install

Windows 10/11, x64. Grab either artifact from the
[latest release](https://github.com/terminalis/envarsa/releases/latest):

- **Installer** (`Envarsa_x.y.z_x64-setup.exe`) — per-user NSIS install, no admin prompt.
  Fetches the WebView2 runtime on the rare machine that lacks it.
- **Portable** (`envarsa_x.y.z_x64_portable.zip`) — unzip anywhere, run `envarsa.exe`. The
  `WebView2Loader.dll` beside it must stay there. No installer, no admin, nothing on PATH.

Three things to know up front:

- Both need the **WebView2 runtime** — preinstalled on Windows 11 and on any Windows 10 kept
  current. The portable build doesn't fetch it; if the app won't start, install it
  [from Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).
- The binaries are **unsigned**, so first launch shows a SmartScreen prompt — *More info* →
  *Run anyway*.
- **Portable means no install, not data-on-a-stick.** The store still lives in
  `%APPDATA%\com.envarsa.app`, not next to the exe — moving the folder doesn't move your data.
  Repoint the store in Settings if you want it somewhere you sync.

## What it is

Envarsa is a **store-only librarian**:

- It **organizes, copies, and exports**. It never writes into a project tree and never injects
  variables into processes. "Use these values" always means *you* place them.
- **Capture is manual.** A snapshot is a moment in time; re-capture to update. Envarsa never
  watches project files for drift.
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

Location: `%APPDATA%\com.envarsa.app\envarsa.store` by default; overridable in Settings
(persisted in `config.json`) or with the `ENVARSA_STORE_PATH` env var.

## Core loop

1. **Capture** — import a `.env` via file picker, paste, or drop the file on the window.
   Comments, blank lines, and even unparseable lines are kept byte-for-byte.
2. **Recall** — find the project, see keys, comments, history. Duplicate keys are tagged
   `overridden`; shared keys carry a reuse badge.
3. **Hand back** — copy one value (it goes core → clipboard without ever rendering on screen),
   copy the whole block, or export a `.env` wherever *you* choose. Exports are byte-identical to
   what was captured. Copies are marked to stay out of the Windows clipboard history (Win+V) and
   the cross-device cloud clipboard, and Envarsa clears the clipboard after 30 seconds if it
   still holds what was copied.
4. **History** — every capture appends a snapshot. View older ones read-only, or bring one back
   as latest.

## Security posture, honestly

Masking is a *shoulder-surfing* defense, not encryption. The baseline secrets boundary is your
full-disk encryption, same as the plaintext `.env` files you already keep. Envarsa concentrates
values into one file — mitigated by masking, atomic saves with backup, and opt-in passphrase
encryption. The webview is sandboxed to the app's own commands (CSP `default-src 'self'`, no
plugin surface exposed to JS); values cross into the UI only on explicit per-value reveal, and
single-value copies never transit the UI at all. No registered command takes a filesystem path
from the webview: dropped files are read on the Rust side of the window event, and store imports
are keyed by an opaque token minted when you pick the file — preview and apply only accept that
token. Clipboard copies are excluded from Windows clipboard history / cloud sync and cleared
after 30 seconds.

The single deliberate exception to no-egress is the update check: a GET to `api.github.com` for
the latest release tag, made only when you click *Check for updates* or opt into the daily
automatic check (off by default). The reply is treated as untrusted input — nothing but a
semver-validated version number is ever used — and nothing downloads or installs itself; the
"get it" button just opens the releases page in your browser. All of it lives in one module
(`update.rs`), the app's entire network surface.

## Build

Prereqs: Rust (the `x86_64-pc-windows-gnu` toolchain works without Visual Studio — see note),
Node for the Tauri CLI, WebView2 runtime (ships with Windows 11).

```
npm install
npm run dev      # run the app (tauri dev)
npm run build    # release exe + NSIS installer (per-user, no admin)
npm run package:portable   # zip the exe + WebView2Loader.dll from the last build
```

One-time staging on a fresh clone: `tauri.conf.json` bundles `target/release/WebView2Loader.dll`
as a resource, and tauri-build refuses to build anything — even `cargo test` — until that file
exists. The DLL ships inside the `webview2-com-sys` crate; the *Stage WebView2Loader.dll* step
in `.github/workflows/release.yml` is the two-command recipe (build `-p webview2-com-sys`, copy
out of its `out/x64/`).

Engine tests and the end-to-end selftest:

```
cd src-tauri && cargo test                    # parser, store, crypto
$env:ENVARSA_SELFTEST='1'                     # drives the real app through
$env:ENVARSA_STORE_PATH="$env:TEMP\st.envarsa"  # capture/reveal/copy/export/
.\target\debug\envarsa.exe                    # encrypt/lock/unlock/import, prints a report
```

`ENVARSA_DEMO=1` seeds sample projects into an *empty* store (screenshots, trying it out).

> **windows-gnu note:** rustc must keep its self-contained linker — do **not** expose
> `x86_64-w64-mingw32-gcc` on PATH (a foreign GCC's CRT clashes with rustup's bundled MinGW
> objects). The resource step needs binutils' `windres`/`dlltool`/`as` plus an *unprefixed*
> `gcc` for `.rc` preprocessing; a stock MinGW-w64 with its prefixed aliases removed satisfies
> both constraints. MSVC toolchains need none of this.

## Layout

```
ui/            no-build frontend (ES modules, withGlobalTauri; mock.js = browser-preview stub)
src-tauri/     Rust core: envfile.rs (parse), store.rs (atomic persistence),
               crypto.rs (age), commands.rs (the entire IPC surface), state.rs (session)
```

## License

MIT — see [LICENSE](LICENSE).

# Envarsa

A local-first library for your environment values.

Env values scatter across projects and machines, get silently duplicated, and die with a disk.
Envarsa gives them one home: capture a project's `.env` as a point-in-time snapshot, recall and
inspect it with values masked, and hand values back out — by clipboard or export — when you need
a project working again. No cloud, no telemetry, and — apart from an optional update check
against GitHub, off by default — no egress. The files never leave.

![Envarsa](screenshots/demo.png)

*Full walkthrough, screenshots, and FAQ → [envarsa.dev](https://envarsa.dev).*

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

## How it works

Envarsa is a **local-first librarian**: it organizes, copies, and exports your env values and
**never injects them into processes**. A project always lives in the store — captured from a `.env`,
pasted, or typed into the **structured editor** — and is identified by the **name you give it**, not
a path. Capture is **manual**: a snapshot is a moment in time, and Envarsa never watches your files.

1. **Capture** — import a `.env` by file picker, paste, or drop; comments, blank lines, and
   unparseable lines are kept **byte-for-byte**. No file? Build it by hand in the structured editor.
2. **Recall** — browse a project's keys, comments, and history. Duplicate keys are tagged
   `overridden`; keys shared across projects show a **reuse badge** (same value or different) —
   flagged, never linked.
3. **Hand back** — copy one value (core → clipboard, never on screen), copy the block, or export a
   **byte-identical** `.env`. Copies clear after 30 seconds.
4. **History** — every capture appends a snapshot; view older ones read-only or restore one as latest.
5. **Write a `.env.local`** — write a snapshot to a `.env*.local` you pick: **merge** to fill values
   while keeping the file's comments and keys, or **fresh** to overwrite. A preview counts what
   changes first. Point it at a `.env.example` and it scaffolds a `.env.local`, blanking any key it
   can't fill so placeholders never leak. Only `.env*.local` is writable
   (see [Security posture](#security-posture-honestly)); writing requires an unlocked store.
6. **Build by hand** — type keys, values, and `#` comments into the structured editor and save a
   store-only snapshot. Editing a snapshot saves a **new** one and keeps the old in history.

**Masking is structural, not heuristic.** A key is always visible; its value is dots until you
reveal it, **one at a time**, and auto-hides after 30 seconds and on focus loss. Even unparseable
lines are masked — a malformed line is as likely as any to hold a secret.

See the full walkthrough with screenshots at [envarsa.dev](https://envarsa.dev/#how).

## The store file

Everything lives in **one portable file** (`envarsa.store`):

- **Plaintext, pretty-printed JSON by default.** You own the bytes — read, diff, and back them up
  with anything. Format `envarsa-store` v1, raw snapshot text embedded verbatim.
- **Durable by construction.** Every save is atomic (temp + fsync + rename) and keeps a one-step
  `.bak` sibling. A corrupt store is never overwritten; the app offers the backup, restorable only
  from that corrupt-store gate, never in a healthy session. Enabling encryption or changing the
  passphrase rewrites the backup too — no stale `.bak` under the old protection.
- **Optional encryption at rest** — a passphrase in standard [age](https://age-encryption.org)
  format (scrypt); `age -d envarsa.store > store.json` works with the reference CLI. There is **no
  recovery** from a lost passphrase, which is why it's opt-in.
- **Portability is manual and yours.** Export a copy from Settings (optionally age-encrypted for
  transport) and import it elsewhere: projects merge, name conflicts flagged per project (replace,
  rename, or skip). Or keep the live file in a folder you sync yourself.

Location: `%APPDATA%\com.envarsa.app\envarsa.store` (Linux:
`~/.local/share/com.envarsa.app/envarsa.store`); overridable in Settings (`config.json`) or via
`ENVARSA_STORE_PATH`.

## Security posture, honestly

No theater. **Masking is a shoulder-surfing defense, not encryption** — the baseline boundary is
your full-disk encryption, same as the plaintext `.env` files you already keep. Envarsa
**concentrates** values into one file; that trade is mitigated by masking, atomic saves with a
backup ([above](#the-store-file)), and opt-in passphrase encryption.

- **The webview never sees more than it must.** Sandboxed to the app's own commands (CSP
  `default-src 'self'`, no plugin surface in JS); values reach the UI only on explicit per-value
  reveal, and single-value copies never transit it. **No command takes a filesystem path from the
  webview** — dropped files are read in Rust, and store imports and `.env*.local` write targets are
  resolved behind opaque tokens minted there. Only `.env*.local` is writable; the guard
  (`classify_name` in `envpath.rs`) refuses committed example files — `.env.example`, `.sample`,
  `.template`, `.dist` win even over a `.local` suffix.
- **The clipboard forgets.** Copies clear after 30 seconds; on Windows they're also kept out of
  clipboard history (Win+V) and the cloud clipboard. Linux has no such flag — a clipboard manager
  (Klipper, CopyQ, a GNOME extension) keeps its own copy, so pause it or clear its history after
  copying a secret.
- **One opt-in egress.** Out of the box, no network calls. The single exception is the update check,
  **off by default**: a GET to `api.github.com` for the latest release tag. The reply is untrusted
  input — only a semver-validated version is used — and nothing downloads or installs itself; *get
  it* just opens the releases page. It all lives in one module (`update.rs`), the entire network
  surface.

The long-form writeup — what masking is and isn't, in full — is at
[envarsa.dev](https://envarsa.dev/#security).

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

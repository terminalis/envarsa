# Microsoft Store listing — Envarsa

Copy-paste content for Partner Center. Fill the **Identity** values after reserving the
name (blocker B1). Everything else below is ready to use.

---

## Identity (set after reserving the name in Partner Center)

Partner Center → your app → **Product management → Product identity**. Copy these into BOTH
`Package.appxmanifest` and the packaging payload manifest, then re-run `tools\package-msix.ps1`:

| Field | Manifest target | Value |
|-------|-----------------|-------|
| Package/Identity/Name | `<Identity Name="…">` | _(assigned, e.g. `1234Publisher.Envarsa`)_ |
| Publisher | `<Identity Publisher="…">` | _(assigned `CN=<GUID>`)_ |
| Publisher display name | `<PublisherDisplayName>` | _(assigned)_ |

---

## Product details

- **Display name:** Envarsa
- **Category:** Developer tools
- **Pricing:** Free
- **Markets:** All markets
- **Visibility:** Public
- **Device family:** Windows Desktop (x64)

---

## Short description

> A local-first manager for your `.env` files. Capture a project's environment values as a
> point-in-time snapshot, recall them with values masked, and hand them back by copy or
> export. No cloud, no telemetry.

## Description

> Envarsa is a local-first home for the environment variables that your `.env` files hold.
> Those files are git-ignored on purpose and backed up by nothing — they scatter across
> projects and machines, duplicate, drift, and disappear. Envarsa gives them one durable
> place to live, entirely on your own machine.
>
> Capture a project's `.env` as a point-in-time snapshot — by file picker, paste, or drag and
> drop — preserving comments and blank lines byte-for-byte. Recall any project by the name you
> gave it, with every value masked until you reveal it one at a time. Hand values back when you
> need them: copy a single value straight to the clipboard (never shown on screen, cleared
> after 30 seconds), copy a whole block, or export a byte-identical `.env`. You can also write
> a `.env*.local` you choose — merged or fresh — or scaffold one from a `.env.example`.
>
> Everything lives in one portable store file with atomic saves and a one-step backup. The
> store is plaintext JSON by default — you own the bytes — with optional passphrase encryption
> at rest in the standard age format. There is no cloud, no account, and no telemetry; aside
> from an opt-in update check that is off by default, Envarsa makes no network connections.
>
> Free and open source (MIT), built with a Rust core and a no-build web frontend.

**Features**
- Capture `.env` files as point-in-time snapshots (file picker, paste, or drag and drop)
- Structural masking — keys always visible, values hidden until revealed one at a time
- Per-value clipboard copy that bypasses the screen and clears after 30 seconds
- Byte-identical `.env` export
- Guarded write-back to a `.env*.local` you choose (merge or fresh), or scaffolded from a `.env.example`
- Build or edit a project by hand in a structured editor
- Snapshot history per project
- Cross-project key-reuse detection — flagged, never linked
- One portable store file with atomic saves and a one-step backup
- Optional passphrase encryption at rest (standard age format)
- No cloud, no telemetry — no network egress beyond an opt-in, off-by-default update check

## Search terms (up to 7)

`env vars`, `environment variables`, `dotenv`, `.env`, `secrets`, `env file`, `developer tools`

## What's new (1.0.0)

> First Microsoft Store release of Envarsa.

---

## Required URLs

- **Privacy policy (REQUIRED):** `https://envarsa.dev/privacy` — page is in the repo at `privacy/index.html`; publish it before submitting.
- **Support / website:** `https://envarsa.dev`
- **Support contact:** `https://github.com/terminalis/envarsa/issues`

## Properties → personal data

Answer **Yes** to the personal-data question and supply the privacy-policy URL. Envarsa itself
collects nothing; the disclosure exists because the opt-in update check transmits the device IP
and a `envarsa/<version>` User-Agent to GitHub. Keep the wording consistent with `privacy/index.html`.

---

## Screenshots (≥ 1 required)

Desktop screenshots must be **1366×768 or 1920×1080 PNG** (up to 9).

⚠️ The existing `screenshots/*.png` are 1122×751 / 1136×759 — **below the 1366×768 minimum**.
Recapture at ≥ 1366×768 before submitting. Suggested shots: main window with masked values,
the capture dialog, the by-hand editor, and Settings (encryption).

---

## runFullTrust justification (Submission options)

`runFullTrust` is a restricted capability; Partner Center flags it with a validation **warning**
(not a rejection) and asks for a justification. Paste:

> Envarsa is a packaged classic Win32 application (Tauri / WebView2) running as
> `mediumIL` / `packagedClassicApp`, so `runFullTrust` is required by the platform to install and
> launch the packaged process. It is not used to elevate privileges or launch external
> processes; no `allowElevation`, `FullTrustProcessLauncher`, or `broadFileSystemAccess` is
> declared.

---

## Age rating (IARC)

Launch the IARC questionnaire in the **Age ratings** section. Choose the utility / productivity
(non-game) category and answer truthfully: no violence, sexual, gambling, or mature content;
declare the optional network/update behavior honestly where asked. Click **Save and generate**.

---

## Notes

- The submitted `.msix` is **unsigned** — the Store re-signs it with a Microsoft certificate on
  ingestion. Do **not** buy or apply a code-signing certificate for the MSIX.
- WACK passes overall; the only failure is the optional S-Mode "Blocked executables" test, which
  is excluded from certification and only affects availability on Windows S Mode devices.
- The direct-download NSIS `.exe` on envarsa.dev / GitHub is a separate channel with its own
  (unsigned, SmartScreen-prompted) story — out of scope for this Store listing.

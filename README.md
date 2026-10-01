# Envarsa

A local-first store for your `.env` files — capture, recall masked, hand back by clipboard or export. One
JSON file on disk; no cloud, no telemetry, no egress except an opt-in update check (off by default).

[Download](https://github.com/terminalis/envarsa/releases/latest) · [Microsoft Store](https://apps.microsoft.com/detail/9NQCBXD2WQ2M) · [envarsa.dev](https://envarsa.dev)

## Build

Prereqs — **Windows:** Rust (`x86_64-pc-windows-gnu`, see note), Node, WebView2. **Linux:** Rust, Node, and
`sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev librsvg2-dev libssl-dev`.

```
npm install
npm run dev      # tauri dev
npm run build    # Windows: exe + NSIS installer · Linux: compiles the app only
cd src-tauri && cargo test     # includes every IPC command end to end
bash tools/check-contract.sh   # the webview and the core agree on commands and events
```

After a Windows build, `tools/package-msix.ps1` packages the MSIX for the Microsoft Store. The Flatpak is built
with flatpak-builder from `flatpak/`; [flatpak/README.md](flatpak/README.md) covers building, running and linting
it locally.

The store lives in the app's data folder: `%APPDATA%\com.envarsa.app\` on Windows (installer and Store),
`~/.var/app/dev.envarsa.Envarsa/data/` in the Flatpak, and `~/.local/share/dev.envarsa.Envarsa/` for a Linux dev
build. `ENVARSA_STORE_PATH` overrides it; `ENVARSA_DEMO=1` seeds sample projects. Serving `ui/` from
any static server opens the UI in a plain browser against canned responses (`ui/js/mock.js`).

The app version lives only in `src-tauri/Cargo.toml`; Tauri and the release workflow read it from there.
`src-tauri/icons/` keeps just the icons the bundles use. `npx tauri icon <source.png>` regenerates the full set
(including iOS, Android and macOS files this project doesn't ship); commit only the ones `tauri.conf.json`
lists, plus `icon.png`. The MSIX tiles live separately in `Assets/`.

**Windows build notes**

- Fresh clone: `tauri.windows.conf.json` bundles `target/release/WebView2Loader.dll` as a resource, and
  tauri-build won't build (even `cargo test`) until it exists. The *Stage WebView2Loader.dll* step in
  `.github/actions/setup-windows-gnu/action.yml` is the recipe. Linux needs no staging.
- **windows-gnu:** rustc must keep its self-contained linker — do **not** put `x86_64-w64-mingw32-gcc` on PATH
  (its CRT clashes with rustup's MinGW objects). The resource step needs binutils' `windres`/`dlltool`/`as`
  plus an *unprefixed* `gcc`; a stock MinGW-w64 with prefixed aliases removed satisfies both. MSVC needs none.

## Distribution

- **Windows installer:** `Envarsa_x.y.z_x64-setup.exe` on [GitHub Releases](https://github.com/terminalis/envarsa/releases/latest).
- **Microsoft Store:** the MSIX from the release run, uploaded to Partner Center.
- **Flatpak:** `Envarsa_x.y.z_x86_64.flatpak` on GitHub Releases now, and on Flathub once it passes review.

A version tag runs `.github/workflows/release.yml`, which builds all three and publishes the installer and the
bundle.

## Layout

`ui/` is the no-build frontend and `src-tauri/` the Rust core. [ARCHITECTURE.md](ARCHITECTURE.md) maps every
module, follows a request from the webview to disk, and names the rules the code keeps.

## License

MIT — see [LICENSE](LICENSE).

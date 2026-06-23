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
npm run build    # Windows: exe + NSIS · Linux: AppImage + .deb
cd src-tauri && cargo test
```

End-to-end selftest: `ENVARSA_SELFTEST=1 ENVARSA_STORE_PATH=/tmp/st.envarsa ./target/debug/envarsa`.
`ENVARSA_STORE_PATH` overrides the store location; `ENVARSA_DEMO=1` seeds sample projects.

**Windows build notes**

- Fresh clone: `tauri.windows.conf.json` bundles `target/release/WebView2Loader.dll` as a resource, and
  tauri-build won't build (even `cargo test`) until it exists. The *Stage WebView2Loader.dll* step in
  `.github/workflows/release.yml` is the recipe. Linux needs no staging.
- **windows-gnu:** rustc must keep its self-contained linker — do **not** put `x86_64-w64-mingw32-gcc` on PATH
  (its CRT clashes with rustup's MinGW objects). The resource step needs binutils' `windres`/`dlltool`/`as`
  plus an *unprefixed* `gcc`; a stock MinGW-w64 with prefixed aliases removed satisfies both. MSVC needs none.

## Layout

```
ui/         no-build frontend (ES modules, withGlobalTauri; mock.js = browser-preview stub)
src-tauri/  Rust core: envfile.rs (parse/serialize/merge), envpath.rs (.env*.local guard),
            store.rs (atomic persistence), crypto.rs (age), commands.rs (IPC), state.rs (session)
```

## License

MIT — see [LICENSE](LICENSE).

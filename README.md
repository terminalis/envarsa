# Envarsa

A local-first store for your `.env` files — capture, recall masked, hand back by clipboard or export. One
JSON file on disk; no cloud, no telemetry, no egress except an opt-in update check (off by default).

[Microsoft Store](https://apps.microsoft.com/detail/9NQCBXD2WQ2M) · [Flatpak](https://github.com/terminalis/envarsa/releases/latest)

## Build

Prereqs — **Windows:** Rust (the default MSVC toolchain), Visual Studio Build Tools with the "Desktop
development with C++" workload, Node, WebView2. **Linux:** Rust, Node, and
`sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev librsvg2-dev libssl-dev xdg-utils`.

```
npm install
npm run dev      # tauri dev
npm run build    # Windows: envarsa.exe · Linux: the AppImage
cd src-tauri && cargo test     # includes every IPC command end to end
bash tools/check-contract.sh   # the webview and the core agree on commands and events
```

After a Windows build, `tools/package-msix.ps1` packages the MSIX for the Microsoft Store. The Flatpak is built
with flatpak-builder from `flatpak/`; [flatpak/README.md](flatpak/README.md) covers building, running and linting
it locally.

The downloads are portable: the Windows exe and the AppImage keep the store and `config.json` in an
`Envarsa Data` folder beside themselves, so the library moves with the folder. On first run, a download
offers to copy the library an installed copy kept. The Microsoft Store build uses `%APPDATA%\com.envarsa.app\`,
the Flatpak `~/.var/app/io.github.terminalis.Envarsa/data/`, and dev builds the app's data folder
(`%APPDATA%\com.envarsa.app\`, or `~/.local/share/io.github.terminalis.Envarsa/` on Linux).
`ENVARSA_STORE_PATH` overrides it; `ENVARSA_DEMO=1` seeds sample projects. Serving `ui/` from
any static server opens the UI in a plain browser against canned responses (`ui/js/mock.js`).

The app version lives only in `src-tauri/Cargo.toml`; Tauri and the release workflow read it from there.
`src-tauri/icons/` keeps just the icons the bundles use. `npx tauri icon <source.png>` regenerates the full set
(including iOS, Android and macOS files this project doesn't ship); commit only the ones `tauri.conf.json`
lists, plus `icon.png`. The MSIX tiles live separately in `Assets/`.

## Distribution

- **Windows:** `Envarsa_x.y.z_x64.exe` on [GitHub Releases](https://github.com/terminalis/envarsa/releases/latest),
  portable, with no installer.
- **Linux AppImage:** `Envarsa_x.y.z_amd64.AppImage` on GitHub Releases, portable. Make it executable
  (`chmod +x`) and run it.
- To update either one, save the new file into the same folder; it picks up the same `Envarsa Data`, and the old
  file can be deleted.
- **Microsoft Store:** the MSIX from the release run, uploaded to Partner Center.
- **Flatpak:** `Envarsa_x.y.z_x86_64.flatpak` on GitHub Releases now, and on Flathub once it passes review.

A version tag runs `.github/workflows/release.yml`, which builds all four and publishes the exe, the AppImage
and the Flatpak bundle.

## Layout

`ui/` is the no-build frontend and `src-tauri/` the Rust core. [ARCHITECTURE.md](ARCHITECTURE.md) maps every
module, follows a request from the webview to disk, and names the rules the code keeps.

## License

MIT — see [LICENSE](LICENSE).

# Flatpak

`io.github.terminalis.Envarsa.yml` builds Envarsa as a Flatpak on the GNOME runtime. The app gets a Wayland window
(X11 as a fallback) and the GPU, and nothing else: no network, no filesystem access, no extra D-Bus names.
Files come and go through the desktop portals.

The build has no network either, so every crate in `src-tauri/Cargo.lock` is listed as a source in
`cargo-sources.json`. That file is generated, not committed:

```sh
tools/flatpak-cargo-sources.sh
```

Run it again whenever `Cargo.lock` changes. It needs `python3` with `pip`.

## Build and run

With Flatpak, the Flathub remote and Flathub's builder:

```sh
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub org.flatpak.Builder
```

From the repository root:

```sh
flatpak run --command=flathub-build org.flatpak.Builder --install flatpak/io.github.terminalis.Envarsa.yml
flatpak run io.github.terminalis.Envarsa
```

`flathub-build` builds the way Flathub does, into `builddir/` and the OSTree repo `repo/`, with its cache in
`.flatpak-builder/`. All three are gitignored and left out of the build's own source copy. To make a bundle
like the one on GitHub Releases:

```sh
flatpak build-bundle repo Envarsa.flatpak io.github.terminalis.Envarsa \
  --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo
```

## Lint

```sh
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest flatpak/io.github.terminalis.Envarsa.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
```

Both should pass. The one known difference is the source: this manifest builds the checkout it sits in
(`type: dir`), which the linter accepts everywhere except on Flathub's own servers, where it reports
`module-envarsa-source-dir-not-allowed`.

## Distribution

Each release attaches the bundle to its GitHub Release, then starts `publish.yml` in
[terminalis/flatpak](https://github.com/terminalis/flatpak). That workflow imports the bundle into the signed
repository that Flatpak installs update from (`https://terminalis.github.io/flatpak/repo`). The bundle on its
own installs without updates.

# Flatpak

`dev.envarsa.Envarsa.yml` builds Envarsa as a Flatpak on the GNOME runtime. The app gets a Wayland window
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
flatpak run --command=flathub-build org.flatpak.Builder --install flatpak/dev.envarsa.Envarsa.yml
flatpak run dev.envarsa.Envarsa
```

`flathub-build` builds the way Flathub does, into `builddir/` and the OSTree repo `repo/`, with its cache in
`.flatpak-builder/`. All three are gitignored and left out of the build's own source copy. To make a bundle
like the one on GitHub Releases:

```sh
flatpak build-bundle repo Envarsa.flatpak dev.envarsa.Envarsa \
  --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo
```

## Lint

```sh
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest flatpak/dev.envarsa.Envarsa.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
```

Both should pass. The one known difference is the source: this manifest builds the checkout it sits in
(`type: dir`), which the linter accepts everywhere except on Flathub's own servers, where it reports
`module-envarsa-source-dir-not-allowed`. That's why the Flathub copy uses the release tag instead.

## Flathub

The Flathub repository `flathub/dev.envarsa.Envarsa` holds a copy of the manifest and a `cargo-sources.json`.
For each release:

1. Copy `dev.envarsa.Envarsa.yml` from the release tag, and replace its `type: dir` source (and the comment
   above it) with the tag and the commit it points to:

   ```yaml
         - type: git
           url: https://github.com/terminalis/envarsa.git
           tag: v2.0.0
           commit: <full commit hash of v2.0.0>
   ```

2. Replace `cargo-sources.json` with the one from the release run's `cargo-sources` artifact, so it matches the
   tag's `Cargo.lock`.

Flathub's generative-AI policy requires the submission and these update pull requests, including their
descriptions, commit messages and review replies, to be written by the owner rather than an AI tool.

# GLib security backport

`glib-0.18.5/` is the complete published crates.io source archive, including
its upstream LICENSE and COPYRIGHT files. It is used by the application through
`[patch.crates-io]` in `../Cargo.toml` to preserve the GTK 0.18 dependency family.

- Source: https://static.crates.io/crates/glib/glib-0.18.5.crate
- Archive SHA-256: `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`
- Advisory: https://rustsec.org/advisories/RUSTSEC-2024-0429.html
- Upstream fix: https://github.com/gtk-rs/gtk-rs-core/pull/1343

The only changes inside the upstream archive are in `src/variant_iter.rs`:
make the pointer binding `p` mutable and pass `&mut p` to the variadic
`g_variant_get_child` call. This permits C to populate the out-pointer before
`CStr::from_ptr` reads it. The returned strings still borrow from the variant.

Application integration tests in `../tests/glib_variant_str_iter.rs` cover
forward/backward iteration, skipping, last, mixed iteration, Unicode, empty
strings, exhaustion, and empty arrays. From the repository root on Linux with
the application's normal Rust and GTK/WebKit development prerequisites:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --release --test glib_variant_str_iter
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

The first command uses the application's release optimization settings. The
Linux release workflow runs it before building the bundles. A passing release
test must be obtained before treating runtime verification as complete; the
backport was prepared on a host without Rust or GLib development tools.

Version-based scanners may continue to report 0.18.5. This is a documented
source backport, not an upstream fixed release; no advisory is suppressed.
When the entire GTK/Tauri stack supports an upstream-fixed GLib (the advisory
lists 0.20.0+), remove this patch and vendored directory, update the test
dependency and Cargo.lock, and rerun the optimized tests and Linux build.

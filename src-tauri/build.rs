fn main() {
    // tauri.conf.json takes the version from package.json, which
    // tauri-build doesn't watch; without this a bump keeps the old one.
    println!("cargo:rerun-if-changed=../package.json");
    tauri_build::build()
}

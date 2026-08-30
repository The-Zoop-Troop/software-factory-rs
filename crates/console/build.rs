//! Embed the built web console (`ui/dist`) when present; otherwise a placeholder page that
//! says how to build it. Either way the binary is self-contained.
fn main() {
    let dist = std::path::Path::new("ui/dist/index.html");
    let dir = if dist.exists() {
        "ui/dist"
    } else {
        "ui/placeholder"
    };
    println!("cargo:rustc-env=CONSOLE_UI_DIR={dir}");
    println!("cargo:rerun-if-changed=ui/dist");
    println!("cargo:rerun-if-changed=ui/placeholder");
}

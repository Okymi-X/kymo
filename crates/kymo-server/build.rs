use std::fs;

// rust-embed needs the folder to exist even before the web app has been built.
fn main() {
    let _ = fs::create_dir_all("../../web/dist");
    println!("cargo:rerun-if-changed=../../web/dist");
}

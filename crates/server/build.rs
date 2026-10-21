//! The web bundle is embedded from `web/dist`; make sure the folder exists so that
//! `cargo build` / `cargo test` work before the front-end has been built.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    let _ = std::fs::create_dir_all(&dist);
    println!("cargo:rerun-if-changed=../../web/dist");
}

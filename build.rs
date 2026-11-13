//! Build script: embeds every JPEG in `assets/images/` into the executable.
//!
//! It generates `$OUT_DIR/embedded_images.rs`, a lookup function built from
//! `include_bytes!`, so adding or replacing a photo never needs a code change:
//! drop the file in `assets/images/`, reference it from `assets/ships.json`,
//! and rebuild.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/images");
    println!("cargo:rerun-if-changed=assets/ships.json");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let images_dir = manifest_dir.join("assets").join("images");

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    match fs::read_dir(&images_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                    files.push((name.to_owned(), path.clone()));
                } else {
                    println!(
                        "cargo:warning=assets/images/{name} was skipped: only .jpg/.jpeg images are embedded"
                    );
                }
            }
        }
        Err(err) => {
            // The app still builds and shows its placeholder artwork.
            println!("cargo:warning=assets/images could not be read ({err}); no images embedded");
        }
    }
    files.sort();

    let mut code = String::from(
        "/// Returns the embedded bytes of `assets/images/<name>`, if that file existed at build time.\n\
         pub fn embedded_image(name: &str) -> Option<&'static [u8]> {\n    match name {\n",
    );
    for (name, path) in &files {
        // `{:?}` produces correctly escaped Rust string literals.
        let _ = writeln!(
            code,
            "        {:?} => Some(include_bytes!({:?})),",
            name,
            path.to_string_lossy()
        );
    }
    code.push_str("        _ => None,\n    }\n}\n");
    let _ = writeln!(
        code,
        "\n/// Number of images embedded at build time.\n#[allow(dead_code)]\npub const EMBEDDED_IMAGE_COUNT: usize = {};",
        files.len()
    );

    let out = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("embedded_images.rs");
    fs::write(&out, code).expect("failed to write embedded_images.rs");
}

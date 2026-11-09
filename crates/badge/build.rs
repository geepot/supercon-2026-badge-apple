// Embed the optional, git-ignored Bad Apple frames (scripts/bad-apple.py).
// Without them the app still builds and reports that no video is present.
fn main() {
    let source = std::path::Path::new("../../assets/video/bad-apple.bin");
    println!("cargo:rerun-if-changed={}", source.display());
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("bad-apple.bin");
    std::fs::write(out, std::fs::read(source).unwrap_or_default()).unwrap();
}

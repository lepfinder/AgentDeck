fn main() {
    let manifest_dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let generated = manifest_dir.join("src/api_docs.generated.md");
    if !generated.exists() {
        eprintln!(
            "warning: missing api_docs.generated.md — run `npm run generate:api-docs` from project root"
        );
    }
    tauri_build::build()
}

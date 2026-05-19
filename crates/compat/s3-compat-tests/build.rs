use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let specs_dir = Path::new(&manifest_dir).join("specs");
    if !specs_dir.exists() {
        return;
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("generated_tests.rs");

    let mut code = String::new();
    discover_and_generate(&specs_dir, &specs_dir, &mut code);
    fs::write(&dest, code).unwrap();

    println!("cargo:rerun-if-changed=specs");
}

fn discover_and_generate(root: &Path, dir: &Path, code: &mut String) {
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            discover_and_generate(root, &path, code);
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            let rel = path.strip_prefix(root).unwrap();
            let test_name = rel
                .with_extension("")
                .to_string_lossy()
                .replace(['/', '\\', '-', '.'], "_");
            let spec_path = path.display();

            code.push_str(&format!(
                r#"
#[test]
fn spec_{test_name}() {{
    crate::runtime().block_on(crate::run_spec_file("{spec_path}"));
}}
"#
            ));
        }
    }
}

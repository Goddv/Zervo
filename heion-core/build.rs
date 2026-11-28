use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=locales");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("crate directory"));
    let mut paths: Vec<_> = fs::read_dir(root.join("locales"))
        .expect("locales directory")
        .map(|entry| entry.expect("locale file").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ftl"))
        .collect();
    paths.sort_unstable();

    let mut catalogues = String::from("&[\n");
    for path in paths {
        let locale = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("UTF-8 locale");
        let path = path.to_str().expect("UTF-8 path");
        catalogues.push_str(&format!("    ({locale:?}, include_str!({path:?})),\n"));
    }
    catalogues.push_str("]\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"));
    fs::write(output.join("locales.rs"), catalogues).expect("write catalogue list");
}

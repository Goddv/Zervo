use std::fs;

fn version<'a>(lock: &'a str, name: &str) -> &'a str {
    let header = format!("name = \"{name}\"\nversion = \"");
    lock.split("[[package]]\n")
        .filter_map(|entry| entry.strip_prefix(&header))
        .find_map(|entry| entry.split_once('"').map(|(version, _)| version))
        .unwrap_or_else(|| panic!("missing {name} version in Cargo.lock"))
}

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lock = fs::read_to_string("Cargo.lock").expect("read Cargo.lock");
    for (name, variable) in [
        ("servo", "ZERVO_SERVO_VERSION"),
        ("egui", "ZERVO_EGUI_VERSION"),
        ("winit", "ZERVO_WINIT_VERSION"),
    ] {
        println!("cargo:rustc-env={variable}={}", version(&lock, name));
    }
}

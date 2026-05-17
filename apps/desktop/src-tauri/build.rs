fn main() {
    println!("cargo:rerun-if-changed=src/ipc/mod.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/auth/mod.rs");
    println!("cargo:rerun-if-changed=src/db/mod.rs");
    println!("cargo:rerun-if-changed=src/sync/mod.rs");
    println!("cargo:rerun-if-changed=src/render/mod.rs");
    println!("cargo:rerun-if-changed=src/storage/mod.rs");

    tauri_build::build();
}

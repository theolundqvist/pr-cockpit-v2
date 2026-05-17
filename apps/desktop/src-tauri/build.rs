fn main() {
    println!("cargo:rerun-if-changed=src/ipc/mod.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/auth/mod.rs");
    println!("cargo:rerun-if-changed=src/db/mod.rs");
    println!("cargo:rerun-if-changed=src/sync/mod.rs");
    println!("cargo:rerun-if-changed=src/render/mod.rs");
    println!("cargo:rerun-if-changed=src/storage/mod.rs");

    if std::env::var_os("SKIP_IPC_BINDINGS_GEN").is_none() {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set");
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR must be set");
        let nested_target_dir = std::path::Path::new(&out_dir).join("ipc-bindings-target");
        let status = std::process::Command::new("cargo")
            .current_dir(manifest_dir)
            .arg("run")
            .arg("--quiet")
            .arg("--bin")
            .arg("generate-ipc-bindings")
            .arg("--target-dir")
            .arg(&nested_target_dir)
            .env("SKIP_IPC_BINDINGS_GEN", "1")
            .status()
            .expect("failed to regenerate IPC bindings");
        assert!(
            status.success(),
            "IPC bindings regeneration failed during build script"
        );
    }

    tauri_build::build();
}

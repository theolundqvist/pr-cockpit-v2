use anyhow::{Context, Result};

#[test]
fn ipc_bindings_are_up_to_date() -> Result<()> {
    let output = desktop_lib::ipc::bindings_output_path();
    let before = std::fs::read_to_string(&output).unwrap_or_default();

    desktop_lib::ipc::export_bindings(&output)?;

    let after = std::fs::read_to_string(&output)
        .with_context(|| format!("reading generated bindings at {}", output.display()))?;
    assert_eq!(
        before,
        after,
        "IPC bindings changed; run `cargo run -p desktop --bin generate-ipc-bindings` and commit the updated file."
    );
    Ok(())
}

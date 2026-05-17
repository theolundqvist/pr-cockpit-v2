use anyhow::Result;

fn main() -> Result<()> {
    desktop_lib::ipc::export_default_bindings()
}

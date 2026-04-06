use anyhow::Result;

fn main() -> Result<()> {
    if let Some(bundle) = rts::runtime::bundle::read_embedded_entry_from_current_exe()? {
        let _ = rts::runtime::runner::run_embedded_program(&bundle.payload)?;
        return Ok(());
    }

    rts::cli::dispatch(std::env::args())
}

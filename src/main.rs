use anyhow::Result;

fn main() -> Result<()> {
    rts::cli::dispatch(std::env::args())
}

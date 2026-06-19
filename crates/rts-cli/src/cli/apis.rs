//! `rts apis` — list ABI namespaces. STUBBED at the P5 cutover: the SPECS catalog
//! lived in the old engine; the new engine's registry export is pending.

use anyhow::{Result, anyhow};

pub fn command() -> Result<()> {
    Err(anyhow!(
        "`rts apis` is not yet available on the new engine (cutover in progress)"
    ))
}

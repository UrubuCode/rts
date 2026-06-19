//! `rts emit-types` — generate `rts.d.ts` from the ABI SPECS. STUBBED at the P5
//! cutover (SPECS catalog lived in the old engine).

use anyhow::{Result, anyhow};

pub fn command(_output: Option<String>) -> Result<()> {
    Err(anyhow!(
        "`rts emit-types` is not yet available on the new engine (cutover in progress)"
    ))
}

// The other half of the cycle — see `_claude_esm_cycle_defaults.ts`.
//
// The import is at the top, where the exporter has published nothing; the USE is
// inside a function called long afterwards. Node answers `32:64` here, which is
// the proof the binding is live: a snapshot taken at this line could only be
// `undefined:undefined`.

import { KEY_BUNDLE_TYPE, KEY_LEN } from "./_claude_esm_cycle_defaults";
import * as D from "./_claude_esm_cycle_defaults";

// Read AT THIS LINE, while the module it names has published nothing at all.
// Node answers "object": a namespace exists from the moment the module is
// entered, which is what a live import reads through. `undefined` here would be
// the namespace not existing yet, and every live read of this cycle would be a
// read off nothing.
export const namespaceSoFar: string = typeof D;

export function makeCreds(): string {
    return KEY_BUNDLE_TYPE + ":" + KEY_LEN;
}

export function viaNamespace(): number {
    return D.KEY_LEN;
}

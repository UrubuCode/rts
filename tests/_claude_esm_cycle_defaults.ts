// Helper for tests/claude-esm-live-binding.test.ts — the cycle, reduced from
// the one `@whiskeysockets/baileys` is built out of:
//
//     Defaults -> Signal/libsignal -> Utils/index -> Utils/crypto -> Defaults
//
// This module is entered first and its very first line names the other half, so
// `_claude_esm_cycle_crypto` runs while this one has published nothing. Its top
// level therefore reads `KEY_BUNDLE_TYPE` out of a namespace that does not hold
// it yet — which is correct only if the read happens where the VALUE is used.

import { makeCreds, namespaceSoFar, viaNamespace } from "./_claude_esm_cycle_crypto";

export const KEY_BUNDLE_TYPE = 32;
export const KEY_LEN = 64;

export function creds(): string {
    return makeCreds();
}

export function probeNamespace(): string {
    return namespaceSoFar + ":" + viaNamespace();
}

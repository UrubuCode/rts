// `export { … } from` over the live module, so the test can ask whether a name
// that was never bound here forwards the assignment too.

export { n, bump, K } from "./_claude_esm_live_a";

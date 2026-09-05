// What `rts compile` exists to make true, by DEFAULT now (the flag in this
// file's own name predates that flip and stays valid as an accepted
// synonym): a page's own `<script>` can call `eval` inside a compiled
// `.exe`, exactly as it already does under `rts run`
// (`rts-dom-bridge::DomScope::run` -> `evaluate_in_scope_with_receiver`).
// `--sem-compilador`/`--no-compiler` opts OUT, to the small archive
// (`rts-runtime`), which installs no evaluator — `rts-host/README.md`
// states the refusal by name — so this same program, compiled WITH that
// flag, prints the empty string `eval` never got the chance to fill in.
//
// No window: this is the DOM headless, exactly as `dom.parseHtml` always is
// without a `rts:egui` surface open, so the claim is about the compiler and
// nothing else.
//
// The fixture stringifies `eval`'s answer (`"" + eval(...)`) rather than
// assigning it bare. That is not about `eval`: `textContent = 3` (a plain
// numeric LITERAL, no `eval` anywhere) answers the same empty string under
// `rts run` today — a `textContent` setter that does not `ToString` a
// non-string value, which is a `rts-dom`/`rts-dom-bridge` WebIDL-coercion
// defect this lot found and is NOT the one to fix here: it is orthogonal to
// whether a compiler is embedded, and reproduces with no `--embed-compiler`
// and no AOT involved at all.
import { readFileSync } from "node:fs";
import dom from "rts:dom";

const html = readFileSync("tests/aot/claude-pagina-eval.html", "utf8") as string;
const d: i64 = dom.parseHtml(html);
runScriptsAt(new Document(d), "https://localhost/");
console.log(dom.getText(d, dom.querySelector(d, "#x")));

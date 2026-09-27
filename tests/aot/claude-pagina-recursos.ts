// AOT-1's ruler: every resource a compiled page references travels inside the
// binary. The page links a sheet that `@import`s a second one, a
// `<script src>` and a local `<img src>` — all four under
// `claude-pagina-recursos/`. This driver loads the page HEADLESS through
// `loadDocumentFrom` and prints one value that only each resource can
// produce: the width the linked sheet sets, the height only the `@import`ed
// sheet sets, the text only `probe.js` writes, and the image's natural size.
//
// The claim is the equality of two runs of the SAME `.exe`, compiled with
// `--html tests/aot/claude-pagina-recursos.html`: one with the folder beside
// the page, one from another working directory with the folder renamed away.
// Before AOT-1 the second run lost all four.
//
// The resource base is `__dirname` + the page's own name, not a literal: the
// build records each resource under the exact string the loader resolves from
// the page's base, and `__dirname` is baked at build time with the same
// spelling the recorder's base has (`rts_host::object::page_resources::
// resource_base` — canonical, Windows verbatim prefix stripped). The HTML is
// read from that same absolute path, so a run from another directory still
// finds it — only the resource FOLDER is taken away.
//
// `__dirname` is bound only for a program compiled as a module GRAPH; one
// compiled alone answers "" for it. A file that mentions `import.meta` is
// always compiled as a graph, by `rts run` and `rts compile` alike
// (`rts-cli`'s `imports_a_file`) — the `void` below is that mention, so the
// two destinations agree about `__dirname` for a reason written down.
import { readFileSync } from "node:fs";
import dom from "rts:dom";

void import.meta;
const page: string = __dirname + "/claude-pagina-recursos.html";
const html = readFileSync(page, "utf8") as string;
const doc = loadDocumentFrom(html, "https://localhost/", page);
const d: i64 = doc._dom;
const alvo = doc.getElementById("alvo");
const probe = doc.getElementById("probe");
const img = dom.querySelector(d, "#img");
console.log("width: " + (alvo === null ? "(none)" : alvo.getComputedProp("width")));
console.log("height: " + (alvo === null ? "(none)" : alvo.getComputedProp("height")));
console.log("probe: " + (probe === null ? "(none)" : probe.textContent));
console.log("image: " + dom.imageNaturalWidth(d, img) + "x" + dom.imageNaturalHeight(d, img));

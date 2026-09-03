// What `rts compile` has to answer identically to `rts run`, and did not:
// this program is three files, and an AOT build refused it outright until the
// object path learned to compile a module graph.
//
// Every line here is one of the things that was broken or absent:
//   - the imports themselves         — refused, "does not compile a module graph"
//   - `await` inside a method        — "async function has no registered frame"
//   - a generator in another module  — the same
//   - `.length` / `.name`            — silently `undefined`
//   - `import.meta.url`              — a graph has one; a lone file has none
//   - `require("./util")`            — asks at run time, and an AOT binary has
//                                      no disk to ask; the answers ride the
//                                      manifest instead
//
// The CI smoke runs it BOTH ways and compares, which is the claim rule 4 of
// `crates/rts-host/README.md` makes: one program, two destinations.
import { Service } from "./service";
import { LABEL, upto, twice } from "./util";

async function main() {
  const service = new Service();
  let running = 0;
  for (const n of upto(3)) running = await service.add(n);

  console.log(LABEL + " total=" + running);
  console.log("arity=" + twice.length + " name=" + twice.name);
  console.log("meta=" + (typeof import.meta.url === "string"));

  // The same file again, through the other module system, in the same file.
  // Not a second module: `graph.rs` resolves this specifier while loading, so
  // it is the one compilation the ESM import above is in.
  const alsoUtil = require("./util");
  console.log("require=" + alsoUtil.twice(5));

  try {
    (null as unknown as { x: { y: number } }).x.y;
  } catch (error) {
    console.log("caught=" + (error as Error).message);
  }
}

main();

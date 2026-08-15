// Generates obfuscated cross-runtime fixtures from the seeds.
//
// A fixture is only kept when Bun and Node produce the SAME output as each
// other AND the same output as the un-obfuscated seed. The second check is the
// one that matters: an obfuscator that changed the program's meaning would
// otherwise hand us a fixture pinning its bug rather than the language.
import { readFileSync, writeFileSync, readdirSync, mkdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import JsObfuscator from "javascript-obfuscator";

const here = import.meta.dirname;
const seeds = join(here, "seeds");
const out = join(here, "out");
mkdirSync(out, { recursive: true });

// Options that are non-deterministic, environment-sniffing, or that defeat any
// instrumentation are OFF: `selfDefending` and `debugProtection` hang under a
// harness, `domainLock` reads a location that does not exist here, and the
// `rgf` transform builds `new Function` — which this corpus's README excludes
// by policy.
const profiles = {
  strings: {
    compact: false,
    stringArray: true,
    stringArrayThreshold: 1,
    stringArrayEncoding: ["base64"],
    stringArrayWrappersCount: 2,
    stringArrayWrappersType: "function",
    splitStrings: true,
    splitStringsChunkLength: 3,
    identifierNamesGenerator: "hexadecimal",
  },
  flatten: {
    compact: false,
    controlFlowFlattening: true,
    controlFlowFlatteningThreshold: 1,
    identifierNamesGenerator: "mangled",
    simplify: true,
  },
  deadcode: {
    compact: false,
    deadCodeInjection: true,
    deadCodeInjectionThreshold: 1,
    stringArray: true,
    stringArrayThreshold: 1,
    identifierNamesGenerator: "hexadecimal",
  },
  numbers: {
    compact: false,
    numbersToExpressions: true,
    simplify: true,
    transformObjectKeys: true,
    identifierNamesGenerator: "mangled",
  },
  everything: {
    compact: true,
    controlFlowFlattening: true,
    controlFlowFlatteningThreshold: 0.75,
    deadCodeInjection: true,
    deadCodeInjectionThreshold: 0.4,
    numbersToExpressions: true,
    simplify: true,
    splitStrings: true,
    splitStringsChunkLength: 5,
    stringArray: true,
    stringArrayThreshold: 1,
    stringArrayEncoding: ["rc4"],
    transformObjectKeys: true,
    identifierNamesGenerator: "hexadecimal",
  },
};

function runWith(binary, file) {
  try {
    return {
      ok: true,
      text: execFileSync(binary, [file], {
        encoding: "utf8",
        timeout: 20000,
        stdio: ["ignore", "pipe", "pipe"],
      }),
    };
  } catch (error) {
    return { ok: false, text: String(error.stdout ?? "") + String(error.stderr ?? "") };
  }
}

// The corpus rejects a fixture naming any of these — see the harness's
// `RTS_ONLY_PATTERNS`. An obfuscator can emit `global`/`process` sniffing even
// with self-defence off, so it is checked rather than assumed.
const banned = /(^|[^A-Za-z_])(JSON5|Bun|Deno|process|require)([^A-Za-z_]|$)/;

const report = [];
for (const seed of readdirSync(seeds).filter((n) => n.endsWith(".js"))) {
  const source = readFileSync(join(seeds, seed), "utf8");
  const base = seed.replace(/\.js$/, "");
  const expected = runWith("node", join(seeds, seed));
  if (!expected.ok) {
    report.push(`SEED FALHA ${seed}: ${expected.text.slice(0, 120)}`);
    continue;
  }
  const alsoBun = runWith("bun", join(seeds, seed));
  if (!alsoBun.ok || alsoBun.text !== expected.text) {
    report.push(`SEED bun!=node ${seed}`);
    continue;
  }

  for (const [name, options] of Object.entries(profiles)) {
    let code;
    try {
      code = JsObfuscator.obfuscate(source, options).getObfuscatedCode();
    } catch (error) {
      report.push(`OBF FALHA ${base}/${name}: ${error.message.slice(0, 80)}`);
      continue;
    }
    const file = join(out, `${base}__${name}.ts`);
    writeFileSync(file, code);
    if (banned.test(code)) {
      report.push(`BANIDO ${base}/${name}: emite um nome que o medidor rejeita`);
      continue;
    }
    const node = runWith("node", file);
    const bun = runWith("bun", file);
    const agree = node.ok && bun.ok && node.text === bun.text;
    const faithful = agree && node.text === expected.text;
    // An output that is not FAITHFUL is deleted rather than reported and left
    // behind: it would otherwise be installed by `install.mjs`, which reads the
    // directory rather than this report, and a fixture pinning the obfuscator's
    // own bug is the one thing this check exists to keep out. Measured:
    // `transformObjectKeys` rewrites `{ get v() {} }` into a form that loses
    // the accessor, so two outputs ran and answered something the seed did not.
    if (!faithful) {
      rmSync(file, { force: true });
    }
    report.push(
      `${faithful ? "OK      " : agree ? "INFIEL  " : "DISCORDA"} ${base}/${name}`,
    );
  }
}
console.log(report.join("\n"));

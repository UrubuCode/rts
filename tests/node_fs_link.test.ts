// node:fs — chmodSync + linkSync.
import { describe, test, expect } from "rts:test";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, statSync, chmodSync, linkSync, rmSync } from "node:fs";

const d = mkdtempSync("__rts_link_");
const f = d + "/orig.txt";
writeFileSync(f, "shared");

// hard link: both paths, same content.
const ln = d + "/hardlink.txt";
linkSync(f, ln);
const linkOk = existsSync(ln) && readFileSync(ln, "utf8") === "shared";

// chmod: set read-only-ish; mode reflects the owner-read bits.
chmodSync(f, 0o644);
const mode = statSync(f).mode;
// low 9 bits are the permission bits; owner-read (0o400) must be set.
const chmodOk = (mode & 0o400) !== 0;

const rmOpts = { recursive: true };
rmSync(d, rmOpts);
const cleaned = existsSync(d) === false;

describe("node:fs chmod/link", () => {
    test("linkSync hard link", () => expect(linkOk).toBe(true));
    test("chmodSync sets mode", () => expect(chmodOk).toBe(true));
    test("cleanup", () => expect(cleaned).toBe(true));
});

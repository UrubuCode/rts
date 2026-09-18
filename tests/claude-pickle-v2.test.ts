import { describe, test, expect } from "rts:test";
import { serialize, deserialize, version, upgrade } from "rts:serde";

// What RTSP v2 adds to the v1 pickle (docs/engine/pickle.md): bytes as a
// Uint8Array, schema versions a class migrates itself, and names that say
// which module a class came from. Pre-computed at top level, as the other
// pickle tests are.

// ── 1. bytes are bytes ─────────────────────────────────────────────────────
const out: any = serialize({ a: [1, 2, 3], s: "texto" });
const isU8 = out instanceof Uint8Array;
const fromU8: any = deserialize(out);
const fromArrayBuffer: any = deserialize(out.buffer);
const fromBuffer: any = deserialize(Buffer.from(out));
const fromNumbers: any = deserialize(Array.from(out));

// ── 2. schema versions ─────────────────────────────────────────────────────
// A class at version 1, written; then the same class at version 2 reading it
// back. The two are simulated in one program by writing under `static
// [version] = 1` and bumping the static before reading — the class is the
// same declaration, which is what the name lookup finds.
class Save {
  static [version] = 1;
  coins: number;
  #owner: string;
  constructor(coins: number, owner: string) {
    this.coins = coins;
    this.#owner = owner;
  }
  get owner(): string {
    return this.#owner;
  }
  static [upgrade](fields: any, from: number): any {
    upgrades.push(from);
    if (from < 2) {
      fields.gold = fields.coins * 10;
      delete fields.coins;
      fields["#owner"] = fields["#owner"].toUpperCase();
    }
    return fields;
  }
}
const upgrades: number[] = [];
const written = serialize({ save: new Save(3, "ana") });
(Save as any)[version] = 2;
const migrated: any = deserialize(written);
// Same version on both sides: `upgrade` is NOT called.
const sameVersion = serialize(new Save(1, "bia"));
const notMigrated: any = deserialize(sameVersion);

// A class with no version at all keeps v1's rule: fields matched by key.
class Plain {
  x = 1;
}
const plainBack: any = deserialize(serialize(new Plain()));

// ── 3. qualified names are in the stream ───────────────────────────────────
// The entry module's key is "" and the class name follows it; a reader that
// finds that pair picks this class without looking at any other `Plain`.
const plainBytes = Array.from(serialize(new Plain()));
const text = String.fromCharCode(...plainBytes.filter((b) => b >= 32 && b < 127));

describe("rts:serde v2", () => {
  test("serialize answers a Uint8Array, and every byte source reads back", () => {
    expect(isU8).toBe(true);
    expect(fromU8.s).toBe("texto");
    expect(fromArrayBuffer.a[2]).toBe(3);
    expect(fromBuffer.s).toBe("texto");
    expect(fromNumbers.a.length).toBe(3);
  });

  test("an older version is migrated by the class's upgrade, before revival", () => {
    expect(migrated.save instanceof Save).toBe(true);
    expect(migrated.save.gold).toBe(30);
    expect(migrated.save.coins).toBe(undefined);
    expect(migrated.save.owner).toBe("ANA");
    expect(upgrades.length).toBe(1);
    expect(upgrades[0]).toBe(1);
  });

  test("the same version is not migrated", () => {
    expect(notMigrated.coins).toBe(1);
    expect(notMigrated.owner).toBe("bia");
    expect(upgrades.length).toBe(1);
  });

  test("a class without a version keeps fields by key", () => {
    expect(plainBack instanceof Plain).toBe(true);
    expect(plainBack.x).toBe(1);
  });

  test("the class name is in the stream", () => {
    expect(text.includes("Plain")).toBe(true);
  });
});

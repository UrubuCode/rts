// node:sqlite — DatabaseSync/StatementSync over turso_core. Nothing in this
// repository's own fixtures had ever constructed a DatabaseSync before this
// file (per `docs/reference/node/node_completed.md`'s own note that the one
// file matching "node:sqlite" only checks `isBuiltin()` list membership).
//
// `node:sqlite` is not available in the real Node installed on this machine
// (v20.19.5 — the module shipped experimentally starting Node 22.5), so the
// shapes asserted below that Node itself defines (`{ changes,
// lastInsertRowid }` from `run()`, a plain object per row from `get()`/
// `all()`, column metadata from `columns()`) come from Node's own
// documentation for `node:sqlite`/`better-sqlite3`'s compatible shape, not
// from running `node -e` here.
//
// The crate's OWN doc (`crates/rts-node/src/sqlite/mod.rs` and
// `database.rs`/`value.rs`) states, up front and repeatedly, that no native
// here can raise a catchable JS exception — `entry::throw` ends the whole
// program, it is not `throw`. So `DatabaseSync`'s constructor never throws
// (a bad path answers `isOpen === false` instead), and `exec`/`prepare`
// swallow a SQL error into `undefined` instead of throwing `SQLiteError`.
// That is a declared, crate-wide architectural limit — not a bug this file
// re-discovers — so the tests below assert the DOCUMENTED (silent) behaviour
// for those specific cases, with a comment saying so, the same way this
// crate's `node:vm` fixture is asked to treat ITS one documented limit.
import { describe, test, expect } from "rts:test";
import { DatabaseSync, constants } from "node:sqlite";

// ── in-memory: open, schema, insert, read back ──────────────────────────────
const db = new DatabaseSync(":memory:");
const dbIsOpenAfterCtor = db.isOpen;
const dbIsTransactionAfterCtor = db.isTransaction;

db.exec(
    "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, val REAL, note TEXT, blob BLOB)"
);

const insertStmt = db.prepare("INSERT INTO t (name, val, note) VALUES (?, ?, ?)");
const run1 = insertStmt.run("hello", 3.14, null);
const run2 = insertStmt.run("world", 2.71, "second");

// ── get() / all() ────────────────────────────────────────────────────────────
const getStmt = db.prepare("SELECT * FROM t WHERE id = ?");
const row1 = getStmt.get(1);
const row2 = getStmt.get(2);
const rowMissing = getStmt.get(999);

const allStmt = db.prepare("SELECT * FROM t ORDER BY id");
const allRows = allStmt.all();

// ── NULL round-trips ─────────────────────────────────────────────────────────
const noteOfRow1IsNull = row1.note === null;

// ── BLOB round-trip ───────────────────────────────────────────────────────────
const blobStmt = db.prepare("INSERT INTO t (name, blob) VALUES (?, ?)");
const bytes = new Uint8Array([0, 1, 2, 254, 255]);
blobStmt.run("hasblob", bytes);
const blobRow = db.prepare("SELECT blob FROM t WHERE name = ?").get("hasblob");
const blobBuf: any = blobRow.blob;
const blobIsBufferLike = blobBuf != null && typeof blobBuf.length === "number";
const blobBytesOk =
    blobIsBufferLike &&
    blobBuf.length === 5 &&
    blobBuf[0] === 0 &&
    blobBuf[3] === 254 &&
    blobBuf[4] === 255;

// ── columns() ─────────────────────────────────────────────────────────────────
const columnsDescribed = db.prepare("SELECT id, name FROM t").columns();

// ── iterate() is NOT implemented — the module doc names it by name ──────────
const iterateIsFunction = typeof (allStmt as any).iterate === "function";

// ── constants object ─────────────────────────────────────────────────────────
const constantsHasReadonly = typeof constants.SQLITE_OPEN_READONLY === "number";

// ── db.close() ────────────────────────────────────────────────────────────────
db.close();
const dbIsOpenAfterClose = db.isOpen;

// ── DIVERGENCE (documented): a syntax error does not throw — it answers
// `undefined` silently, because no native here can raise a catchable
// exception (crate-wide limit, not sqlite-specific). Real Node throws a
// `SQLiteError` from BOTH `exec()` on bad SQL and `prepare()` on bad SQL.
const dbForErrors = new DatabaseSync(":memory:");
let execThrew = false;
try {
    dbForErrors.exec("THIS IS NOT VALID SQL AT ALL");
} catch (e) {
    execThrew = true;
}
const execBadSqlResult = dbForErrors.exec("THIS IS NOT VALID SQL AT ALL");

let prepareThrew = false;
let preparedBad: any = undefined;
try {
    preparedBad = dbForErrors.prepare("SELEC * FROM nowhere");
} catch (e) {
    prepareThrew = true;
}
// Since prepare() on bad SQL answers `undefined` here (rather than throwing
// per Node), the following would be `undefined.run is not a function` in
// real Node's own catch too (Node never gets a StatementSync back from a
// failed prepare) — but the FAILURE MODE differs: Node fails at `prepare()`
// with a descriptive SQLiteError; this engine fails at `.run()` with a bare
// TypeError on `undefined`, several lines away from the actual mistake.
let runOnUndefinedThrew = false;
try {
    (preparedBad as any).run();
} catch (e) {
    runOnUndefinedThrew = true;
}

// ── DIVERGENCE (documented): construction never throws — a bad/unreachable
// path answers an instance with `isOpen === false` rather than refusing to
// construct. Real Node throws synchronously from `new DatabaseSync(path)`
// for a path whose directory does not exist.
const badPathDb = new DatabaseSync("Z:\\this\\path\\does\\not\\exist\\anywhere\\claude.db");
const badPathIsOpen = badPathDb.isOpen;

// ── file-backed database, NOT under /tmp (Windows resolves that to
// C:\tmp\x, which this repository's own history names as a false-negative
// trap) — under the real OS temp dir instead. ───────────────────────────────
const tmpDir = process.env.TEMP || process.env.TMP || ".";
const filePath = tmpDir + "/claude-sqlite-file-test.db";
try {
    require("node:fs").unlinkSync(filePath);
} catch (e) {}

const fileDb = new DatabaseSync(filePath);
const fileDbIsOpen = fileDb.isOpen;
fileDb.exec("CREATE TABLE p (id INTEGER PRIMARY KEY, name TEXT)");
fileDb.prepare("INSERT INTO p (name) VALUES (?)").run("persisted-row");
const fileLocation = fileDb.location();
fileDb.close();

// Reopen — data should have actually reached disk.
const fileDb2 = new DatabaseSync(filePath);
const reopenedRow = fileDb2.prepare("SELECT * FROM p").get();
fileDb2.close();
try {
    require("node:fs").unlinkSync(filePath);
} catch (e) {}

// ── { open: false } ───────────────────────────────────────────────────────────
const notOpenedDb = new DatabaseSync(":memory:", { open: false });
const notOpenedIsOpen = notOpenedDb.isOpen;

// ── location() for :memory: is null ──────────────────────────────────────────
const memDb = new DatabaseSync(":memory:");
const memLocation = memDb.location();

describe("node:sqlite — construction", () => {
    test("new DatabaseSync(':memory:') opens", () => expect(dbIsOpenAfterCtor).toBe(true));
    test("isTransaction starts false", () => expect(dbIsTransactionAfterCtor).toBe(false));
    test("{ open: false } does not open", () => expect(notOpenedIsOpen).toBe(false));
    test("location() is null for :memory:", () => expect(memLocation).toBe(null));
});

describe("node:sqlite — run()", () => {
    test("run() returns changes:1 for first insert", () => expect(run1.changes).toBe(1));
    test("run() returns lastInsertRowid:1 for first insert", () => expect(run1.lastInsertRowid).toBe(1));
    test("run() returns lastInsertRowid:2 for second insert", () => expect(run2.lastInsertRowid).toBe(2));
});

describe("node:sqlite — get()/all()", () => {
    test("get(1) returns the right row's name", () => expect(row1.name).toBe("hello"));
    test("get(1) returns the right row's REAL value", () => expect(row1.val).toBe(3.14));
    test("get(2) returns the right row's name", () => expect(row2.name).toBe("world"));
    test("get() past the end is undefined", () => expect(rowMissing).toBe(undefined));
    test("all() returns every row", () => expect(allRows.length).toBe(2));
    test("all()[0] matches get(1)", () => expect(allRows[0].name).toBe("hello"));
    test("NULL round-trips as null, not undefined or ''", () => expect(noteOfRow1IsNull).toBe(true));
});

describe("node:sqlite — BLOB", () => {
    test("a Uint8Array binds and reads back as buffer-like", () => expect(blobIsBufferLike).toBe(true));
    test("BLOB bytes round-trip exactly", () => expect(blobBytesOk).toBe(true));
});

describe("node:sqlite — columns()", () => {
    test("columns() names the first column 'id'", () => expect(columnsDescribed[0].name).toBe("id"));
    test("columns() names the second column 'name'", () => expect(columnsDescribed[1].name).toBe("name"));
    test("columns()'s database field is always null (documented)", () => expect(columnsDescribed[0].database).toBe(null));
});

describe("node:sqlite — not implemented", () => {
    test("stmt.iterate is not a function (refused by name)", () => expect(iterateIsFunction).toBe(false));
});

describe("node:sqlite — constants", () => {
    test("constants.SQLITE_OPEN_READONLY is a number", () => expect(constantsHasReadonly).toBe(true));
});

describe("node:sqlite — close()", () => {
    test("isOpen is false after close()", () => expect(dbIsOpenAfterClose).toBe(false));
});

// These two are RED on purpose: they assert what real Node actually does
// (throws), against this engine's documented, crate-wide "no native can
// raise a catchable exception" limit. See the comment block above.
describe("node:sqlite — error handling (Node throws; this engine cannot)", () => {
    test("exec() with invalid SQL throws in real Node", () => expect(execThrew).toBe(true));
    test("prepare() with invalid SQL throws in real Node", () => expect(prepareThrew).toBe(true));
});

describe("node:sqlite — file-backed database", () => {
    test("opens a real file path", () => expect(fileDbIsOpen).toBe(true));
    test("location() answers the absolute path for a file db", () => expect(typeof fileLocation).toBe("string"));
    test("data persists across close()+reopen", () => expect(reopenedRow.name).toBe("persisted-row"));
});

describe("node:sqlite — construction never throws (documented divergence)", () => {
    test("an unreachable path still answers an instance", () => expect(typeof badPathDb).toBe("object"));
    test("...with isOpen === false", () => expect(badPathIsOpen).toBe(false));
});

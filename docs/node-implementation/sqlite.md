# node:sqlite

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:sqlite` |
| Node.js version | 25.x |
| Stability | 1.2 - Release candidate (was fully Experimental behind `--experimental-sqlite` v22.5.0–v23.3.0; unflagged-but-still-experimental v23.4.0–v25.6.0; promoted to Release Candidate as of v25.7.0) |
| Tier | P2 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import sqlite from 'node:sqlite'`; `import { DatabaseSync, StatementSync, Session, constants } from 'node:sqlite'`; CJS `require('node:sqlite')`. **No legacy bare specifier** — this module only ever existed under the `node:` prefix, `require('sqlite')` is not a thing. |
| Globals exposed | None — `node:sqlite` adds nothing to `globalThis`; every export must be imported explicitly. |

## 1. Purpose

`node:sqlite` is Node's built-in binding to the SQLite embedded relational
database engine — no external dependency, no native addon compilation step
for the end user, the SQLite C library ships vendored inside the Node binary
itself. It exposes a fully **synchronous** API (`DatabaseSync`/`StatementSync`)
mirroring the ergonomics of popular userland packages (`better-sqlite3`), plus
the SQLite **session/changeset extension** (`Session`) for capturing and
replaying sets of row-level changes (used for sync/replication scenarios),
a convenience LRU-cached tagged-template query helper (`SQLTagStore`), and a
`backup()` function for online hot backups. There is no callback- or
Promise-based query API — the only asynchronous surface in the entire module
is `sqlite.backup()`.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class DatabaseSync`

Not a subclass of `EventEmitter` or any stream base — a plain class. Holds
one open (or not-yet-opened) SQLite connection.

**Constructor**

```typescript
new DatabaseSync(path: string | Buffer | URL, options?: DatabaseSyncOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `path` | `string \| Buffer \| URL` | no | — (`':memory:'` for a private, temporary, in-memory database) |
| `options` | `DatabaseSyncOptions` | yes | see §3 for full field list/defaults |

Throws: a `TypeError`/`ERR_INVALID_ARG_TYPE`-class error for a malformed
`path`/`options` shape; an `ERR_SQLITE_ERROR` (wrapping the underlying
`sqlite3_open_v2()` failure, e.g. unwritable directory, corrupt file, invalid
URI) if `open` is not explicitly `false` and the open fails immediately.
Variant: **sync**. `path` accepting `Buffer`/`URL` added v23.10.0/v22.15.0;
the `timeout` option added v24.0.0/v22.16.0; several new options
(`enableDoubleQuotedStringLiterals`, `allowExtension`, `readBigInts`,
`returnArrays`, `allowBareNamedParameters`, `allowUnknownNamedParameters`)
added v24.4.0/v22.18.0; `defensive` option added v25.1.0/v24.12.0, defaulted
to `true` since v25.5.0/v24.14.0.

**Instance methods** (15 total, plus `[Symbol.dispose]`):

| Method | Signature |
|---|---|
| `aggregate` | `aggregate(name: string, options: AggregateOptions): void` |
| `close` | `close(): void` |
| `createSession` | `createSession(options?: CreateSessionOptions): Session` |
| `createTagStore` | `createTagStore(maxSize?: number): SQLTagStore` |
| `applyChangeset` | `applyChangeset(changeset: Uint8Array, options?: ApplyChangesetOptions): boolean` |
| `enableDefensive` | `enableDefensive(active: boolean): void` |
| `enableLoadExtension` | `enableLoadExtension(allow: boolean): void` |
| `exec` | `exec(sql: string): void` |
| `function` | `function(name: string, fn: SqlFunction): void` (overload A) |
| `function` | `function(name: string, options: FunctionOptions, fn: SqlFunction): void` (overload B) |
| `loadExtension` | `loadExtension(path: string): void` |
| `location` | `location(dbName?: string): string \| null` |
| `open` | `open(): void` |
| `prepare` | `prepare(sql: string, options?: PrepareOptions): StatementSync` |
| `setAuthorizer` | `setAuthorizer(callback: AuthorizerCallback \| null): void` |
| `[Symbol.dispose]` | `[Symbol.dispose](): void` |

**`aggregate(name, options)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `name` | `string` | no | — |
| `options` | `AggregateOptions` | no | — |

Returns: `void`. Throws: `ERR_INVALID_ARG_TYPE` if `step`/`result` are not
functions; `ERR_SQLITE_ERROR` if registration with SQLite itself fails (e.g.
name collides with a built-in that cannot be shadowed). Variant: **sync**
(the `step`/`result`/`inverse` callbacks themselves are invoked
**synchronously, re-entrantly**, from inside a later `exec`/statement `step`
call — not at registration time). Supports window-function semantics when
`inverse` is supplied (`result` may then be called multiple times per query,
once per window frame).

**`close()`** — no params, returns `void`. Wraps `sqlite3_close_v2()`
(deferred close — safe even with outstanding un-finalized statement handles
still reachable from JS, matching Node's own semantics). Throws:
`ERR_SQLITE_ERROR` only in pathological cases (corrupted connection state).
Variant: **sync**.

**`createSession(options?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `CreateSessionOptions` | yes | `{ db: 'main' }` |

Returns: a new `Session`. Throws: `ERR_SQLITE_ERROR` if the named `db` schema
does not exist, or if the SQLite session extension fails to attach. Variant:
**sync**.

**`createTagStore(maxSize?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `maxSize` | `integer` | yes | `1000` |

Returns: a new `SQLTagStore` bound to this database. Variant: **sync**.

**`applyChangeset(changeset, options?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `changeset` | `Uint8Array` | no | — |
| `options` | `ApplyChangesetOptions` | yes | `{}` |

Returns: `boolean` — `true` if applied without conflict/abort, `false` if an
`onConflict` handler (or the default behavior) caused the changeset to be
only partially applied or fully rolled back. Throws: `ERR_SQLITE_ERROR` for a
malformed changeset blob; whatever the `filter`/`onConflict` callback itself
throws propagates out (synchronously — no dropped exceptions). Variant:
**sync**.

**`enableDefensive(active)` / `enableLoadExtension(allow)`** — single
`boolean` param, `void` return, `ERR_SQLITE_ERROR` if the underlying
`sqlite3_db_config()` call fails. Variant: **sync**.

**`exec(sql)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `sql` | `string` | no | — |

Returns: `void`. Runs `sqlite3_exec()`-equivalent semantics: one or more
`;`-separated statements, no bound parameters, no result rows returned to the
caller (any `SELECT` output is discarded — use `prepare()` for reading rows
back). Throws: `ERR_SQLITE_ERROR` on any syntax/constraint/runtime SQL error
(execution stops at the first failing statement; prior statements in the same
`exec()` call are **not** automatically rolled back unless the SQL itself
wraps them in an explicit transaction). Variant: **sync**.

**`function(name, [options,] fn)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `name` | `string` | no | — |
| `options` | `FunctionOptions` | yes | all fields `false` |
| `fn` | `SqlFunction` | no | — |

Returns: `void`. Registers a scalar SQL function backed by a JS function.
Throws: `ERR_INVALID_ARG_TYPE` if `fn` is not callable; `ERR_SQLITE_ERROR` if
SQLite registration fails. Variant: **sync** (registration); `fn` itself is
invoked synchronously/re-entrantly during later query execution, exactly like
`aggregate`'s callbacks.

**`loadExtension(path)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `path` | `string` | no | — |

Returns: `void`. Throws: `ERR_LOAD_SQLITE_EXTENSION` if `allowExtension` was
not set to `true` on the constructor, or if the shared library fails to load
/ export the expected `sqlite3_extension_init` entry point. **Security note**:
loading an extension executes arbitrary native code in-process — never accept
an extension path from untrusted input. Variant: **sync**.

**`location(dbName?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `dbName` | `string` | yes | `'main'` |

Returns: `string` (absolute file path) or `null` for an in-memory / temporary
database. Throws: `ERR_SQLITE_ERROR` if `dbName` does not name an attached
database. Variant: **sync**.

**`open()`** — no params, `void` return. Only meaningful when the database
was constructed with `{ open: false }`. Throws: `ERR_INVALID_STATE` if already
open; `ERR_SQLITE_ERROR` if the underlying open fails. Variant: **sync**.

**`prepare(sql, options?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `sql` | `string` | no | — |
| `options` | `PrepareOptions` | yes | inherited from the `DatabaseSync` instance |

Returns: a new `StatementSync`. Throws: `ERR_SQLITE_ERROR` wrapping
`sqlite3_prepare_v2()` syntax errors (unknown table/column, malformed SQL).
Variant: **sync**.

**`setAuthorizer(callback)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `callback` | `AuthorizerCallback \| null` | no | — (`null` clears/uninstalls) |

Returns: `void`. The callback is invoked synchronously, once per
schema/statement-compile-time action, and must return one of
`sqlite.constants.SQLITE_OK` / `SQLITE_DENY` / `SQLITE_IGNORE`. Throws:
whatever the callback itself throws propagates synchronously out of the
`prepare()`/`exec()` call that triggered it. Variant: **sync**.

**`[Symbol.dispose]()`** — no params, `void` return, equivalent to `close()`
but a no-op if already closed (safe for `using db = new DatabaseSync(...)`).
Added v23.11.0. Variant: **sync**.

**Instance properties**

| Property | Type | Access | Description |
|---|---|---|---|
| `isOpen` | `boolean` | read-only | Whether the connection is currently open. |
| `isTransaction` | `boolean` | read-only | Whether a transaction (`BEGIN`) is currently active on this connection. |
| `limits` | `LimitsObject` | read-only object, writable fields | Live view of the SQLite runtime limits (`sqlite3_limit()`); each named field can be read and assigned (assigning `Infinity` resets to the SQLite compile-time maximum). |

---

#### `class StatementSync`

Not constructible directly (`new StatementSync()` throws `ERR_ILLEGAL_CONSTRUCTOR` /
is simply not exposed as a public constructor) — obtained only via
`database.prepare(sql)` or a `SQLTagStore` tag call. Wraps one compiled
`sqlite3_stmt`, bound to the `DatabaseSync` that created it (a
`StatementSync` becomes unusable, throwing `ERR_INVALID_STATE`, once its
parent database is `close()`d).

**Instance methods** (9 total):

| Method | Signature |
|---|---|
| `all` | `all(...params: BindParams): Row[]` |
| `get` | `get(...params: BindParams): Row \| undefined` |
| `iterate` | `iterate(...params: BindParams): IterableIterator<Row>` |
| `run` | `run(...params: BindParams): RunResult` |
| `columns` | `columns(): ColumnMetadata[]` |
| `setAllowBareNamedParameters` | `setAllowBareNamedParameters(enabled: boolean): void` |
| `setAllowUnknownNamedParameters` | `setAllowUnknownNamedParameters(enabled: boolean): void` |
| `setReadBigInts` | `setReadBigInts(enabled: boolean): void` |
| `setReturnArrays` | `setReturnArrays(enabled: boolean): void` |

`BindParams` = an optional leading named-parameters object followed by zero
or more anonymous positional values: `(namedParameters?: Record<string, SqlInputValue> | SqlInputValue, ...anonymousParameters: SqlInputValue[])`.

**`all(...params)`** — binds `params`, fully drains the statement (repeated
`sqlite3_step()` until `SQLITE_DONE`), resets it. Returns: `Row[]` — empty
array if the statement produces zero rows or is not a row-producing
statement. Throws: `ERR_SQLITE_ERROR` on a runtime SQL error mid-execution
(e.g. constraint violation triggered by an SQL function); `ERR_OUT_OF_RANGE`
if an `INTEGER` column value falls outside the JS safe-integer range and
`readBigInts` is not enabled for this statement. Variant: **sync**.

**`get(...params)`** — binds, steps once, resets. Returns: `Row` (the first
row) or `undefined` if there are no rows. Same throws as `all()`. Variant:
**sync**.

**`iterate(...params)`** — binds, returns a lazy `IterableIterator<Row>`
that calls `sqlite3_step()` on each `.next()`. Returns: the iterator itself
(also implements `[Symbol.iterator]`, usable directly in `for...of`). Same
per-row throws as `all()`, raised from the `.next()` call that reaches the
failing row. The underlying statement is considered **busy** until the
iterator is fully drained or explicitly closed (`.return()`); attempting to
call `all`/`get`/`run`/another `iterate()` on the same `StatementSync` while
a prior iterator is still open throws `ERR_INVALID_STATE` (verify — inherent
to `sqlite3_stmt` being a single cursor, not merely a Node policy choice).
Variant: **sync** (each step is synchronous; the *iteration protocol* is
what makes it lazy, not concurrency).

**`run(...params)`** — binds, single `sqlite3_step()` cycle intended for
non-row-producing statements (`INSERT`/`UPDATE`/`DELETE`/DDL), resets.
Returns: `RunResult` (`{ changes: number | bigint, lastInsertRowid: number | bigint }`
— sourced from `sqlite3_changes64()`/`sqlite3_last_insert_rowid()` on the
parent connection, not the statement; type is `bigint` iff `readBigInts` is
enabled). Throws: same as `all()`. Variant: **sync**.

**`columns()`** — no params. Returns: `ColumnMetadata[]`, one entry per
result column, using `sqlite3_column_database_name()`/`table_name()`/
`origin_name()`/`name()`/`decltype()` (requires the SQLite build to have
`SQLITE_ENABLE_COLUMN_METADATA` compiled in — see §5.1). Fields are `null`
when the column is a computed expression with no backing table/database
(e.g. `SELECT 1 + 1`). Added v23.11.0. Variant: **sync**.

**`setAllowBareNamedParameters` / `setAllowUnknownNamedParameters` /
`setReadBigInts` / `setReturnArrays`** — single `boolean` param, `void`
return, per-statement override of the corresponding `DatabaseSync`/
`prepare()`-time default. Variant: **sync**.

**Instance properties**

| Property | Type | Access | Description |
|---|---|---|---|
| `sourceSQL` | `string` | read-only | The exact SQL text passed to `prepare()`. |
| `expandedSQL` | `string` | read-only | `sourceSQL` with every bound-parameter placeholder substituted by its most-recently-bound literal value (via `sqlite3_expanded_sql()`) — for logging/debugging, reflects only the **last** execution's bindings. |

---

#### `class Session`

Created via `database.createSession(options?)`. Wraps the SQLite session
extension's `sqlite3_session` object, recording every row-level change made
on the tracked table(s)/database since creation.

**Instance methods** (4 total):

| Method | Signature |
|---|---|
| `changeset` | `changeset(): Uint8Array` |
| `patchset` | `patchset(): Uint8Array` |
| `close` | `close(): void` |
| `[Symbol.dispose]` | `[Symbol.dispose](): void` |

**`changeset()`** — no params. Returns: `Uint8Array`, the SQLite binary
changeset format (full before/after images for `UPDATE`s) covering every
change recorded since the session was created (or since the last call — the
session's internal change log accumulates continuously; each call is a
non-destructive snapshot of everything recorded so far, callable multiple
times). Throws: `ERR_SQLITE_ERROR` if the session is closed or SQLite fails
to serialize. Variant: **sync**.

**`patchset()`** — same shape as `changeset()` but the more compact SQLite
"patchset" format (only the columns that actually changed, no full
before-image for `UPDATE`s — smaller but not independently invertible).
Variant: **sync**.

**`close()`** — detaches and frees the underlying `sqlite3_session` object.
Idempotent is **not** guaranteed by the documented surface (verify —
recommend the RTS `.ts` shim make it idempotent regardless, matching the
`[Symbol.dispose]` no-op-if-closed contract below). Variant: **sync**.

**`[Symbol.dispose]()`** — closes the session; no-op if already closed.
Added v24.9.0. Variant: **sync**.

No instance properties documented.

---

#### `class SQLTagStore`

Created via `database.createTagStore(maxSize?)`. An LRU cache of prepared
`StatementSync` instances keyed by the tagged-template's static string parts,
exposing the four query verbs as **tag functions** so bound values are always
passed as real SQL parameters (never string-interpolated into the SQL text —
this is the built-in SQL-injection-safe query helper).

**Instance methods** (5 total):

| Method | Signature |
|---|---|
| `all` | `` all(strings: TemplateStringsArray, ...values: SqlInputValue[]): Row[] `` |
| `get` | `` get(strings: TemplateStringsArray, ...values: SqlInputValue[]): Row \| undefined `` |
| `iterate` | `` iterate(strings: TemplateStringsArray, ...values: SqlInputValue[]): IterableIterator<Row> `` |
| `run` | `` run(strings: TemplateStringsArray, ...values: SqlInputValue[]): RunResult `` |
| `clear` | `clear(): void` |

Each of `all`/`get`/`iterate`/`run` is used **only** as a tagged template
(`sql.all\`SELECT ...\``), reconstructs the parameterized SQL text from
`strings` (joining with `?` placeholders), looks up or lazily `prepare()`s +
caches the resulting `StatementSync` (evicting the least-recently-used entry
past `capacity`), binds `values` positionally, and delegates to the
identically-named `StatementSync` method. Same return types, throws, and
sync variant as their `StatementSync` counterparts above.

**`clear()`** — no params, `void` return. Evicts every cached prepared
statement (each is finalized, not merely dropped from the JS-visible cache).
Variant: **sync**.

**Instance properties**

| Property | Type | Access | Description |
|---|---|---|---|
| `size` | `integer` | read-only getter (changed from a plain value to a getter in v25.4.0) | Current number of cached prepared statements. |
| `capacity` | `integer` | read-only | The `maxSize` passed to `createTagStore()` (default `1000`). |
| `db` | `DatabaseSync` | read-only | The owning database instance. |

### 2.2 Top-level functions

| Function | Signature |
|---|---|
| `backup` | `backup(sourceDb: DatabaseSync, path: string \| Buffer \| URL, options?: BackupOptions): Promise<number>` |

**`backup(sourceDb, path, options?)`**

| Param | Type | Optional | Default |
|---|---|---|---|
| `sourceDb` | `DatabaseSync` | no | — (must be open) |
| `path` | `string \| Buffer \| URL` | no | — destination file |
| `options` | `BackupOptions` | yes | `{ source: 'main', target: 'main', rate: 100 }` |

Returns: `Promise<number>` resolving to the total number of pages copied.
Throws (via promise rejection): `ERR_SQLITE_ERROR` if `sourceDb` is not open,
the named `source` schema does not exist, or the destination cannot be
opened/written; whatever the `progress` callback itself throws propagates as
a rejection. Variant: **promise** — this is the module's **only** async
entry point; internally repeats `sqlite3_backup_step(rate pages)` in a loop,
invoking `progress({ totalPages, remainingPages })` after every batch until
`sqlite3_backup_finish()`. `functionCount` for this spec's bookkeeping is
**1** (this is the sole top-level function; every other export is a class or
the `constants` object).

### 2.3 Properties & constants

**`sqlite.constants`** (plain object, all values `number`)

Conflict-resolution outcomes for a session `onConflict` handler to return:

| Constant | Meaning |
|---|---|
| `SQLITE_CHANGESET_OMIT` | Skip this change, keep applying the rest. |
| `SQLITE_CHANGESET_REPLACE` | Replace the conflicting row (`UPDATE`/`INSERT` conflicts only). |
| `SQLITE_CHANGESET_ABORT` | Abort the whole `applyChangeset()` call, rolling it back. |

Conflict-type codes passed **into** an `onConflict` handler:

| Constant | Meaning |
|---|---|
| `SQLITE_CHANGESET_DATA` | Row being updated/deleted no longer matches its pre-image. |
| `SQLITE_CHANGESET_NOTFOUND` | Row being updated/deleted no longer exists. |
| `SQLITE_CHANGESET_CONFLICT` | `INSERT` collides with an existing primary key. |
| `SQLITE_CHANGESET_CONSTRAINT` | A `FOREIGN KEY`/`CHECK`/`UNIQUE`/etc. constraint would be violated. |
| `SQLITE_CHANGESET_FOREIGN_KEY` | Foreign-key constraint violated after applying the whole changeset. |

Authorizer return values (what `setAuthorizer`'s callback must return):

| Constant | Meaning |
|---|---|
| `SQLITE_OK` | Allow the action. |
| `SQLITE_DENY` | Deny the action — aborts the statement compile with an error. |
| `SQLITE_IGNORE` | Silently substitute `NULL` (for reads) / skip the operation (for writes), without erroring. |

Authorizer **action codes** (the first argument the callback receives,
identifying what SQL construct is being authorized): `SQLITE_CREATE_INDEX`,
`SQLITE_CREATE_TABLE`, `SQLITE_CREATE_TEMP_INDEX`, `SQLITE_CREATE_TEMP_TABLE`,
`SQLITE_CREATE_TEMP_TRIGGER`, `SQLITE_CREATE_TEMP_VIEW`,
`SQLITE_CREATE_TRIGGER`, `SQLITE_CREATE_VIEW`, `SQLITE_DELETE`,
`SQLITE_DROP_INDEX`, `SQLITE_DROP_TABLE`, `SQLITE_DROP_TEMP_INDEX`,
`SQLITE_DROP_TEMP_TABLE`, `SQLITE_DROP_TEMP_TRIGGER`,
`SQLITE_DROP_TEMP_VIEW`, `SQLITE_DROP_TRIGGER`, `SQLITE_DROP_VIEW`,
`SQLITE_INSERT`, `SQLITE_PRAGMA`, `SQLITE_READ`, `SQLITE_SELECT`,
`SQLITE_TRANSACTION`, `SQLITE_UPDATE`, `SQLITE_ATTACH`, `SQLITE_DETACH`,
`SQLITE_ALTER_TABLE`, `SQLITE_REINDEX`, `SQLITE_ANALYZE`,
`SQLITE_CREATE_VTABLE`, `SQLITE_DROP_VTABLE`, `SQLITE_FUNCTION`,
`SQLITE_SAVEPOINT`, `SQLITE_COPY`, `SQLITE_RECURSIVE` (32 codes total,
1:1 with SQLite's own `sqlite3_set_authorizer()` action-code enum).

No other module-level properties/constants exist (no `sqlite.VERSION`
string is documented on this module — SQLite's own version can only be read
via `sqlite_version()` inside SQL, e.g. `db.prepare('SELECT sqlite_version()').get()`).

### 2.4 Events

None. No class in `node:sqlite` is `EventEmitter`-based — every operation is
either a direct synchronous call/throw, or (for `backup()` only) a `Promise`
with a plain callback-style `progress` option; there is no `.on()`/`.emit()`
surface anywhere in this module.

## 3. Types & option objects

```typescript
interface DatabaseSyncOptions {
  open?: boolean;                              // default: true
  readOnly?: boolean;                           // default: false
  enableForeignKeyConstraints?: boolean;        // default: true — PRAGMA foreign_keys
  enableDoubleQuotedStringLiterals?: boolean;   // default: false — SQLITE_DBCONFIG_DQS_DDL/DML
  allowExtension?: boolean;                     // default: false — gates loadExtension()
  timeout?: number;                             // default: 0 — busy-timeout, ms
  readBigInts?: boolean;                        // default: false
  returnArrays?: boolean;                       // default: false
  allowBareNamedParameters?: boolean;           // default: true
  allowUnknownNamedParameters?: boolean;        // default: false
  defensive?: boolean;                          // default: true (since v25.5.0/v24.14.0; previously false)
  limits?: Partial<LimitsObject>;
}

interface LimitsObject {
  length?: number;            // SQLITE_LIMIT_LENGTH
  sqlLength?: number;         // SQLITE_LIMIT_SQL_LENGTH
  column?: number;            // SQLITE_LIMIT_COLUMN
  exprDepth?: number;         // SQLITE_LIMIT_EXPR_DEPTH
  compoundSelect?: number;    // SQLITE_LIMIT_COMPOUND_SELECT
  vdbeOp?: number;            // SQLITE_LIMIT_VDBE_OP
  functionArg?: number;       // SQLITE_LIMIT_FUNCTION_ARG
  attach?: number;            // SQLITE_LIMIT_ATTACHED
  likePatternLength?: number; // SQLITE_LIMIT_LIKE_PATTERN_LENGTH
  variableNumber?: number;    // SQLITE_LIMIT_VARIABLE_NUMBER
  triggerDepth?: number;      // SQLITE_LIMIT_TRIGGER_DEPTH
}

type SqlInputValue = null | number | bigint | string | Uint8Array | DataView;
type SqlOutputValue = null | number | bigint | string | Uint8Array;
type Row = Record<string, SqlOutputValue> | SqlOutputValue[]; // array form iff returnArrays

interface RunResult {
  changes: number | bigint;         // bigint iff readBigInts enabled
  lastInsertRowid: number | bigint; // bigint iff readBigInts enabled
}

interface ColumnMetadata {
  column: string | null;    // origin column name (sqlite3_column_origin_name)
  database: string | null;  // origin database/schema name
  name: string;             // result-set column name (may be an alias)
  table: string | null;     // origin table name
  type: string | null;      // declared column type from the schema (decltype), null for expressions
}

interface PrepareOptions {
  readBigInts?: boolean;
  returnArrays?: boolean;
  allowBareNamedParameters?: boolean;
  allowUnknownNamedParameters?: boolean;
}

interface AggregateOptions {
  deterministic?: boolean;   // default: false
  directOnly?: boolean;      // default: false
  useBigIntArguments?: boolean; // default: false
  varargs?: boolean;         // default: false
  start: SqlInputValue | (() => SqlInputValue); // identity/seed value, may itself be a thunk
  step: (accumulator: SqlOutputValue, ...args: SqlOutputValue[]) => SqlOutputValue;
  result?: (accumulator: SqlOutputValue) => SqlOutputValue; // default: identity
  inverse?: (accumulator: SqlOutputValue, ...args: SqlOutputValue[]) => SqlOutputValue; // enables window-fn use
}

interface FunctionOptions {
  deterministic?: boolean;
  directOnly?: boolean;
  useBigIntArguments?: boolean;
  varargs?: boolean;
}

type SqlFunction = (...args: SqlOutputValue[]) => SqlInputValue;

type AuthorizerCallback = (
  actionCode: number,             // one of sqlite.constants.SQLITE_* action codes
  arg1: string | null,
  arg2: string | null,
  dbName: string | null,
  triggerOrView: string | null,
) => number;                      // one of SQLITE_OK / SQLITE_DENY / SQLITE_IGNORE

interface CreateSessionOptions {
  table?: string;   // limit tracking to one table; default: all tables
  db?: string;      // default: 'main'
}

interface ApplyChangesetOptions {
  filter?: (tableName: string) => boolean;               // false ⇒ skip that table's changes entirely
  onConflict?: (conflictType: number, changeInfo: unknown) => number; // returns an SQLITE_CHANGESET_{OMIT,REPLACE,ABORT}
}

interface BackupOptions {
  source?: string;    // default: 'main'
  target?: string;    // default: 'main'
  rate?: number;       // default: 100 — pages copied per sqlite3_backup_step() batch
  progress?: (info: { totalPages: number; remainingPages: number }) => void;
}
```

## 4. Node semantics & edge cases

### Type conversion (JS ↔ SQLite storage class)

| SQLite storage class | JS → SQLite (bind) | SQLite → JS (read) |
|---|---|---|
| `NULL` | `null` | `null` |
| `INTEGER` | `number` (safe integer) or `bigint` | `number`, or `bigint` if `readBigInts` |
| `REAL` | `number` | `number` |
| `TEXT` | `string` | `string` |
| `BLOB` | `TypedArray` or `DataView` | `Uint8Array` |

Reading an `INTEGER` outside `Number.MIN_SAFE_INTEGER..Number.MAX_SAFE_INTEGER`
**without** `readBigInts` enabled throws `ERR_OUT_OF_RANGE` — SQLite's
64-bit integer storage class routinely exceeds JS's 53-bit safe range
(`rowid`s, `AUTOINCREMENT` counters, hashes stored as integers, etc.), so
this is a common, expected failure mode for real-world schemas, not a rare
corner case.

### Parameter binding

- **Anonymous**: `stmt.run(1, 'hello')` binds positionally to `?`
  placeholders (1-indexed internally, exposed as ordinary JS args).
- **Named, prefixed**: `stmt.run({ ':id': 1, '@name': 'x' })` — the object
  key must include the same sigil (`:`, `@`, or `$`) used in the SQL text.
- **Named, bare**: `stmt.run({ id: 1 })` works only when
  `allowBareNamedParameters` is `true` (the default) — the sigil is inferred.
- **Unknown named parameter**: an object key with no matching placeholder in
  the SQL throws unless `allowUnknownNamedParameters` is `true` (silently
  ignored in that mode).
- Named and anonymous styles are **not** meant to be mixed in one call
  (mixing has undefined/unspecified precedence — treat as programmer error).

### Statement reuse and cursor state

A `StatementSync` wraps exactly one `sqlite3_stmt*` — a single, stateful
cursor. `all()`/`get()`/`run()` each fully reset (`sqlite3_reset` +
`sqlite3_clear_bindings`) before binding their own fresh parameters, so
calling the same prepared statement repeatedly with different arguments is
the whole point (this is why `prepare()` exists separately from `exec()`).
`iterate()`'s returned iterator, however, leaves the cursor **open** between
`.next()` calls — invoking any other method on that same `StatementSync`
while the iterator is not yet exhausted is expected to fail (busy statement);
always drain or explicitly close an iterator before reusing its statement.

### `exec()` vs `prepare()`

`exec()` takes a full script of `;`-separated statements with **no**
parameter binding and **discards** any rows a `SELECT` would have produced —
it exists for schema/migration scripts (`CREATE TABLE`, `PRAGMA`, multi
statement setup), not for reading data back. Use `prepare()` (or
`SQLTagStore`) whenever the caller needs rows or needs to bind values safely
(never string-concatenate untrusted input into `exec()`'s SQL text).

### `defensive` mode and `loadExtension` security

`defensive` (default `true` since v25.5.0/v24.14.0) restricts the SQL
surface an attacker could otherwise use to bypass application-level
safety even with a valid connection (e.g. writing to `sqlite_dbpage`
virtual tables, certain schema-manipulation tricks). `loadExtension()` is a
**separate**, off-by-default (`allowExtension: false`) capability that loads
and executes arbitrary native code from a shared library file — never
enable it for a path that is even partially attacker-influenced.

### Transactions

`isTransaction` reflects whether a `BEGIN` is currently open on the
connection. There is no dedicated `db.transaction(fn)` helper documented (unlike
`better-sqlite3`) — transactions are plain SQL (`exec('BEGIN')` /
`exec('COMMIT')` / `exec('ROLLBACK')`), or via a `.ts`-level convenience the
RTS `.ts` shim may add non-natively.

### Windows vs POSIX

- File locking: SQLite uses POSIX advisory `fcntl` locks on Unix (byte-range,
  cooperative — a crashed process's lock releases automatically) vs Windows
  mandatory range locks via `LockFile`/`LockFileEx` (also process-crash-safe,
  but semantically stricter — a locked byte range is enforced by the OS even
  against a well-behaved process ignoring the convention).
- Default journal mode is the rollback journal (`DELETE` mode) unless the
  application issues `PRAGMA journal_mode=WAL` — WAL mode uses `mmap` +
  shared-memory (`-shm` file) on both platforms, with the same portability
  caveats SQLite documents for network filesystems (WAL is **not** safe over
  most network filesystems on either OS).
- `path` given as a `file://` URL follows the same drive-letter/UNC quirks
  Node's `fs` module already handles for `URL`→path conversion (reuse that
  logic rather than reimplementing it — see §5.1).

### Deprecations

None — every documented member is current; nothing in `node:sqlite`'s surface
is marked deprecated as of Node 25.

### Ordering guarantees

All operations on one `DatabaseSync`/`StatementSync` execute in exact call
order (fully synchronous, single-threaded API — no reordering, no implicit
batching). Cross-connection ordering to the same underlying file is governed
entirely by SQLite's own locking protocol, not by Node/RTS.

### Backpressure

Not applicable to the synchronous surface (no streams). `backup()`'s `rate`
option (pages per `sqlite3_backup_step()` batch) is the only knob resembling
backpressure — a smaller `rate` yields more, smaller synchronous bursts with
more opportunities for the `progress` callback and (in RTS) for the event
loop to interleave other work between batches.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` is fully independent of `rts-std`; it vendors its **own** SQLite
C library binding rather than reusing any existing RTS runtime surface.

| Surface area | Backing |
|---|---|
| The SQLite C library itself | **`libsqlite3-sys`** (raw FFI bindings) with its `bundled` Cargo feature — statically compiles the SQLite amalgamation into the `rts-node` binary/staticlib, matching Node's own "vendored, no system dependency" distribution model and RTS's "standalone toolchain, no external runtime support library" goal. |
| Connection open/close/pragma/config | Direct calls to `sqlite3_open_v2`, `sqlite3_close_v2`, `sqlite3_busy_timeout`, `sqlite3_db_config` (`SQLITE_DBCONFIG_DQS_DDL`/`DML`, `SQLITE_DBCONFIG_DEFENSIVE`), `sqlite3_enable_load_extension`, `sqlite3_limit`. |
| Prepared statements, step, column read, bind | Direct `sqlite3_prepare_v2`, `sqlite3_step`, `sqlite3_reset`, `sqlite3_clear_bindings`, `sqlite3_column_{type,int64,double,text,blob,bytes,count,name}`, `sqlite3_bind_{null,int64,double,text,blob}`, `sqlite3_bind_parameter_{count,name}`. |
| `run()`'s `changes`/`lastInsertRowid` | `sqlite3_changes64(db)` / `sqlite3_last_insert_rowid(db)` — connection-level, not statement-level. |
| `columns()` | `sqlite3_column_{database_name,table_name,origin_name,name,decltype}` — **requires the vendored build to define `SQLITE_ENABLE_COLUMN_METADATA`** (not on by default in the plain amalgamation); `rts-node`'s `build.rs` must add this compile-time flag when invoking `libsqlite3-sys`'s bundled build (via its `bundled_bindgen`/cc-passthrough knobs), exactly as Node's own build does. |
| `Session`/`changeset`/`patchset`/`applyChangeset` | The SQLite **session extension** (`sqlite3session_create/attach/changeset/patchset/delete`, `sqlite3changeset_apply` with `xFilter`/`xConflict` callbacks) — **requires `SQLITE_ENABLE_SESSION` and `SQLITE_ENABLE_PREUPDATE_HOOK`** compiled in, also not on by default. Same `build.rs` flag-wiring requirement as column metadata; verify the exact `libsqlite3-sys`/`cc` incantation at implementation time, since these two compile-time defines are the two features most likely to be missing from a naive `bundled`-only setup. |
| User-defined scalar/aggregate functions | `sqlite3_create_function_v2` (scalar) / the xStep/xFinal/xValue/xInverse aggregate registration form, with a Rust `extern "C"` trampoline that reads `sqlite3_value*` args via `sqlite3_value_{type,int64,double,text,blob}`, invokes the stored JS `Function` handle **synchronously and re-entrantly** (see §5.3), and writes the JS return value back via `sqlite3_result_{null,int64,double,text,blob}`/`sqlite3_result_error`. |
| `setAuthorizer` | `sqlite3_set_authorizer` with a similar synchronous JS-callback trampoline (5 string/null args in, one `int` out). |
| `backup()` | `sqlite3_backup_init`/`_step`/`_remaining`/`_pagecount`/`_finish` — looped in `rate`-sized batches on a background thread (see §5.3). |
| `loadExtension` | `sqlite3_load_extension` (gated behind `enableLoadExtension`/`allowExtension`, itself requiring `SQLITE_ENABLE_LOAD_EXTENSION` in the vendored build or the equivalent runtime `sqlite3_enable_load_extension` call, which `libsqlite3-sys`'s default bundled build does support at runtime). |

**Why raw `libsqlite3-sys` instead of the higher-level `rusqlite` crate**: `rusqlite`'s
safe `Statement<'conn>` type **borrows** its parent `Connection` for its
entire lifetime — a natural fit for scoped Rust code, but a poor fit for
RTS's `HandleTable`, where a `StatementSync` handle must be independently
freeable/GC-tracked with no compile-time borrow relationship to its parent
`DatabaseSync` handle. Working directly against the raw `sqlite3*`/
`sqlite3_stmt*` pointers (exactly what Node's own C++ binding does — see
`src/node_sqlite.cc` on the Node source tree) sidesteps that mismatch
entirely and gives `rts-node` full control over the two build-time defines
above. (`rusqlite` remains a reasonable **reference implementation** to
diff error/edge-case behavior against during testing, even though it is not
the runtime dependency.)

**Handle storage** (new `rts-node`-local handle kinds — this crate keeps its
own handle table or a `rts-engine`-hosted `Entry` extension, per the same
pattern already used for e.g. `Entry::UdpSocket`):

- `SqliteConnEntry { db: *mut sqlite3, path: String, open: bool, owner_thread: ThreadId, registered_functions: Vec<...>, authorizer: Option<Handle /* Function */> }`
- `SqliteStmtEntry { stmt: *mut sqlite3_stmt, conn: Handle, source_sql: String, busy_iterating: bool, opts: PerStatementOptions }` — `conn` keeps the parent `DatabaseSync` handle alive (ref-counted or GC-rooted) for the statement's lifetime, since `sqlite3_stmt*` is only valid while its `sqlite3*` is open.
- `SqliteSessionEntry { session: *mut sqlite3_session, conn: Handle }`
- `SQLTagStore` needs **no new native handle kind** — it is implementable
  almost entirely as a `.ts` shim: an LRU map from the template's joined
  static-string key to a `StatementSync` handle, calling the existing
  `prepare()`/`all`/`get`/`iterate`/`run` externs. `size`/`capacity`/`db`
  are plain `.ts`-side fields.

`*mut sqlite3`/`*mut sqlite3_stmt` are raw pointers and therefore not
`Send`/`Sync` by default; SQLite itself is safe to use from multiple threads
only in its default "serialized" build mode (mutex-protected internally) —
`libsqlite3-sys`'s bundled build uses this mode by default, so cross-thread
access is *memory-safe*, but RTS should still enforce **one connection, one
owning thread** at the RTS-semantics level (§5.4) to match Node's own model
and avoid surprising lock-contention stalls on a supposedly-synchronous API.

### 5.2 ABI surface

`ns_prefix = "node_sqlite"`, `node_module = "sqlite"`, registered in
`rts-node`'s `NODE_SPECS` like every other node module. All SQLite objects
(`sqlite3*` / `sqlite3_stmt*` / `sqlite3_session*`) are opaque `Handle`s; the
`DatabaseSync`/`StatementSync`/`Session`/`SQLTagStore` classes, option
normalization, named/anonymous parameter-binding dispatch, row-object vs
row-array assembly, and every error-code → `Error` mapping live in a `.ts`
shim over these raw externs.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| **Connection lifecycle** | | | |
| `__RTS_FN_NODE_SQLITE_OPEN` | `StrPtr(path), Bool(read_only), Bool(create_if_missing)` | `Handle` (or a sentinel + separate error-fetch, see below) | `create_if_missing` = `!readOnly` per SQLite's own `SQLITE_OPEN_CREATE` convention. |
| `__RTS_FN_NODE_SQLITE_CLOSE` | `Handle` | `I32` (status) | `sqlite3_close_v2` — safe with outstanding statement handles. |
| `__RTS_FN_NODE_SQLITE_IS_OPEN` | `Handle` | `Bool` | |
| `__RTS_FN_NODE_SQLITE_LOCATION` | `Handle, StrPtr(db_name)` | `StrPtr` | Empty-sentinel for in-memory/temp. |
| `__RTS_FN_NODE_SQLITE_EXEC` | `Handle, StrPtr(sql)` | `I32` (status) | |
| `__RTS_FN_NODE_SQLITE_LAST_ERROR_MESSAGE` | `Handle` | `StrPtr` | Fetches `sqlite3_errmsg`/code after any op returns a nonzero status — the uniform "check status, then fetch detail" pattern used across every fallible extern in this table. |
| `__RTS_FN_NODE_SQLITE_LAST_ERROR_CODE` | `Handle` | `I32` | `sqlite3_extended_errcode`. |
| **Config / pragma-equivalents** | | | |
| `__RTS_FN_NODE_SQLITE_SET_BUSY_TIMEOUT` | `Handle, I32(ms)` | `I32` | |
| `__RTS_FN_NODE_SQLITE_SET_FOREIGN_KEYS` | `Handle, Bool` | `I32` | `PRAGMA foreign_keys`. |
| `__RTS_FN_NODE_SQLITE_SET_DQS` | `Handle, Bool(ddl), Bool(dml)` | `I32` | `SQLITE_DBCONFIG_DQS_DDL`/`DML`. |
| `__RTS_FN_NODE_SQLITE_SET_DEFENSIVE` | `Handle, Bool` | `I32` | |
| `__RTS_FN_NODE_SQLITE_ENABLE_LOAD_EXTENSION` | `Handle, Bool` | `I32` | |
| `__RTS_FN_NODE_SQLITE_LOAD_EXTENSION` | `Handle, StrPtr(path)` | `I32` | |
| `__RTS_FN_NODE_SQLITE_GET_LIMIT` / `_SET_LIMIT` | `Handle, I32(limit_id)` [, `I64(new_value)`] | `I64` | `limit_id` is the 11-entry enum from §3's `LimitsObject`, mapped 1:1 to SQLite's `SQLITE_LIMIT_*` ids. |
| **Prepared statements** | | | |
| `__RTS_FN_NODE_SQLITE_PREPARE` | `Handle(conn), StrPtr(sql)` | `Handle` (stmt) | |
| `__RTS_FN_NODE_SQLITE_STMT_FINALIZE` | `Handle(stmt)` | `Void` | Also invoked by GC finalization of the JS-side wrapper. |
| `__RTS_FN_NODE_SQLITE_STMT_RESET` | `Handle(stmt)` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_CLEAR_BINDINGS` | `Handle(stmt)` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_STEP` | `Handle(stmt)` | `I32` (0=`SQLITE_DONE`, 1=`SQLITE_ROW`, negative=error) | |
| `__RTS_FN_NODE_SQLITE_STMT_SOURCE_SQL` / `_EXPANDED_SQL` | `Handle(stmt)` | `StrPtr` | |
| `__RTS_FN_NODE_SQLITE_STMT_BIND_PARAMETER_COUNT` | `Handle(stmt)` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_BIND_PARAMETER_NAME` | `Handle(stmt), I32(idx)` | `StrPtr` | For named→bare/sigil matching in the `.ts` shim. |
| `__RTS_FN_NODE_SQLITE_STMT_BIND_NULL` | `Handle(stmt), I32(idx)` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_BIND_INT64` | `Handle(stmt), I32(idx), I64` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_BIND_DOUBLE` | `Handle(stmt), I32(idx), F64` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_BIND_TEXT` | `Handle(stmt), I32(idx), StrPtr` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_BIND_BLOB` | `Handle(stmt), I32(idx), U64(ptr), I64(len)` | `I32` | `ptr`/`len` from the stable `ArrayBuffer`/`Buffer` pointer (§5.5). |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_COUNT` | `Handle(stmt)` | `I32` | |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_NAME` | `Handle(stmt), I32(idx)` | `StrPtr` | |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_TYPE` | `Handle(stmt), I32(idx)` | `I32` (SQLite's own `SQLITE_{INTEGER=1,FLOAT=2,TEXT=3,BLOB=4,NULL=5}`) | Drives which typed getter the `.ts` shim calls next — the "tag, then typed fetch" pattern used for every dynamically-typed value crossing this ABI. |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_INT64` | `Handle(stmt), I32(idx)` | `I64` | |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_DOUBLE` | `Handle(stmt), I32(idx)` | `F64` | |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_TEXT` | `Handle(stmt), I32(idx)` | `StrPtr` | |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_BLOB` | `Handle(stmt), I32(idx)` | `Handle` (a fresh `Buffer`/`ArrayBuffer` handle, bytes copied out of SQLite's internal buffer) | |
| `__RTS_FN_NODE_SQLITE_STMT_COLUMN_DATABASE_NAME` / `_TABLE_NAME` / `_ORIGIN_NAME` / `_DECLTYPE` | `Handle(stmt), I32(idx)` | `StrPtr` | `columns()` metadata; empty-sentinel ⇒ `null` in `.ts`. Needs `SQLITE_ENABLE_COLUMN_METADATA` (§5.1). |
| `__RTS_FN_NODE_SQLITE_DB_LAST_INSERT_ROWID` | `Handle(conn)` | `I64` | For `run()`'s `RunResult`. |
| `__RTS_FN_NODE_SQLITE_DB_CHANGES` | `Handle(conn)` | `I64` | |
| `__RTS_FN_NODE_SQLITE_DB_IS_TRANSACTION` | `Handle(conn)` | `Bool` | `sqlite3_get_autocommit(db) == 0`. |
| **User-defined functions / authorizer** | | | |
| `__RTS_FN_NODE_SQLITE_REGISTER_FUNCTION` | `Handle(conn), StrPtr(name), Handle(js_fn), Bool(deterministic), Bool(direct_only), I32(arity_or_varargs)` | `I32` | `js_fn` is a `Function` handle (primordial) invoked synchronously from the native trampoline — see §5.3. |
| `__RTS_FN_NODE_SQLITE_REGISTER_AGGREGATE` | `Handle(conn), StrPtr(name), Handle(step_fn), Handle(result_fn_or_0), Handle(inverse_fn_or_0), Handle(start_value_or_thunk), ...flags` | `I32` | |
| `__RTS_FN_NODE_SQLITE_SET_AUTHORIZER` | `Handle(conn), Handle(js_fn_or_0)` | `I32` | `0` handle clears/uninstalls. |
| **Session / changeset** | | | |
| `__RTS_FN_NODE_SQLITE_SESSION_CREATE` | `Handle(conn), StrPtr(db_name), StrPtr(table_or_empty)` | `Handle` (session) | |
| `__RTS_FN_NODE_SQLITE_SESSION_CHANGESET` / `_PATCHSET` | `Handle(session)` | `Handle` (a `Buffer` handle) | |
| `__RTS_FN_NODE_SQLITE_SESSION_CLOSE` | `Handle(session)` | `Void` | |
| `__RTS_FN_NODE_SQLITE_APPLY_CHANGESET` | `Handle(conn), U64(ptr), I64(len), Handle(filter_fn_or_0), Handle(conflict_fn_or_0)` | `I32` (`Bool`-ish: 1 applied clean, 0 partial/aborted, negative error) | |
| **Backup** | | | |
| `__RTS_FN_NODE_SQLITE_BACKUP_START` | `Handle(source_conn), StrPtr(dest_path), StrPtr(source_name), StrPtr(target_name), I32(rate)` | `Handle` (an opaque backup-job handle) | Returns immediately; see §5.3 for the async loop. |
| `__RTS_FN_NODE_SQLITE_BACKUP_POLL` | `Handle(job)` | `I32` (`-1`=error, `0`=in-progress-more-pages, `1`=done) | Called from the background/blocking-pool thread's loop; drives one `rate`-sized `sqlite3_backup_step` batch per call. |
| `__RTS_FN_NODE_SQLITE_BACKUP_PROGRESS_TOTAL` / `_REMAINING` | `Handle(job)` | `I32` | For the `progress` callback payload. |
| `__RTS_FN_NODE_SQLITE_BACKUP_FINISH` | `Handle(job)` | `I32` (final status) | |

`Handle`s for `Function` callbacks (`js_fn`, `step_fn`, `filter_fn`, …) reuse
the **existing primordial `Function` handle representation** (`Entry::Function`
per the engine's async/Promise/Function spec) — invoking one synchronously
from inside a `extern "C"` trampoline triggered by SQLite is the same
"invoke a stored JS callback from native code" capability the engine already
needs for `Array.prototype.sort`/`map`/`forEach` comparators, not a new
primitive `rts-node` must invent.

### 5.3 Async model

`node:sqlite`'s public surface is **overwhelmingly synchronous** — a
deliberate design choice by Node itself (unlike `node:fs`, there is no
`sqlite/promises` variant). Only two things are not plain call/return:

- **User-defined function / aggregate / authorizer callbacks** — these are
  invoked **synchronously and re-entrantly** from inside a native SQLite
  callback that itself fires during `sqlite3_step()`/`sqlite3_prepare_v2()`.
  This needs **no event loop, no Promise, no tokio** — it is the same
  "call back into JS from native code, block until it returns, marshal the
  result" pattern already used for sort/map comparator callbacks elsewhere
  in the engine. The one wrinkle: SQLite forbids calling most `sqlite3_*`
  APIs on the **same connection** reentrantly from inside such a callback in
  certain contexts (e.g. modifying schema from inside an authorizer) —
  document, don't silently allow, if the JS callback tries to call back into
  the same `DatabaseSync` mid-callback in an unsupported way (surface
  SQLite's own `SQLITE_MISUSE`/`SQLITE_LOCKED` as an `ERR_SQLITE_ERROR`
  rather than crashing).
- **`backup()`** — the module's only genuinely async, Promise-returning
  operation. Recommended design: `BACKUP_START` opens the destination +
  `sqlite3_backup_init` synchronously (fast), then the actual page-copy loop
  (`BACKUP_POLL`, repeated `rate`-sized `sqlite3_backup_step` batches) runs
  on the **shared multi-thread tokio runtime** via `spawn_blocking` (mirrors
  the existing `thread.spawn_async_join` pattern) so large backups never
  block the JS thread. After each batch, the `progress` callback must run
  **on the JS thread**, not the background thread — hand a
  `(totalPages, remainingPages)` tuple back across the same kind of
  cross-thread task-queue bridge other callback-marshaling async ops already
  use (fs read callbacks, timers), then resolve/reject the returned
  `Promise` via the **Promise subsystem** once `BACKUP_FINISH` reports done
  or errored.

Both the shared tokio runtime and the Promise subsystem currently live in
`rts-std` (`async_rt`/`promise`) — `rts-node` cannot depend on `rts-std`, so
this is flagged as shared infra in §5.7, not assumed available.

### 5.4 Multithread / worker interaction

Per `docs/specs/rts-threading-model.md` (worker = RTS thread/region,
`MessagePort` = channel, `SharedArrayBuffer` = shared heap):

- A `DatabaseSync` wraps a raw `sqlite3*` connection handle. SQLite's default
  "serialized" build mode makes cross-thread use of the *same* connection
  memory-safe at the C level, but Node's own `node:sqlite` documents no
  worker_threads story and RTS should not invent implicit cross-thread
  sharing either — treat **one connection, one owning RTS thread/region** as
  the default contract, matching the `Entry`-per-thread-region ownership
  pattern used elsewhere (e.g. the `dgram` spec's socket precedent). A
  second worker wanting the same database file opens its **own**
  `DatabaseSync` against the same path; SQLite's file-level locking (§4)
  arbitrates concurrent access across those independent connections exactly
  as it would across independent OS processes.
- `owner_thread` tracked on `SqliteConnEntry` (§5.1) lets RTS optionally
  assert/diagnose an accidental cross-thread call rather than silently
  hitting an internal SQLite mutex stall.
- User-defined function/aggregate/authorizer callbacks (§5.3) always run on
  the same thread that issued the triggering `exec`/`prepare`/statement-step
  call — no cross-thread dispatch is ever needed for them.
- `backup()`'s background copy loop (§5.3) runs on a shared tokio worker
  thread, distinct from any specific worker-thread **region** — it holds no
  reference to JS-heap state beyond the two connection handles and the
  `progress` callback's cross-thread hand-off, so it does not need to know
  about the threading model's region/publication rules beyond "the
  `progress` `Function` handle must be safely invocable back on its
  originating region's thread."

### 5.5 Buffer / TypedArray interop

`BLOB` values cross the ABI as raw `(ptr: u64, len: i64)` pairs backed by the
primordial `ArrayBuffer`/`Buffer` memory model, exactly like the `dgram`
module's datagram payloads — never through `StrPtr`/the GC string pool
(BLOB bytes are not necessarily valid UTF-8). Binding a `TypedArray`/
`DataView` parameter (`STMT_BIND_BLOB`) reads its existing stable byte
pointer directly (bounds-checked against `byteLength`/`byteOffset` in the
`.ts` shim before the call). Reading a `BLOB` column
(`STMT_COLUMN_BLOB`) copies the bytes SQLite owns internally into a **freshly
allocated** `Buffer` handle sized to the actual blob length — SQLite's
`sqlite3_column_blob()` pointer is only valid until the next
`sqlite3_step()`/`sqlite3_reset()` on that statement, so it must never be
aliased directly into a long-lived JS-visible buffer. `Session.changeset()`/
`patchset()` similarly return a freshly-allocated `Buffer`-backed
`Uint8Array` (the changeset blob's lifetime is otherwise tied to
`sqlite3session_changeset()`'s internal allocation, which RTS must copy out
of and `sqlite3_free()`, not retain).

### 5.6 Doctrine placement

`node:sqlite` is unambiguously **non-primordial** — it has no native literal
syntax; every object is reached through an ordinary constructor call
(`new DatabaseSync(...)`) or method call, never a literal form. The engine
must never hardcode the strings `"sqlite"`, `"DatabaseSync"`,
`"StatementSync"`, `"Session"`, or `"SQLTagStore"` anywhere in
`crates/rts-codegen-new/`. Resolution path: `import ... from "node:sqlite"`
→ `ns_prefix_for("node:sqlite")` → `"node_sqlite"` → `node_lookup("node_sqlite.<member>")`
→ the matching `NodespaceMember` in `rts-node`'s `sqlite::SPEC`, registered
in `NODE_SPECS` — identical resolution shape to `node:fs`/`node:process`/
`node:dgram` today. This is the "registry for node:" data table, not a
codegen `match` arm; adding `sqlite` support means adding one new
`NodespaceSpec` entry, never touching engine control flow. All JS-facing
ergonomics (the four classes, tagged-template SQL text reconstruction,
named/positional parameter dispatch, row-object vs row-array assembly,
`SqlOutputValue` tagging, error-code → `Error` object mapping) live entirely
in a `.ts` shim shipped by `rts-node`; only the raw primitive ops in §5.2 are
native `extern "C"` symbols.

### 5.7 Shared-infra dependencies (FLAG)

- **Promise subsystem** (`rts-std::promise`/`PromiseSlot`). Needed for
  `sqlite.backup()`'s `Promise<number>` return — the module's only
  Promise-returning surface. Currently lives in `rts-std`, unreachable from
  `rts-node` without violating the no-`rts-std`-dependency rule; must be
  hoisted to `rts-engine` or a shared low crate (or `rts-node` implements a
  minimal Promise-settling path of its own against the primordial `Promise`
  constructor, if that is sufficient — worth deciding once a second
  Promise-returning node module exists, to avoid duplicate reimplementations).
- **Shared multi-thread tokio runtime** (`rts-std::runtime::async_rt::rt()`).
  Needed to run `backup()`'s page-copy loop off the JS thread via
  `spawn_blocking`. Same hoisting gap as above.
- **Cross-thread task/callback hand-off** (whatever mechanism currently
  delivers e.g. `fs` read callbacks or timer callbacks back onto the JS
  thread). Needed so `backup()`'s `progress` callback and the eventual
  Promise settlement run on the correct (originating) thread rather than the
  tokio worker thread that did the actual page copy.
- **`Function` handle synchronous-invoke trampoline** (the primordial
  `Function`/`Entry::Function` invoke mechanism). This one is **not** an
  `rts-std` dependency — `Function` is primordial (lives in/under
  `rts-primitives`/`rts-engine`), so `rts-node` can and should depend on it
  directly for user-defined-function/aggregate/authorizer callbacks (§5.3).
  Listed here only to make explicit that it is *needed* and *available*
  (not a gap), unlike the two bullets above.
- **Everything else in this module needs nothing from `rts-std`.** Connection
  open/close/pragma, prepared-statement execution, column/parameter
  marshaling, sessions/changesets, and the tag-store cache are 100%
  synchronous and self-contained inside `rts-node` + `libsqlite3-sys`.

### 5.8 Implementation phases

a. **Core connection + exec** — `OPEN`/`CLOSE`/`IS_OPEN`/`LOCATION`/`EXEC`/
   `LAST_ERROR_*` externs over `libsqlite3-sys` (bundled build, plain
   defines first — no column-metadata/session flags yet); `readOnly`/
   `timeout`/`enableForeignKeyConstraints` options. Enough for an
   open→`exec(CREATE TABLE...)`→close fixture.
b. **Prepared statements: write path** — `PREPARE`/`STMT_FINALIZE`/
   `STMT_RESET`/`STMT_CLEAR_BINDINGS`/`STMT_STEP`/`STMT_BIND_*`/
   `STMT_BIND_PARAMETER_{COUNT,NAME}`/`DB_LAST_INSERT_ROWID`/`DB_CHANGES` —
   enough for `statement.run()` end to end (anonymous params only).
c. **Prepared statements: read path** — `STMT_COLUMN_*` (type-tag + typed
   getters), row-object assembly in the `.ts` shim, `all`/`get`/`iterate`
   including the busy-cursor rule for `iterate()`; named-parameter binding
   (`allowBareNamedParameters`/`allowUnknownNamedParameters`); `readBigInts`/
   `returnArrays` (both DB-level default and per-statement override);
   `ERR_OUT_OF_RANGE` for unsafe integers.
d. **`build.rs` compile-flag wiring** — add `SQLITE_ENABLE_COLUMN_METADATA`
   to the vendored build; land `columns()`.
e. **User-defined functions** — `REGISTER_FUNCTION` + the synchronous
   JS-callback trampoline (reusing the `Function` invoke mechanism, §5.7);
   `deterministic`/`directOnly`/`useBigIntArguments`/`varargs` flags.
f. **Aggregates** — `REGISTER_AGGREGATE` (step/result, then inverse for
   window-function support); reuses the function trampoline's marshaling.
g. **Authorizer** — `SET_AUTHORIZER` + trampoline; wire
   `sqlite.constants`'s authorizer action/result codes.
h. **Config surface** — `limits` object (get/set + `Infinity` reset),
   `defensive`/`enableDoubleQuotedStringLiterals`/`allowExtension`+
   `loadExtension`/`enableLoadExtension`.
i. **Session extension** — add `SQLITE_ENABLE_SESSION`+
   `SQLITE_ENABLE_PREUPDATE_HOOK` to the vendored build;
   `SESSION_CREATE`/`_CHANGESET`/`_PATCHSET`/`_CLOSE`; `[Symbol.dispose]`.
j. **`applyChangeset`** — `xFilter`/`xConflict` callback trampolines,
   `sqlite.constants`'s changeset conflict-type/resolution codes.
k. **`SQLTagStore`** — pure `.ts` LRU wrapper over (b)/(c)'s
   `prepare()`/`all`/`get`/`iterate`/`run`; no new native surface.
l. **`backup()`** — `BACKUP_START`/`_POLL`/`_PROGRESS_*`/`_FINISH` +
   `spawn_blocking` loop + Promise settlement, once the §5.7 shared-infra
   gap (tokio runtime + Promise subsystem reachability) is resolved.

## 6. Test plan

1. **Basic open/exec/close** (`:memory:`): `CREATE TABLE` via `exec()`,
   assert `isOpen` transitions `true`→`false` across `close()`.
2. **Insert + query round trip**: `prepare('INSERT ...')` bound via
   anonymous params in a loop, then `prepare('SELECT ...').all()` returns
   the expected row objects in insertion order.
3. **Named parameters, bare and prefixed**: same query bound once with
   `{ id: 1 }` (bare) and once with `{ ':id': 1 }` (prefixed); both succeed
   with `allowBareNamedParameters` default `true`.
4. **Unknown named parameter**: throws by default; succeeds silently with
   `allowUnknownNamedParameters: true`.
5. **`get()` vs `all()` vs `iterate()`**: same query, assert `get()` returns
   only the first row, `all()` returns every row, `iterate()`'s `for...of`
   visits every row exactly once and the statement is reusable afterward.
6. **`run()`'s `RunResult`**: `INSERT` then assert `lastInsertRowid` matches
   a subsequent `SELECT last_insert_rowid()`, and `changes` matches an
   `UPDATE` affecting N rows.
7. **`readBigInts`**: a column value larger than
   `Number.MAX_SAFE_INTEGER` throws `ERR_OUT_OF_RANGE` by default and
   returns a correct `bigint` with `readBigInts: true` (both at the
   `DatabaseSync`-level default and via `statement.setReadBigInts(true)`).
8. **`returnArrays`**: same query returns `Row` as a positional array
   instead of a named-property object.
9. **BLOB round trip**: insert a `Uint8Array`, read it back via `get()`,
   assert byte-for-byte equality and that the returned value is a fresh
   `Uint8Array` (not aliased to the source array's buffer).
10. **NULL handling**: insert and read back `null` for every SQLite storage
    class column (`INTEGER`, `REAL`, `TEXT`, `BLOB` all nullable).
11. **`columns()` metadata**: assert `database`/`table`/`name`/`type` for a
    plain `SELECT col FROM t`, and `table === null` for a computed
    expression column (`SELECT 1+1`).
12. **Prepared-statement error surfaces `ERR_SQLITE_ERROR`**: a `UNIQUE`
    constraint violation on `run()`, a syntax error on `prepare()`.
13. **Iterator busy-statement rule**: start an `iterate()`, attempt `get()`
    on the same `StatementSync` before draining — expect a thrown error
    (document actual observed code even if it differs from the
    `(verify)`-tagged `ERR_INVALID_STATE` guess in §4).
14. **User-defined scalar function**: `database.function('double', x => x * 2)`,
    then `SELECT double(21)` returns `42`; assert the callback is invoked
    exactly once per row for a multi-row query.
15. **User-defined aggregate (sum)**: `database.aggregate('sumint', { start: 0, step: (a, v) => a + v })`,
    `SELECT sumint(col) FROM t` matches a hand-computed sum; a window-function
    variant with `inverse` over `OVER (ORDER BY ... ROWS BETWEEN ...)`.
16. **`setAuthorizer` deny**: an authorizer returning `SQLITE_DENY` for
    `SQLITE_DROP_TABLE` causes a subsequent `DROP TABLE` to fail to prepare;
    `SQLITE_IGNORE` on a `SELECT` column substitutes `NULL` for that column.
17. **`loadExtension` gating**: `loadExtension()` throws
    `ERR_LOAD_SQLITE_EXTENSION` when `allowExtension` was not set; succeeds
    (loading a trivial test extension `.so`/`.dll`) when it was.
18. **`limits` get/set/reset**: read a limit's current value, lower it,
    assert a query exceeding it fails appropriately, reset via `Infinity`.
19. **Session round trip**: `createSession()`, perform inserts/updates,
    `changeset()`/`patchset()` produce non-empty byte arrays; apply the
    changeset to a second, freshly-created database with the same schema
    via `applyChangeset()`, assert the target now matches the source.
20. **`applyChangeset` conflict handling**: pre-seed a conflicting row in
    the target database, apply a changeset that collides, assert the
    `onConflict` callback fires with the documented conflict-type constant
    and that returning `SQLITE_CHANGESET_OMIT`/`REPLACE`/`ABORT` each
    produce the expected end state.
21. **`SQLTagStore` basic usage**: `sql.all\`SELECT * FROM t WHERE id = ${id}\``
    returns the expected row(s); assert the interpolated value is bound as
    a parameter, not concatenated (attempt a SQL-injection-shaped string
    value and confirm it is treated as inert data).
22. **`SQLTagStore` LRU eviction**: `createTagStore(2)`, issue three distinct
    query shapes, assert `size` never exceeds `capacity` and the
    least-recently-used prepared statement is finalized on eviction.
23. **`backup()` happy path**: back up a populated `:memory:` database to a
    temp file path, assert the resolved page count is `> 0` and the
    resulting file, reopened as a fresh `DatabaseSync`, contains the same
    rows; assert `progress` is called at least once for a multi-batch
    (`rate: 1`) backup of a multi-page database.
24. **`[Symbol.dispose]` / `using`**: `using db = new DatabaseSync(...)`
    (and `using session = db.createSession()`) closes automatically at
    scope exit; calling `close()`/`[Symbol.dispose]()` again afterward is a
    silent no-op, not a throw.
25. **Multithread isolation**: two RTS worker threads each open their own
    `DatabaseSync` against the **same** on-disk file path concurrently;
    assert each connection's writes are eventually visible to the other
    (subject to SQLite's own locking/journal semantics) and neither process
    crashes nor corrupts the file under interleaved writes.
26. **`readOnly` enforcement**: opening an existing file with
    `readOnly: true` and attempting an `INSERT` throws `ERR_SQLITE_ERROR`
    (`SQLITE_READONLY`).
27. **`path` as `Buffer`/`URL`**: constructing with each of the three
    accepted `path` forms (`string`, `Buffer`, `file://` `URL`) opens the
    same on-disk file.

## 7. Open questions / deferrals

- **`libsqlite3-sys` build-flag wiring** for `SQLITE_ENABLE_COLUMN_METADATA`
  and `SQLITE_ENABLE_SESSION`/`SQLITE_ENABLE_PREUPDATE_HOOK` is proposed but
  unverified against the exact pinned crate version's `build.rs` knobs —
  `(verify)` at implementation time; worst case requires vendoring a custom
  `build.rs`/`cc` invocation rather than relying on the crate's built-in
  bundled-feature flag surface.
- **Iterator busy-statement error code**: §4/§6 flag `ERR_INVALID_STATE` as
  an educated guess for what happens when a second method is called on a
  `StatementSync` with an undrained `iterate()` cursor open; the exact Node
  error code was not confirmed against the fetched documentation and must be
  verified against real Node behavior (or the Node source) before RTS
  hardcodes a specific `.code` string into its own error.
- **`Session.close()` idempotency**: not explicitly documented as safe to
  call twice (unlike `[Symbol.dispose]`, which is documented as a no-op if
  already closed); RTS's `.ts` shim should make `close()` idempotent
  regardless, but this is a deliberate RTS-side robustness choice, not a
  confirmed Node-parity requirement — flag if a real-Node test later shows
  a double-`close()` throwing.
- **Promise subsystem / shared tokio runtime reachability from `rts-node`**
  (§5.7) is the one real architectural gap blocking `backup()` — everything
  else in this module is implementable with zero shared-infra dependencies.
  Until resolved, `backup()` could ship behind a temporary limitation (e.g.
  synchronous-blocking implementation with no true off-thread copy) as an
  explicitly-flagged interim, not a silent behavior change.
- **`node:sqlite` vs `worker_threads`**: Node's own documentation does not
  explicitly bless or forbid using a `DatabaseSync` across `worker_threads`;
  §5.4's "one connection, one owning thread/region" contract is RTS's own
  conservative default, not a directly-cited Node constraint — revisit if a
  cross-worker sharing pattern turns out to be common in real-world usage
  RTS needs to match.
- **`db.transaction(fn)`-style convenience helper**: not part of Node's
  documented `node:sqlite` surface (unlike `better-sqlite3`); out of scope
  for parity, but worth flagging as a plausible RTS-only `.ts`-shim
  ergonomic addition once the core surface lands, if the owner wants it.
- **Full-text search / R-Tree / JSON1 SQLite extensions**: not part of the
  documented `node:sqlite` JS surface at all (they're accessed via plain SQL
  once compiled in) — whether RTS's vendored build enables
  `SQLITE_ENABLE_FTS5`/`SQLITE_ENABLE_RTREE`/`SQLITE_ENABLE_JSON1` is a
  build-configuration decision independent of this spec's API surface, left
  to the implementer to decide against real-world schema needs.

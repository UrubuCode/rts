# node:url

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:url` |
| Node.js version | 25.x (`https://nodejs.org/docs/latest-v25.x/api/url.html`) |
| Stability | 2 - Stable (module overall); **legacy** `url.parse()` is Stability 0 - Deprecated / Legacy (deprecated since v11.0.0, "Legacy" since v15.13.0); **legacy** `url.format(urlString)` (single-string overload) and `url.resolve()` are Stability 0 - Deprecated (`url.resolve` re-deprecated as DEP0169 in v24.0.0, `url.format(urlString)` deprecated as DEP0149 in v24.0.0); `URLPattern` is Stability 1 - Experimental (added v23.8.0) |
| Tier | P0 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import url from 'node:url'`; `import { parse, format, resolve, domainToASCII, domainToUnicode, fileURLToPath, fileURLToPathBuffer, pathToFileURL, urlToHttpOptions, URL, URLSearchParams, URLPattern } from 'node:url'`; `const url = require('node:url')`; ambient globals `URL`, `URLSearchParams`, `URLPattern` usable with **no import** in any module (WHATWG/Web-standard globals, not Node-specific) |
| Globals exposed | `URL` (class, ambient, WHATWG URL Standard), `URLSearchParams` (class, ambient, WHATWG URL Standard), `URLPattern` (class, ambient, experimental, WICG URL Pattern spec) |

---

## 1. Purpose

`node:url` provides URL resolution and parsing. Its primary, standards-track
surface is the **WHATWG URL API** — the `URL` and `URLSearchParams` classes,
which are ambient globals in every JS/TS environment (Node, browsers, RTS)
and are also re-exported as named exports of the `node:url` module for
explicit-import style code. Layered on top of the WHATWG classes, `node:url`
adds a set of **Node-only utility functions** with no browser equivalent:
converting between `file://` URLs and platform file-system paths
(`fileURLToPath`/`fileURLToPathBuffer`/`pathToFileURL`), IDNA/Punycode domain
conversion (`domainToASCII`/`domainToUnicode`), building a Node HTTP
request-options object from a `URL` (`urlToHttpOptions`), and the **legacy**
pre-WHATWG API (`url.parse`/`url.format`/`url.resolve`) — a lenient,
non-standard string-splitting parser kept for backward compatibility only,
which Node explicitly warns is unsafe for security-sensitive input and which
new code should never use. `node:url` also hosts the experimental
`URLPattern` class (a router-style wildcard/pattern matcher over URL
components).

## 2. Exported API surface (COMPLETE)

### Classes

#### `URL` — WHATWG URL Standard (ambient global + named export)

```typescript
new URL(input: string | { toString(): string }, base?: string | URL): URL
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `input` | `string` (or any value with a `.toString()`/`Symbol.toPrimitive` coercion — **not** a raw `Buffer`/`Uint8Array`) | no | — |
| `base` | `string \| URL` | yes | — (if omitted, `input` must itself be an absolute URL) |

Throws: `TypeError` (`ERR_INVALID_URL`) if the resulting URL cannot be parsed
per the WHATWG URL parsing algorithm (invalid scheme, malformed host, etc.).

**Instance properties** (all are accessor getter/setter pairs on the
prototype, except `searchParams` and `origin`, which are getter-only):

| Property | Type | Notes |
|---|---|---|
| `url.href` | `string` | Full serialized URL. Setting it **re-parses** the entire URL (equivalent to `new URL(value)`, but mutates in place / throws `ERR_INVALID_URL` on failure and leaves the instance unchanged). |
| `url.protocol` | `string` | Scheme with trailing `:`, e.g. `'https:'`. Setter re-validates scheme-change legality (e.g. cannot change a special scheme to a non-special one in some cases per WHATWG). |
| `url.username` | `string` | Userinfo before `:`. Setter percent-encodes per the userinfo percent-encode set. |
| `url.password` | `string` | Userinfo after `:`. Same encoding rule as `username`. |
| `url.host` | `string` | `hostname` + `:` + `port` (port omitted if default/empty). Setter parses the combined form. |
| `url.hostname` | `string` | Host without port. Non-ASCII hostnames are Punycode-encoded (`xn--...`) per IDNA. |
| `url.port` | `string` | Numeric port as a string; empty string if the URL has no explicit port or the port equals the scheme's default port (setting the default port clears it back to `''`). |
| `url.pathname` | `string` | Path portion, percent-encoded per the path percent-encode set. |
| `url.search` | `string` | Query string **with** leading `?` (empty string if no query). Setting it re-parses `searchParams` from the new value. |
| `url.searchParams` | `URLSearchParams` (read-only) | Live view: mutating the returned `URLSearchParams` updates `url.search`/`url.href`, and vice versa. |
| `url.hash` | `string` | Fragment **with** leading `#` (empty string if none). |
| `url.origin` | `string` (read-only) | `protocol + '//' + host` for special schemes; `'null'` for opaque-origin schemes (e.g. `file:`, `blob:` without a nested origin — verify exact `file:` origin behavior, historically `'null'`). |

**Instance methods**

| Signature | Returns | Notes |
|---|---|---|
| `url.toString()` | `string` | Identical to reading `url.href`. |
| `url.toJSON()` | `string` | Identical to `url.toString()`/`url.href` — used automatically by `JSON.stringify(url)`. |

**Static methods**

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `URL.canParse(input, base?)` | `input: string`; `base?: string` | `boolean` — `true` iff `new URL(input, base)` would **not** throw | never throws | sync |
| `URL.parse(input, base?)` *(added v22.1.0)* | `input: string`; `base?: string` | `URL \| null` — same parse as the constructor but returns `null` on failure **instead of throwing** | never throws | sync |
| `URL.createObjectURL(blob)` *(stable v24.0.0 / v22.17.0)* | `blob: Blob` (from `node:buffer`, itself global) | `string` — a `blob:nodedata:...`-shaped opaque URL string | `TypeError` if `blob` is not a `Blob` | sync |
| `URL.revokeObjectURL(id)` *(stable v24.0.0 / v22.17.0)* | `id: string` | `void` | never throws (revoking an unregistered/already-revoked id is a silent no-op) | sync |

**Events:** none. `URL` is not an `EventEmitter`.

---

#### `URLSearchParams` — WHATWG URL Standard (ambient global + named export)

```typescript
new URLSearchParams(): URLSearchParams
new URLSearchParams(init: string): URLSearchParams
new URLSearchParams(init: Record<string, string | readonly string[]>): URLSearchParams
new URLSearchParams(init: Iterable<[string, string]> | Array<[string, string]>): URLSearchParams
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `init` | `string \| Record<string,string\|string[]> \| Iterable<[string,string]>` | yes | `''` (empty params) |

A leading `?` on a string `init` is stripped if present, but not required.
Throws: `TypeError` (`ERR_INVALID_TUPLE`, message *"Each query pair must be an
iterable [name, value] tuple"*) if `init` is an iterable whose elements are
not themselves 2-element iterables.

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `params.size` *(added v19.8.0/v18.16.0)* | `number` (read-only) | Total number of name/value **entries** (not unique names — a repeated key counts once per occurrence). |

**Instance methods**

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `params.append(name, value)` | `name: string`; `value: string` | `void` | `ERR_INVALID_THIS` if `this` is not a `URLSearchParams` | sync |
| `params.delete(name, value?)` | `name: string`; `value?: string` *(2nd-arg form added v20.2.0/v18.18.0)* | `void` | `ERR_INVALID_THIS` | sync |
| `params.entries()` | — | `IterableIterator<[string,string]>` | `ERR_INVALID_THIS` | sync |
| `params.forEach(fn, thisArg?)` | `fn: (value: string, name: string, searchParams: URLSearchParams) => void`; `thisArg?: unknown` | `void` | `ERR_INVALID_ARG_TYPE` if `fn` is not callable; `ERR_INVALID_THIS` | sync |
| `params.get(name)` | `name: string` | `string \| null` | `ERR_INVALID_THIS` | sync |
| `params.getAll(name)` | `name: string` | `string[]` (all values for repeated keys, insertion order) | `ERR_INVALID_THIS` | sync |
| `params.has(name, value?)` | `name: string`; `value?: string` *(2nd-arg form added v20.2.0/v18.18.0)* | `boolean` | `ERR_INVALID_THIS` | sync |
| `params.keys()` | — | `IterableIterator<string>` | `ERR_INVALID_THIS` | sync |
| `params.set(name, value)` | `name: string`; `value: string` | `void` (replaces **all** existing entries for `name` with a single entry) | `ERR_INVALID_THIS` | sync |
| `params.sort()` | — | `void` — stable sort of all entries by name (UTF-16 code-unit order); ties keep relative order | `ERR_INVALID_THIS` | sync |
| `params.toString()` | — | `string` — `application/x-www-form-urlencoded` serialization | `ERR_INVALID_THIS` | sync |
| `params.values()` | — | `IterableIterator<string>` | `ERR_INVALID_THIS` | sync |
| `params[Symbol.iterator]()` | — | `IterableIterator<[string,string]>` (identical to `entries()`; makes `for (const [k,v] of params)` and `[...params]` work) | `ERR_INVALID_THIS` | sync |

**Events:** none. `URLSearchParams` is not an `EventEmitter`.

---

#### `URLPattern` — WICG URL Pattern spec (experimental, Stability 1, added v23.8.0)

```typescript
new URLPattern(): URLPattern
new URLPattern(input: string, baseURL?: string, options?: URLPatternOptions): URLPattern
new URLPattern(input: URLPatternInit, baseURL?: string, options?: URLPatternOptions): URLPattern
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `input` | `string \| URLPatternInit` | yes | matches everything |
| `baseURL` | `string` | yes | — (only meaningful when `input` is a relative pattern string, or when `input.baseURL` is not set) |
| `options` | `URLPatternOptions` (see §3) | yes | `{ ignoreCase: false }` |

**Instance methods**

| Signature | Params | Returns | Throws | Variant |
|---|---|---|---|---|
| `pattern.exec(input, baseURL?)` | `input: string \| URLPatternInit`; `baseURL?: string` | `URLPatternResult \| null` (see §3) | `TypeError` on malformed `input` | sync |
| `pattern.test(input, baseURL?)` | `input: string \| URLPatternInit`; `baseURL?: string` | `boolean` | `TypeError` on malformed `input` | sync |

**Instance properties** (read-only, mirror the constructor's per-component
patterns): `pattern.protocol`, `pattern.username`, `pattern.password`,
`pattern.hostname`, `pattern.port`, `pattern.pathname`, `pattern.search`,
`pattern.hash`, all `string`; `pattern.hasRegExpGroups: boolean` (verify exact
property set against Node 25's shipped version — this class is explicitly
experimental and its surface has changed across versions).

**Events:** none.

---

### Top-level functions

All nine functions below are `url.<fn>` (or named imports). Every one is
**fully synchronous** — `node:url` has no callback- or Promise-returning
member anywhere.

#### `url.parse(urlString[, parseQueryString[, slashesDenoteHost]])`

**Status: Legacy / Deprecated (Stability 0).** Use `new URL()` instead.

| Param | Type | Optional | Default |
|---|---|---|---|
| `urlString` | `string` | no | — |
| `parseQueryString` | `boolean` | yes | `false` |
| `slashesDenoteHost` | `boolean` | yes | `false` |

Returns: `Url` (legacy plain object, see §3 `LegacyUrlObject`) — **never**
`null`; malformed input degrades to a best-effort partial object rather than
throwing, except for a small set of genuinely unparseable inputs, which throw
`URIError` (e.g. an `auth` component that fails percent-decoding). Throws:
`URIError` (rare, `auth` decode failure). Variant: sync.

If `parseQueryString` is `true`, `.query` is parsed into a
`Record<string,string|string[]>` (the same shape `querystring.parse()`
produces); otherwise `.query` is the raw, un-decoded query substring (no
leading `?`). If `slashesDenoteHost` is `true`, a URL of the form
`//foo/bar` is parsed with `foo` as the host; otherwise `foo` becomes part of
the path. Node's own docs call the underlying algorithm "lenient,
non-standard" and warn it is prone to host-name-spoofing-style security
issues — **no CVEs are issued** against `url.parse()` bugs precisely because
it is deprecated in favor of the WHATWG API.

#### `url.format(urlObjectOrString[, options])`

Three overloads:

```typescript
// 1. Format a legacy Url-shaped plain object (or url.parse()'s result)
url.format(urlObject: LegacyUrlObject | Partial<LegacyUrlObject>): string

// 2. Format a WHATWG URL instance, with formatting options
url.format(url: URL, options?: UrlFormatOptions): string

// 3. Format a URL string — DEPRECATED (DEP0149, v24.0.0)
url.format(urlString: string): string
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `urlObject` (overload 1) | `LegacyUrlObject`-shaped object (§3) | no | — |
| `url` (overload 2) | `URL` | no | — |
| `options` (overload 2) | `UrlFormatOptions` (§3) | yes | `{ auth: true, fragment: true, search: true, unicode: false }` |
| `urlString` (overload 3) | `string` | no | — |

Returns: `string`. Throws: `TypeError` (`ERR_INVALID_ARG_TYPE`) if the sole
argument is neither a plain object, a `URL` instance, nor a string. Variant:
sync.

Overload 1 re-serializes a legacy `Url`-shaped object per the composition
rules historically implemented by `Url.prototype.format` (never a real
exported class in modern Node — see §3). Overload 2 is the modern,
non-deprecated form for reformatting a WHATWG `URL` with selective component
suppression (`auth: false` drops `username:password@`, `fragment: false`
drops `#hash`, `search: false` drops `?query`, `unicode: true` converts
Punycode (`xn--`) hostnames back to Unicode for display). Overload 3
(deprecated, DEP0149) is functionally `new URL(urlString).toString()` with
looser/legacy parsing tolerance and is slated for eventual removal.

#### `url.resolve(from, to)`

**Status: Deprecated (Stability 0), re-deprecated as DEP0169 in v24.0.0.**
Use `new URL(to, from)` instead.

| Param | Type | Optional |
|---|---|---|
| `from` | `string` (base URL) | no |
| `to` | `string` (URL to resolve against `from`) | no |

Returns: `string` — `to` resolved against `from` per the legacy resolution
algorithm (Node's own docs give the canonical example:
`url.resolve('/one/two/three', 'four')` → `'/one/two/four'`;
`url.resolve('http://example.com/', '/one')` → `'http://example.com/one'`;
`url.resolve('http://example.com/one', '/two')` → `'http://example.com/two'`).
Throws: none documented (degrades gracefully like `url.parse`). Variant: sync.
**Not** byte-identical to `new URL(to, from).toString()` in every edge case —
the legacy algorithm has its own quirks; Node's docs explicitly recommend
migrating rather than treating the two as interchangeable.

#### `url.domainToASCII(domain)`

**Added:** v7.4.0. ICU requirement removed in v20.0.0 (Node now ships its own
lightweight IDNA implementation, no longer requiring a full-ICU build).

| Param | Type | Optional |
|---|---|---|
| `domain` | `string` | no |

Returns: `string` — the Punycode ASCII serialization of `domain`
(`'xn--...'` per label as needed), or an **empty string** if `domain` is
invalid. Throws: none documented (invalid input → `''`, not an exception).
Variant: sync.

Example: `url.domainToASCII('español.com')` → `'xn--espaol-zwa.com'`;
`url.domainToASCII('中文.com')` → `'xn--fiq228c.com'`;
`url.domainToASCII('xn--iñvalid.com')` → `''` (verify exact invalid-input
example against live Node — general contract is "invalid → empty string").

#### `url.domainToUnicode(domain)`

**Added:** v7.4.0. ICU requirement removed in v20.0.0.

| Param | Type | Optional |
|---|---|---|
| `domain` | `string` | no |

Returns: `string` — the Unicode serialization of `domain` (decoding any
`xn--` labels), or an empty string if `domain` is invalid. Throws: none
documented. Variant: sync.

Example: `url.domainToUnicode('xn--espaol-zwa.com')` → `'español.com'`;
`url.domainToUnicode('xn--iñvalid.com')` → `''`.

#### `url.fileURLToPath(url[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `url` | `URL \| string` (a `file:` URL or its string form) | no | — |
| `options` | `{ windows?: boolean }` *(option added v22.1.0)* | yes | `undefined` (use the RTS **compile target's** OS flavor) |

Returns: `string` — the platform file-system path. Throws: `TypeError`
(`ERR_INVALID_URL_SCHEME`, *"The URL must be of scheme file"*) if the URL's
`protocol` is not `'file:'`; `TypeError` (`ERR_INVALID_FILE_URL_HOST`) on
POSIX/`windows: false` if the URL has a non-empty, non-`localhost` host
(file URLs on POSIX must be host-less); `TypeError`
(`ERR_INVALID_FILE_URL_PATH`) on Windows/`windows: true` for a malformed
drive-letter path. Variant: sync.

`options.windows`: `true` forces Windows path semantics (backslash
separators, drive letters, UNC), `false` forces POSIX semantics, `undefined`
(default) uses the host/target OS the running binary was built for. Examples
(POSIX): `fileURLToPath('file:///你好.txt')` → `'/你好.txt'`;
`fileURLToPath('file:///hello world')` → `'/hello world'`;
`fileURLToPath('file:///foo%231')` → `'/foo#1'`. Examples (Windows):
`fileURLToPath('file:///C:/path/')` → `'C:\\path\\'`;
`fileURLToPath('file://nas/foo.txt')` → `'\\\\nas\\foo.txt'` (UNC).

**Security note (quoted from Node's docs):** *"This function decodes
percent-encoded characters, including encoded dot-segments (`%2e` as `.` and
`%2e%2e` as `..`), and then normalizes the resulting path... This means that
encoded directory traversal sequences (such as `%2e%2e`) are decoded and
processed as actual path traversal sequences... Applications must not rely on
`fileURLToPath()` alone to prevent directory traversal attacks."*

#### `url.fileURLToPathBuffer(url[, options])`

Same parameters as `fileURLToPath` (see above). Returns: `Buffer` — the raw
percent-decoded path **bytes**, with no UTF-8 validity requirement. Same
throw conditions as `fileURLToPath`. Variant: sync.

Exists because a `file:` URL's percent-encoded path segments are not
guaranteed to decode to valid UTF-8 on POSIX filesystems (which accept
arbitrary byte sequences as file names); `fileURLToPath`'s `string` return
would either lossily replace or throw on such bytes, while
`fileURLToPathBuffer` preserves them exactly.

#### `url.pathToFileURL(path[, options])`

| Param | Type | Optional | Default |
|---|---|---|---|
| `path` | `string` | no | — |
| `options` | `{ windows?: boolean }` *(added v22.1.0)* | yes | `undefined` (compile-target OS) |

Returns: `URL` — a `file:` URL object. Throws: none documented beyond generic
`TypeError` for a non-string `path`. Variant: sync.

Percent-encodes characters that would otherwise be structurally significant
in a URL but are legal in a path (`#` → `%23`, `%` → `%25`, plus whatever the
WHATWG path percent-encode set additionally requires once the string is fed
through URL construction). Examples (POSIX): `pathToFileURL('/foo#1')` →
`'file:///foo%231'`; `pathToFileURL('/some/path%.c')` →
`'file:///some/path%25.c'`. Examples (Windows):
`pathToFileURL('C:\\path\\')` → `'file:///C:/path/'` (backslashes converted
to forward slashes).

#### `url.urlToHttpOptions(url)`

**Added:** v15.7.0. Enhanced in v19.9.0 to copy every enumerable own
property of `url`, not just the fixed set below.

| Param | Type | Optional |
|---|---|---|
| `url` | `URL` | no |

Returns: `HttpOptions` (see §3) — an object shaped for direct use as
`http.request(options)`/`https.request(options)`'s first argument:
`{ protocol, hostname, hash, search, pathname, path, href, port: number,
auth, ...url's own enumerable properties }`. Throws: none documented.
Variant: sync.

Example: `urlToHttpOptions(new URL('https://a:b@測試?abc#foo'))` →
`{ protocol: 'https:', hostname: 'xn--g6w251d', hash: '#foo', search: '?abc',
pathname: '/', path: '/?abc', href: 'https://a:b@xn--g6w251d/?abc#foo',
port: undefined, auth: 'a:b' }` (verify exact `port` representation —
Node's docs show it omitted/`undefined` when absent rather than `NaN`/`''`).

### Properties & constants

None documented at the `node:url` module level. (There is **no**
`url.constants` object in current Node — an old, long-removed feature; do
not resurrect it.) All constant-like values live on the classes themselves
(none currently — `URL`/`URLSearchParams` expose no static data constants,
only the methods in the Classes section above).

### Events

None. Nothing in `node:url` extends `EventEmitter`.

## 3. Types & option objects

```typescript
/** Returned by the legacy url.parse(); the historical (non-exported) `Url` shape. */
interface LegacyUrlObject {
  /** userinfo "user:pass" portion, or null if absent. */
  auth: string | null;
  /** fragment with leading '#', or null. */
  hash: string | null;
  /** host:port (lowercased), or null. */
  host: string | null;
  /** host without port (lowercased), or null. */
  hostname: string | null;
  /** full URL string (lowercased protocol/host). */
  href: string;
  /** pathname + search concatenated. */
  path: string | null;
  /** path portion only, NOT percent-decoded. */
  pathname: string | null;
  /** numeric port as a string, or null. */
  port: string | null;
  /** scheme with trailing ':', lowercased, or null. */
  protocol: string | null;
  /** raw query string (no '?') if parseQueryString=false, or a parsed
   *  Record if parseQueryString=true; null if no query. */
  query: string | ParsedUrlQuery | null;
  /** query string WITH leading '?', or null. */
  search: string | null;
  /** true if the protocol is followed by '//'. */
  slashes: boolean | null;
}

/** Shape produced by querystring.parse() / url.parse(s, true).query */
type ParsedUrlQuery = Record<string, string | string[] | undefined>;

/** url.format()'s WHATWG-URL overload options. */
interface UrlFormatOptions {
  /** include username:password@ (default true). */
  auth?: boolean;
  /** include #hash (default true). */
  fragment?: boolean;
  /** include ?search (default true). */
  search?: boolean;
  /** decode Punycode hostname labels back to Unicode for display (default false). */
  unicode?: boolean;
}

/** Shared by fileURLToPath / fileURLToPathBuffer / pathToFileURL. */
interface FileUrlPathOptions {
  /** true = force Windows semantics; false = force POSIX; undefined = compile-target OS. */
  windows?: boolean | undefined;
}

/** Returned by urlToHttpOptions(). */
interface HttpOptions {
  protocol: string;
  hostname: string;
  hash: string;
  search: string;
  pathname: string;
  /** pathname + search. */
  path: string;
  href: string;
  /** numeric port, or undefined if the URL has no explicit/non-default port. */
  port: number | undefined;
  /** "user:pass", or undefined if absent. */
  auth: string | undefined;
  /** every other enumerable own property copied from the URL instance (v19.9.0+). */
  [key: string]: unknown;
}

/** URLPattern constructor's structured-input form and per-component patterns. */
interface URLPatternInit {
  protocol?: string;
  username?: string;
  password?: string;
  hostname?: string;
  port?: string;
  pathname?: string;
  search?: string;
  hash?: string;
  baseURL?: string;
}

interface URLPatternOptions {
  /** case-insensitive matching (default false). */
  ignoreCase?: boolean;
}

/** Per-component match detail (verify exact field set against Node 25). */
interface URLPatternComponentResult {
  input: string;
  groups: Record<string, string | undefined>;
}

interface URLPatternResult {
  /** the raw input(s) tested, echoed back. */
  inputs: [string | URLPatternInit, string?];
  protocol: URLPatternComponentResult;
  username: URLPatternComponentResult;
  password: URLPatternComponentResult;
  hostname: URLPatternComponentResult;
  port: URLPatternComponentResult;
  pathname: URLPatternComponentResult;
  search: URLPatternComponentResult;
  hash: URLPatternComponentResult;
}

/** Error shape thrown across this module's TypeError-producing paths. */
interface NodeUrlError extends Error {
  code:
    | 'ERR_INVALID_URL'
    | 'ERR_INVALID_URL_SCHEME'
    | 'ERR_INVALID_FILE_URL_HOST'
    | 'ERR_INVALID_FILE_URL_PATH'
    | 'ERR_INVALID_THIS'
    | 'ERR_INVALID_TUPLE'
    | 'ERR_INVALID_ARG_TYPE'
    | 'ERR_INVALID_ARG_VALUE'
    | 'ERR_MISSING_ARGS';
  input?: string;
}
```

## 4. Node semantics & edge cases

- **Percent-encoding is context-dependent (4 encode sets, WHATWG spec).**
  From narrowest to widest: **C0 control set** (`U+0000`–`U+001F` and
  `>U+007E`) ⊂ **fragment set** (C0 + SPACE, `"`, `<`, `>`, `` ` ``) ⊂
  **path set** (fragment + `#`, `?`, `{`, `}`) ⊂ **userinfo set** (path +
  `/`, `:`, `;`, `=`, `@`, `[`–`^`, `|`). Each URL component is encoded with
  the narrowest set applicable to it; getting this wrong (encoding a
  hostname with the userinfo set, say) produces observably wrong `.href`
  output.
- **IDNA / Punycode hostnames.** Non-ASCII hostnames are converted to
  Punycode `xn--` labels at parse time: `new URL('https://測試').href` →
  `'https://xn--g6w251d/'`. Mixed percent-encoding and Punycode can occur in
  the *input* even though the *output* is pure Punycode:
  `new URL('https://%CF%80.example.com/foo').href` →
  `'https://xn--1xa.example.com/foo'` (the percent-encoded `π` character is
  first percent-decoded, then Punycode-encoded).
- **The `~` (tilde) encoding asymmetry is a documented, deliberate
  divergence** between the two encode paths: `new URL('https://example.org/abc?foo=~bar').search`
  is `'?foo=~bar'` (tilde **not** encoded — `search`'s raw-string setter
  path is more permissive), but re-deriving the same query through
  `searchParams` re-encodes it: `myURL.searchParams.sort(); myURL.search`
  → `'?foo=%7Ebar'` (tilde **is** encoded — `URLSearchParams`'s
  `application/x-www-form-urlencoded` serialization is stricter). Both are
  spec-correct; they are simply two different serialization algorithms
  applied to the same conceptual data.
- **`new URL()` does not accept raw bytes.** The constructor takes a
  `string` (or a value coercible via `.toString()`); it explicitly does
  **not** accept a `Buffer`/`Uint8Array`/`ArrayBuffer` as `input` or `base`.
- **Legacy `url.parse()` is a security footgun by design-intent, not bug:**
  Node's own docs state it uses "a lenient, non-standard algorithm for
  parsing URL strings" that is "prone to security issues such as host name
  spoofing and incorrect handling of usernames and passwords" — and
  explicitly: **no CVEs are issued for `url.parse()` vulnerabilities**,
  because the fix is "migrate to the WHATWG URL API," not a patch. RTS
  should preserve this framing (implement it faithfully for compatibility,
  but do not attempt to "fix" its leniency — that would itself be a parity
  break).
- **`fileURLToPath`/`fileURLToPathBuffer` directory-traversal note**
  (security-relevant, quoted above in §2): percent-decoding
  `%2e`/`%2e%2e` and then normalizing means a crafted `file:` URL can
  decode+normalize into a path that escapes an intended directory; callers
  needing traversal protection must additionally validate the **resolved**
  output, not just trust that `fileURLToPath` "sanitizes" it.
- **Windows vs POSIX for the three path-conversion functions:**
  - POSIX `fileURLToPath` decodes bytes as-is with no drive-letter/UNC
    concept; a non-empty, non-`localhost` `host` component is an error
    (`ERR_INVALID_FILE_URL_HOST`) since POSIX file URLs have no host.
  - Windows `fileURLToPath` recognizes `file:///C:/...` (drive-absolute,
    decodes to `C:\...`) and `file://server/share/...` (UNC, decodes to
    `\\server\share\...`, **host becomes part of the resulting path** rather
    than being rejected).
  - `pathToFileURL` mirrors this in reverse: POSIX absolute paths become
    `file:///abs/path`; Windows paths get backslashes converted to forward
    slashes and drive letters preserved (`C:\path\` → `file:///C:/path/`);
    a Windows UNC path becomes a `file://server/share/...` URL **with** a
    host component.
  - The `options.windows` override (v22.1.0+) lets any of the three
    functions be forced into the *other* platform's semantics regardless of
    the host/compile-target OS — needed for cross-platform tooling (e.g. a
    Linux build script computing a Windows-style `file:` URL for output
    consumed on Windows).
- **Deprecations, precisely:**
  - `url.resolve()` — **DEP0169**, re-flagged Deprecated in v24.0.0 (it had
    a long history of being merely "legacy-not-deprecated" before that);
    replacement is `new URL(to, from)`.
  - `url.format(urlString)` (the single-string overload only) — **DEP0149**,
    deprecated in v24.0.0; the object-based and `URL`-based overloads of
    `url.format()` are **not** deprecated.
  - `url.parse()` — deprecated since v11.0.0, downgraded to "Legacy"
    stability (still supported indefinitely, but not recommended for new
    code) since v15.13.0; no single dedicated `DEP0xxx` number surfaced in
    the fetched docs for this one specifically — `(verify)` if an exact
    number is needed for a deprecation-warning message.
- **Blob object-URL registration is thread-local and manual-lifetime.**
  `URL.createObjectURL(blob)` registers `blob` in a table scoped to the
  **current thread only**; `worker_threads` workers cannot resolve an
  object URL created by a different worker or the main thread. The
  registered data is retained until `URL.revokeObjectURL(id)` is explicitly
  called (no automatic GC of the registration itself, independent of the
  underlying `Blob`'s own memory lifetime) — revoking an unregistered id is
  a silent no-op, never an error.
- **`URLSearchParams` iteration order is insertion order**, and remains
  insertion order after `append`/`delete`/`set` mutations (per-key
  `delete`/`set` do not "compact and reinsert at the end"); `sort()` is the
  only operation that reorders entries, and it is a **stable** sort by name.
- **No `url.constants`.** A `require('url').constants` object existed in
  ancient Node (pre-v7) and is fully gone; do not implement it.
- **No backpressure/streaming concerns anywhere in this module** — every
  operation is a single, synchronous, in-memory computation.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:url`'s surface splits cleanly into three provenance groups, and the
key architectural insight is that **none of them require rts-node to gain a
new Rust-level dependency on another crate** — every cross-reference below
is either (a) an *already-ambient global* the generated `.ts` can call
exactly like user code would, or (b) an *in-crate* `.ts`-to-`.ts` or
Rust-to-Rust reuse of another `rts-node` module that already exists as a
sibling source file in the same crate.

1. **`URL` / `URLSearchParams` (and `URLPattern`) are NOT re-implemented by
   this module.** They already exist today as ambient global classes
   implemented in `rts-shared/src/globals/url/` (`register_url_class_spec`/
   `register_urlsp_class_spec`, a hand-written `Engine`/`Member`/`Sig`
   Registry-class registration — **not** an external crate, a bespoke
   parser over `rts_engine::heap::handles`). `node:url`'s job for these
   three classes is a **pure re-export bridge**: the module's `.ts` entry
   point does `export { URL, URLSearchParams, URLPattern };` binding the
   module's named exports to the *same* ambient global identifiers every
   other TS file already resolves through the engine's
   `global_class_lookup`. No new native code, no new `Handle` table, no
   rts-node → rts-shared Rust dependency is introduced — the reference is
   purely at the generated-program level (identifier resolution), which is
   exactly how `node:buffer`'s `Buffer`/`Blob`/`File` re-export already
   works (see `buffer.md` §5.6).
   - **Important, pre-existing parity gaps in the current `rts-shared` URL
     implementation** that block this module from being fully Node-25
     conformant (not this module's own Rust code to fix, but a hard
     dependency of `node:url`'s completeness — tracked in §7): no `URL`
     property **setters** are wired into `register_url_class_spec` (only
     getters — even `__RTS_FN_GL_URL_SET_PATHNAME` already exists as an
     extern in `instance.rs` but is **not** registered as a class member,
     i.e. it is dead code from the JS side today); no `URL.parse()` static
     (returns-null-instead-of-throw variant); no
     `URL.createObjectURL`/`revokeObjectURL`; `URLSearchParams.append()` is
     literally aliased to `set()` in the current implementation (loses
     repeated-key/multi-value semantics entirely — a real correctness bug,
     not just a missing feature); no `.size`; no `entries()`/`forEach()`/
     `[Symbol.iterator]()` (only `keys()`/`values()` returning plain arrays,
     not paired iteration); no `delete(name, value)`/`has(name, value)`
     2-arg overloads; and the hostname parser does **no IDNA/Punycode
     conversion at all** (confirmed by inspection — no `xn--`/punycode
     logic anywhere in `globals/url/instance.rs`), so `new URL('https://測試').hostname`
     does not currently produce `'xn--g6w251d'` the way Node requires.
2. **The legacy `url.parse`/`url.format`/`url.resolve` algorithm is pure
   `.ts`.** It is a lenient, ad hoc string-splitting/regex state machine
   (historically pure JS in Node itself too) — fully expressible using only
   the primordial `String`/`RegExp`/`Object` operations already available
   to any TS program. No native extern is needed anywhere in this group;
   `decodeURIComponent` (an existing ambient global, primordial-adjacent
   per the `global_this` namespace) supplies the `auth`-decode step,
   including its documented `URIError`-on-malformed-input behavior, for
   free.
3. **`domainToASCII`/`domainToUnicode` reuse `node:punycode`'s native
   Bootstring core in-crate.** Per `docs/node-implementation/punycode.md`
   §5.1/§5.2, `rts-node` already plans two tiny native externs,
   `__RTS_FN_NODE_PUNYCODE_ENCODE`/`_DECODE` (RFC 3492 Bootstring), plus a
   `.ts` `toASCII`/`toUnicode` domain-splitting wrapper around them. This
   module's `domainToASCII`/`domainToUnicode` are thin `.ts` wrappers that
   call into that **same in-crate** `.ts` helper (either via the internal
   shim file directly, or via the public `node:punycode` specifier) —
   zero new native symbols. Node's own docs note `url.domainToASCII` uses
   "a slightly different algorithm" than the bare Punycode `toASCII` (the
   full WHATWG host-parsing IDNA/UTS46 algorithm additionally does
   Unicode normalization and a disallowed-character/mapping table pass
   that plain Bootstring does not) — flagged `(verify)` in §7 as a
   precision gap between "reuse punycode's toASCII verbatim" and "a fuller
   UTS46 pass," to be resolved during implementation against real Node
   output on non-trivial inputs (mixed-script labels, disallowed
   codepoints).
4. **`fileURLToPath`/`fileURLToPathBuffer`/`pathToFileURL` are pure `.ts`
   plus in-crate reuse of `node:path`.** Percent-decoding an already-ASCII
   WHATWG-serialized pathname into raw bytes is a trivial `.ts` loop (scan
   for `%XX` triplets, `parseInt(hex, 16)`, push into a growable byte
   array, else push the char code — every byte in a serialized pathname is
   guaranteed ASCII by construction) requiring **no native primitive at
   all**; the resulting bytes are wrapped in a primordial
   `Uint8Array`/`Buffer` for `fileURLToPathBuffer`, or decoded via the
   already-ambient global `TextDecoder` for `fileURLToPath`'s `string`
   return. Percent-**encoding** specific characters for `pathToFileURL`
   (`#` → `%23`, `%` → `%25`) is likewise a trivial `.ts` string replace;
   the final `file://...` string is then simply passed to `new URL(...)`
   (again, an ambient-global call, zero rts-node Rust code), which supplies
   the rest of the WHATWG percent-encoding for free. The **platform-flavor
   decision** (`options.windows` default / drive-letter / UNC handling /
   dot-segment normalization) reuses `node:path`'s already-planned
   `__RTS_FN_NODE_PATH_IS_WIN32` constant and its `.ts` `normalize()`/
   `win32`/`posix` implementations **in-crate** (a same-crate `.ts` import,
   `rts-node/src/path/path.ts`, from `rts-node/src/url/url.ts`) — no new
   native symbol, no new Rust dependency.
5. **`urlToHttpOptions`** is pure `.ts`: it reads the (ambient-global)
   `URL` instance's own getters and assembles a plain object literal. No
   native call.
6. **Net result: this module introduces zero new `__RTS_FN_NODE_URL_*`
   native externs of its own.** Every genuinely native primitive it touches
   (Punycode Bootstring, `IS_WIN32`, GC string/byte allocation) already
   exists or is already planned in a sibling `rts-node` module or in the
   `rts-engine` base layer. This mirrors `node:path`'s "almost entirely
   `.ts`" story (see `path.md` §5.1) and goes one step further: `node:path`
   still needed 3 tiny native symbols, `node:url` needs none beyond what's
   already committed elsewhere in the crate.

### 5.2 ABI surface

**No new `__RTS_FN_NODE_URL_*` symbols are required** (see §5.1 point 6) —
this is the notable, deliberate finding for this module, not an omission.
For completeness, the externs this module's `.ts` layer **calls into**
(owned by other specs/crates, listed here only as the dependency edge):

| Symbol | Owner | Args (`AbiType`) | Returns | Used for |
|---|---|---|---|---|
| `__RTS_FN_NODE_PUNYCODE_ENCODE` | `node:punycode` (in-crate) | `StrPtr` | `StrPtr` | `domainToASCII` |
| `__RTS_FN_NODE_PUNYCODE_DECODE` | `node:punycode` (in-crate) | `StrPtr` | `StrPtr` | `domainToUnicode` |
| `__RTS_FN_NODE_PATH_IS_WIN32` | `node:path` (in-crate) | (none) | `Bool` | `fileURLToPath`/`fileURLToPathBuffer`/`pathToFileURL`'s default platform flavor |
| `__RTS_FN_GL_URL_NEW` / `__RTS_FN_GL_URL_NEW_WITH_BASE` | ambient `URL` global (`rts-shared`) | `StrPtr[, StrPtr]` | `Handle` | invoked indirectly via `.ts`'s `new URL(...)` call in `pathToFileURL`; **not** a direct rts-node → rts-shared Rust call, purely a generated-program-level identifier resolution (see §5.1 point 1) |

`Handle`-table usage: **none new.** `URL`/`URLSearchParams`/`URLPattern`
instances live in the engine's existing shared `HandleTable`
(`rts_engine::heap::handles`, via `rts-shared`'s registration) exactly like
every other GC-tracked object; `fileURLToPathBuffer`'s return is a
primordial `Buffer`/`Uint8Array` (also the existing engine `ArrayBuffer`
`Handle` machinery, per `buffer.md` §5.1) — `node:url` allocates no handle
table of its own.

`.ts` shim vs native extern split: **100% `.ts`** for this module's own
code (`rts-node/src/url/url.ts`), calling three kinds of things: (1) already
-ambient globals (`URL`, `URLSearchParams`, `TextDecoder`,
`decodeURIComponent`, `RegExp`, `Uint8Array`), (2) sibling in-crate `.ts`
helpers (`node_path`'s `normalize`/`win32`/`posix`/`isWin32`,
`node_punycode`'s `toASCII`/`toUnicode`/`encode`/`decode`), and (3)
primordial operators (string slicing, array building). No item in this
module's own surface needs a dedicated new native symbol.

### 5.3 Async model

Entirely synchronous. Every one of the 9 top-level functions, and every
method on `URL`/`URLSearchParams`/`URLPattern`, is `variant: sync` per §2 —
`node:url` has no callback-taking or Promise-returning member anywhere
(unlike, say, `node:buffer`'s `Blob.arrayBuffer()`/`bytes()`/`text()`). No
interaction with the RTS event loop, the Promise subsystem, or the shared
tokio runtime is needed for any part of this module.

### 5.4 Multithread / worker interaction

- **`URL`/`URLSearchParams`/`URLPattern` instances are ordinary GC handles**
  in the engine's shared `HandleTable`, following the standard RTS
  threading model (`docs/specs/rts-threading-model.md`: per-thread regions
  + shared heap with promotion-on-publication) exactly like any other
  heap object — a `URL` created on one thread lives in that thread's
  region until published/shared, with no module-specific multithread
  behavior to design here.
- **Every top-level function in this module is a pure function of its
  arguments** (`domainToASCII`/`domainToUnicode`/`fileURLToPath`/
  `fileURLToPathBuffer`/`pathToFileURL`/`urlToHttpOptions`/legacy
  `parse`/`format`/`resolve`) — no shared mutable module-level state, so
  every one of them is safely callable concurrently from any number of
  RTS threads/worker regions with zero locking, mirroring `node:path` and
  `node:punycode`'s "fully stateless" story (see their §5.4 sections).
  The sole exception is `fileURLToPath(Buffer)`/`pathToFileURL`'s
  dependence on the process's/target's OS-flavor constant
  (`IS_WIN32`), which is a read-only, compile-time-fixed value — not
  mutable per-thread state, so it needs no `threadLocal`/`shared`
  classification either.
- **`URL.createObjectURL`/`revokeObjectURL`'s registry is thread-local BY
  NODE'S OWN DESIGN**, not an RTS threading-model gap to close: Node's docs
  explicitly state object-URL registrations are "registered within the
  current thread" and inaccessible from other `worker_threads` workers.
  RTS should implement this registry as a `thread_local!` (per the
  `02-runtime.md` "pattern for thread-local caches"), which is a **correct
  parity implementation**, not a limitation to work around — do not
  attempt to make it cross-thread-visible, that would be a Node
  incompatibility.
- **`Blob` cross-reference:** `URL.createObjectURL(blob)` accepts a `Blob`
  handle, and `Blob` is specified as an `rts-node`-owned type living in
  `node:buffer`'s own handle table (`buffer.md` §5.1/§5.2) — but `URL`
  itself is implemented in `rts-shared`, a **sibling** crate to
  `rts-node`, not a dependent of it. This is the *exact same* cross-module
  design tension already flagged from the other direction in `buffer.md`
  §5.7 (`buffer.resolveObjectURL` needing a registry `URL.createObjectURL`
  populates) — restated here from the `URL`-owning side. Neither this spec
  nor `buffer.md` resolves it; see §7.

### 5.5 Buffer / TypedArray interop

- **`fileURLToPathBuffer`** is the only member of this module that produces
  byte data: it returns a `Buffer` (primordial `Uint8Array` subclass, per
  the engine's TypedArray/`ArrayBuffer` handle machinery) built from raw,
  UTF-8-unvalidated percent-decoded bytes — see §5.1 point 4 for the
  decode path.
- **Nothing in this module accepts `Buffer`/`TypedArray`/`ArrayBuffer` as
  input.** In particular, `new URL(input, base)` explicitly does **not**
  accept a `Buffer`/`Uint8Array` for `input` (confirmed in §4) — passing
  one must produce the same `TypeError`/coercion behavior real Node does
  (attempting `.toString()` on it, which for a `Buffer` yields a
  UTF-8-decoded string, not a `ERR_INVALID_ARG_TYPE`-style rejection —
  `(verify)` exact behavior against live Node before finalizing).
- Every other function/method in this module operates exclusively on JS
  `string` values.

### 5.6 Doctrine placement

- **Non-primordial, confirmed** for the entire `node:url`-specific surface
  (`parse`/`format`/`resolve`/`domainToASCII`/`domainToUnicode`/
  `fileURLToPath`/`fileURLToPathBuffer`/`pathToFileURL`/`urlToHttpOptions`):
  none of these have a native literal/syntactic form, so the engine
  (`rts-codegen-new`) must never hardcode any of their names. Resolution
  is the standard rts-node data-table mechanism: `import ... from
  'node:url'` resolves through `rts_node::ns_prefix_for("node:url")` →
  `"node_url"` against the `NODE_SPECS` table
  (`crates/rts-node/src/lib.rs`), and (for the rare case this module ever
  needs a real native call beyond what §5.2 already covers) each qualified
  call resolves via `rts_node::node_lookup("node_url.<name>")` — a plain
  data lookup, zero hardcoded arm in codegen, identical in shape to every
  other `node:*` module already in `rts-node`.
- **`URL`/`URLSearchParams`/`URLPattern` are also non-primordial** — per the
  "dividing line is native syntax" doctrine (CLAUDE.md), they have no
  literal form (`new URL(...)` is a call, not syntax like `/re/` is for
  `RegExp`), so they are Registry-only, exactly as `rts-shared`'s existing
  `globals/url/mod.rs` already implements them (`e.class("URL")`/
  `e.class("URLSearchParams")` — ordinary `Engine`/`Member` Registry
  registration, no engine-side hardcoded name). `node:url`'s own `.ts` re-
  export introduces no new engine-side special case either — it is
  ordinary TS `export { URL, URLSearchParams, URLPattern }` syntax over
  identifiers the engine already resolves generically.
- **Global injection without hardcoding**, mirroring `buffer.md`'s
  identical pattern (CLAUDE.md ANTI-HARDCODE §3): `URL`/`URLSearchParams`/
  `URLPattern` are unconditionally-included ambient globals (a `.ts`
  prelude, or already covered by whatever prelude mechanism currently
  wires the `rts-shared` URL/URLSearchParams globals into every program's
  scope today — verify whether that wiring already exists prelude-side, or
  whether it is itself a gap this doc's §7 should also flag).
- **Where the `.ts` lives:** `crates/rts-node/src/url/url.ts` (rts-node
  owns all Node-specific `node:url` surface — the legacy parse/format/
  resolve algorithm, the file-URL conversion trio, `urlToHttpOptions`,
  `domainToASCII`/`domainToUnicode`), analogous to how `node:buffer`/
  `node:path`/`node:punycode` each own their Node-specific `.ts` under
  `rts-node`. The **class bodies** of `URL`/`URLSearchParams`/`URLPattern`
  themselves remain in `rts-shared` (universal, Web-standard, not
  Node-specific — correctly placed there today, not something this module
  should duplicate or move).

### 5.7 Shared-infra dependencies (FLAG)

- **None of the tokio / event-loop / promise-settle / TLS-rustls / crypto /
  net-socket infrastructure applies to this module at all.** Every member
  is synchronous, in-memory, and CPU-only (§5.3). Listed explicitly to rule
  it out, matching the convention set by `path.md`/`punycode.md`.
- **Cross-crate `Blob`/`URL.createObjectURL` registry** (restated from
  §5.4): `URL.createObjectURL(blob)` is implemented in `rts-shared` but
  needs to hand out an id resolvable against a `Blob` table owned by
  `rts-node` (`node:buffer`, per `buffer.md`) — and conversely
  `buffer.resolveObjectURL(id)` (rts-node) needs to read whatever registry
  `URL.createObjectURL` (rts-shared) populates. Since neither crate may
  depend on the other in the wrong direction (`rts-shared` sits below
  `rts-std` in the crate partition and has no reason to gain an
  `rts-node` dependency; `rts-node` is deliberately independent and
  shouldn't reach into `rts-shared` either), this registry likely needs a
  **third, lower home** both can reach — e.g. hoisted into `rts-engine`
  itself (which both `rts-shared` and `rts-node` already depend on) as a
  small `OnceLock<Mutex<HashMap<String, Handle>>>`-shaped primitive keyed
  by opaque handle, not `Blob`-typed, so neither crate needs to know the
  other's concrete struct — flagged as a genuine cross-module design
  question, not solved by this spec alone (mirrors the identical flag
  already raised in `buffer.md` §5.7 from the other side).
- **IDNA/Punycode host-normalization algorithm** is currently duplicated-
  in-spirit across two places that should agree byte-for-byte: (1) the
  WHATWG `URL`/`URLSearchParams` hostname parser (`rts-shared`, **today
  entirely missing IDNA** — a pre-existing parity gap, §5.1/§7), and (2)
  this module's `domainToASCII`/`domainToUnicode` (planned to reuse
  `node:punycode`'s in-crate Bootstring core, per §5.1 point 3). Both
  *should* ultimately run the same Bootstring parameters
  (`base=36, tmin=1, tmax=26, skew=38, damp=700, initial_bias=72,
  initial_n=128`, per `punycode.md` §5.1) for cross-consistent output
  (`new URL('https://測試').hostname` and
  `url.domainToASCII('測試')` must agree), but `rts-shared` cannot depend
  on `rts-node`'s `node:punycode` module (wrong direction — `rts-shared`
  sits below where `rts-node` would be in any acyclic ordering, and
  `rts-node` is meant to be fully independent besides). The pragmatic
  options are: (a) accept two independent ~150-line Bootstring
  implementations (the same "accepted duplication" precedent `buffer.md`
  §5.7 already established for base64), or (b) hoist a tiny, dependency-
  free Bootstring-only primitive into `rts-engine` (or a new
  micro-crate) both `rts-shared` and `rts-node` can call. This spec does
  not decide between (a)/(b) — flagged for an explicit owner decision in
  §7, same as the object-URL registry above.
- **GC string/byte allocation** — the only substrate every `rts-node`
  module needs unconditionally — already lives in `rts-engine`
  (`rts_engine::heap::handles`), which `rts-node` already depends on
  directly; no hoist required (same conclusion as `path.md`/`punycode.md`
  §5.7).

### 5.8 Implementation phases

1. **(a) Close the prerequisite `rts-shared` URL/URLSearchParams gaps**
   this module's re-export depends on for real Node parity (§5.1 point 1):
   wire the already-existing-but-unregistered `SET_PATHNAME` extern plus
   add the missing setters for `protocol`/`host`/`hostname`/`port`/
   `search`/`hash`/`username`/`password`; fix `URLSearchParams.append()`
   to be a true multi-value append (not aliased to `set`); add `.size`,
   `entries()`/`forEach()`/`[Symbol.iterator]()` as real paired iteration,
   and the `delete(name, value)`/`has(name, value)` 2-arg overloads. This
   is technically `rts-shared` work, not `rts-node`, but blocks calling
   this module "done" — state the scope shift explicitly per the
   CLAUDE.md "resolve blocking limitations first" rule if picked up in the
   same PR, or track as an explicit prerequisite issue if not.
2. **(b)** Add `URL.parse()` (return-null-instead-of-throw static) and
   `URL.createObjectURL`/`revokeObjectURL` to the `rts-shared` class spec
   (gated on the §5.7 object-URL-registry-home decision for the latter
   two; `URL.parse()` has no such dependency and can land independently).
3. **(c)** Add IDNA/Punycode hostname normalization to the `rts-shared`
   URL host parser (currently entirely absent, §5.1) — resolve the §5.7
   duplication-vs-hoist question first, or accept duplication and
   implement `rts-shared`'s own Bootstring pass directly.
4. **(d)** Create `rts-node/src/url/mod.rs` with the `NodespaceSpec`
   skeleton (`node_module: "url"`, `ns_prefix: "node_url"`) — expect an
   **empty or near-empty native `MEMBERS` table** per §5.2's finding;
   register in `NODE_SPECS`.
5. **(e)** Write the `.ts` re-export
   (`export { URL, URLSearchParams, URLPattern } from <ambient>`) and
   confirm `import { URL } from 'node:url'` resolves to the identical
   object identity as the ambient global (`URL === require('node:url').URL`).
6. **(f)** Implement `urlToHttpOptions` (simplest function, pure object
   assembly from an existing `URL` instance's getters) as the first
   Node-specific function — smallest possible slice proving the `.ts`
   module wiring end to end.
7. **(g)** Implement `domainToASCII`/`domainToUnicode` by calling into
   `node:punycode`'s `.ts` `toASCII`/`toUnicode` in-crate; verify against
   the `(verify)` UTS46-vs-plain-Bootstring precision question from §5.1
   point 3 using real multi-script test domains.
8. **(h)** Implement `pathToFileURL` (percent-encode `#`/`%`, backslash
   normalization on Windows, hand off to `new URL(...)`).
9. **(i)** Implement `fileURLToPath`/`fileURLToPathBuffer` (percent-decode
   to bytes, `node:path`'s `normalize()` reuse, scheme/host validation
   with the three documented error codes, Windows drive-letter/UNC vs
   POSIX host-rejection branching).
10. **(j)** Implement the legacy `url.parse`/`url.format`/`url.resolve`
    trio (largest single chunk of new `.ts` logic in this module — the
    lenient state-machine parser) — lowest priority since it is entirely
    deprecated surface, but still required for `node:url` completeness;
    reuse `node:querystring`'s (also `rts-node`-owned) `parse`/`stringify`
    for `url.parse(s, true).query`/legacy `format`'s query serialization.
11. **(k)** `URLPattern`: lowest priority (Stability 1 - Experimental);
    implement as a `.ts` pattern-string → `RegExp` compiler per URL
    component (mirroring `path.md`'s `matchesGlob` precedent) — or defer
    entirely past this module's initial P0 landing, tracked in §7.

## 6. Test plan

```
tests/node/url/url_class_basic.test.ts
  - new URL('https://user:pass@sub.example.com:8080/p/a/t/h?query=string#hash')
    has href/protocol/username/password/host/hostname/port/pathname/search/hash/origin
    all matching the expected decomposition
  - new URL('/foo', 'https://example.org/bar/') -> 'https://example.org/foo'
  - new URL('relative') with no base throws TypeError (ERR_INVALID_URL)
  - new URL({ toString: () => 'https://example.org/' }) works via coercion
  - url.toJSON() === url.toString() === url.href
  - JSON.stringify({ u: new URL('https://x/') }) embeds the href string

tests/node/url/url_idna_punycode.test.ts
  - new URL('https://測試').href === 'https://xn--g6w251d/'
  - new URL('https://%CF%80.example.com/foo').href === 'https://xn--1xa.example.com/foo'
  - url.domainToASCII('español.com') === 'xn--espaol-zwa.com'
  - url.domainToUnicode('xn--espaol-zwa.com') === 'español.com'
  - url.domainToASCII('') === '' ; url.domainToASCII(invalidLabel) === ''

tests/node/url/url_setters_and_search_asymmetry.test.ts
  - set url.pathname/protocol/host/hostname/port/search/hash and re-read each
  - new URL('https://example.org/abc?foo=~bar').search === '?foo=~bar'
  - myURL.searchParams.sort(); myURL.search === '?foo=%7Ebar' (tilde re-encode asymmetry)
  - url.searchParams live view: mutate searchParams, assert url.href updated; mutate url.search, assert searchParams updated

tests/node/url/url_static_methods.test.ts
  - URL.canParse('/foo', 'https://example.org/') === true
  - URL.canParse('/foo') === false (no base)
  - URL.parse('not a url') === null (no throw)
  - URL.parse('https://example.com/path') instanceof URL

tests/node/url/urlsearchparams_full.test.ts
  - new URLSearchParams('a=1&b=2&a=3'); getAll('a') deep-equals ['1','3']; size === 3
  - append('c','4'); toString() === 'a=1&b=2&a=3&c=4' (true multi-value, not overwrite)
  - delete('a', '1'); getAll('a') deep-equals ['3'] (2-arg delete)
  - has('b') === true; has('b','wrong') === false (2-arg has)
  - [...params] / for...of / entries()/keys()/values() all yield insertion order
  - sort(); entries in name-sorted stable order
  - new URLSearchParams({ a: '1', b: ['2','3'] })
  - new URLSearchParams([['a','1'],['a','2']])
  - new URLSearchParams([['a']]) throws (ERR_INVALID_TUPLE-shaped)
  - params.get.call({}, 'x') throws (ERR_INVALID_THIS)

tests/node/url/url_legacy_parse_format_resolve.test.ts
  - url.parse('http://user:pass@host.com:8080/p/a/t/h?query=string#hash') matches all LegacyUrlObject fields
  - url.parse('/foo/bar?a=1', true).query deep-equals { a: '1' }
  - url.parse('//foo/bar', false, true) treats 'foo' as host
  - url.format(url.parse('http://example.com/a?b=1')) round-trips to an equivalent URL string
  - url.format(new URL('https://user:pass@x/?a=1#h'), { auth: false }) omits 'user:pass@'
  - url.format(new URL('https://user:pass@x/?a=1#h'), { fragment: false, search: false }) omits '?a=1#h'
  - url.resolve('/one/two/three', 'four') === '/one/two/four'
  - url.resolve('http://example.com/', '/one') === 'http://example.com/one'
  - url.resolve('http://example.com/one', '/two') === 'http://example.com/two'

tests/node/url/url_file_path_conversion_posix.test.ts (POSIX semantics, windows:false)
  - fileURLToPath('file:///你好.txt', { windows: false }) === '/你好.txt'
  - fileURLToPath('file:///hello world', { windows: false }) === '/hello world'
  - fileURLToPath('file:///foo%231', { windows: false }) === '/foo#1'
  - fileURLToPath('file://otherhost/foo', { windows: false }) throws ERR_INVALID_FILE_URL_HOST
  - fileURLToPath('http://x/foo', { windows: false }) throws ERR_INVALID_URL_SCHEME
  - pathToFileURL('/foo#1').href === 'file:///foo%231'
  - pathToFileURL('/some/path%.c').href === 'file:///some/path%25.c'
  - fileURLToPathBuffer('file:///foo%FF', { windows: false }) returns a Buffer whose last byte is 0xFF
    (not valid UTF-8 — fileURLToPath's string form must not silently succeed/produce garbage here)

tests/node/url/url_file_path_conversion_windows.test.ts (Windows semantics, windows:true)
  - fileURLToPath('file:///C:/path/', { windows: true }) === 'C:\\path\\'
  - fileURLToPath('file://nas/foo.txt', { windows: true }) === '\\\\nas\\foo.txt'
  - pathToFileURL('C:\\path\\', { windows: true }).href === 'file:///C:/path/'
  - fileURLToPath('file:///C%3A/bad', { windows: true }) throws ERR_INVALID_FILE_URL_PATH (verify exact trigger)

tests/node/url/url_to_http_options.test.ts
  - urlToHttpOptions(new URL('https://a:b@測試?abc#foo')) deep-equals the documented shape
    (protocol 'https:', hostname 'xn--g6w251d', auth 'a:b', search '?abc', hash '#foo', path '/?abc')
  - urlToHttpOptions(new URL('http://example.com/')) has port === undefined (no explicit port)
  - urlToHttpOptions(new URL('http://example.com:8080/')) has port === 8080 (number, not string)

tests/node/url/url_object_url_blob.test.ts
  - const blob = new Blob(['hello']); const id = URL.createObjectURL(blob);
    typeof id === 'string' && id.startsWith('blob:')
  - resolveObjectURL(id) (from node:buffer) returns a Blob with size === 5 (cross-module, see buffer.md)
  - URL.revokeObjectURL(id); resolveObjectURL(id) === undefined afterward
  - URL.revokeObjectURL('blob:not-registered') does not throw (silent no-op)

tests/node/url/url_worker_threads_object_url.test.ts (multithread)
  - main thread: id = URL.createObjectURL(blob)
  - spawn a worker thread; worker calls resolveObjectURL(id) (from node:buffer) and asserts undefined
    (thread-local registry, Node's documented behavior — regression guard against
    accidentally making this cross-thread-visible)
  - each of N worker threads independently calls new URL(...)/domainToASCII(...)/fileURLToPath(...)
    concurrently on distinct inputs; assert no cross-talk/corruption (stateless-function guarantee, §5.4)

tests/node/url/url_pattern_basic.test.ts (experimental, may be gated behind a feature flag)
  - new URLPattern('https://nodejs.org/docs/latest/api/*.html').test('https://nodejs.org/docs/latest/api/dns.html') === true
  - .exec(...) returns non-null with populated pathname.groups
  - non-matching input -> test() === false, exec() === null
```

## 7. Open questions / deferrals

- **Prerequisite `rts-shared` URL/URLSearchParams gaps (§5.1 point 1,
  §5.8 phase a).** This module's `URL`/`URLSearchParams` re-export is only
  as Node-conformant as the underlying `rts-shared` implementation, which
  today is missing property setters (only `pathname`'s setter extern
  exists, and it isn't even wired to the class spec), `URL.parse()`,
  `URL.createObjectURL`/`revokeObjectURL`, true multi-value
  `URLSearchParams` semantics (`append` is currently `set` in disguise —
  a correctness bug, not just a missing feature), `.size`, real
  `entries()`/`forEach()`/`[Symbol.iterator]()`, the 2-arg
  `delete`/`has` overloads, and any IDNA/Punycode hostname normalization
  at all. Whether to fix these as part of landing `node:url` (crossing
  into `rts-shared`) or track them as a separate prerequisite issue is an
  explicit scope decision for whoever picks this up.
- **Object-URL registry ownership** (`URL.createObjectURL`/
  `revokeObjectURL` in `rts-shared` vs `buffer.resolveObjectURL` needing
  to read the same table in `rts-node`) — genuinely cross-crate, not
  resolved by either this spec or `buffer.md`. Needs an owner decision on
  where the registry actually lives (§5.7).
- **IDNA/Punycode duplication vs. hoist** — should `rts-shared`'s URL host
  parser and `rts-node`'s `node:punycode`/`domainToASCII` share one
  Bootstring implementation (requiring a new low-level shared home,
  e.g. in `rts-engine`) or maintain two independent ~150-line copies
  (matching the `buffer.md` base64-duplication precedent)? Not decided
  here (§5.7).
- **`url.domainToASCII`/`domainToUnicode` exact algorithm precision**
  versus the full WHATWG host-parsing IDNA/UTS46 pass (mapping table +
  Unicode normalization beyond plain Bootstring) — needs verification
  against real Node output for non-trivial inputs (mixed scripts,
  disallowed codepoints, already-mixed percent-encoding/Punycode input)
  before claiming full parity (§5.1 point 3).
- **`url.parse()`'s exact DEP number (if any)** — the fetched docs give
  DEP0169 (`url.resolve`) and DEP0149 (`url.format(urlString)`) explicitly
  but did not surface a dedicated number for `url.parse()` itself (only
  "deprecated since v11.0.0" / "Legacy since v15.13.0" prose); confirm
  whether one exists before emitting a deprecation-warning message that
  cites a DEP code.
- **`URLPattern`'s exact instance-property surface**
  (`pattern.hasRegExpGroups` and friends) and its `URLPatternResult`
  shape were reconstructed from general WICG/Chrome DevTools knowledge of
  the spec, not fully confirmed against the fetched Node docs (which gave
  only two short usage examples) — flagged `(verify)`; given its
  Stability 1 - Experimental status, this class is the lowest-priority
  item in this spec and a reasonable candidate to defer past the initial
  `node:url` P0 landing entirely (§5.8 phase k).
- **Whether the `rts-shared` URL/URLSearchParams global-injection prelude
  already exists** (i.e., is `URL`/`URLSearchParams` already usable with
  zero import in current RTS programs today?) or whether that ambient-
  global wiring is itself an undocumented gap this module's landing
  should also close — needs a quick check against the current prelude
  mechanism before finalizing §5.6's "already ambient" claim as fact
  rather than aspiration.
- **`new URL(buffer)`'s exact coercion behavior** (§5.5) — whether RTS
  should throw or coerce-via-`.toString()` when a `Buffer`/`Uint8Array` is
  passed as `input`, matching whatever real Node actually does (not
  fully confirmed from the fetched docs, which only state the constructor
  "does not accept" raw bytes without specifying the exact failure mode).

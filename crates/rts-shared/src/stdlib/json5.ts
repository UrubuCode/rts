// JSON5 — the rts-shared stdlib global (NOT a primordial; no native syntax).
// Pure TS over primordials + the ambient `class JSON` prelude: a single-pass
// SANITIZER rewrites the JSON5 superset (line/block comments, single-quote
// strings, unquoted keys, trailing commas, hex literals) into strict JSON,
// then delegates to `JSON.parse`. `stringify` delegates verbatim. A parse
// error returns `0` (the documented RTS sentinel this surface always used).
//
// Prelude naming rule: every local carries the `__j5` prefix (a bare name
// would alias a user module-level captured `let`).

function __json5_sanitize(__j5src: string): string {
  let __j5out = "";
  let __j5i = 0;
  const __j5n = __j5src.length;
  // Run technique (same idea as `json.ts`'s `parseString`): `__j5runStart`
  // marks the start of a maximal span of SOURCE characters this sanitizer
  // passes through UNCHANGED — the plain fallthrough at the bottom of the loop
  // used to `+=` one character at a time, and it is hit by nearly every byte
  // of the document (braces, brackets, colons, digits, whitespace). `+=` on
  // this engine's string clones the WHOLE accumulator per call
  // (`__RTS_FN_NS_GC_STRING_CONCAT` is not amortized), so that fallthrough
  // alone made the whole function O(n²) over the document length, not just
  // O(n) per string. Every special branch below FIRST flushes the pending run
  // with ONE `substring` (guarded so an empty run never pays for a no-op
  // concat — with non-amortized `+=` even an empty append clones the
  // accumulator), does its own output, then resets the run start — so the
  // outer loop now pays for (number of special constructs) `substring`s, not
  // one `+=` per character.
  let __j5runStart = 0;
  while (__j5i < __j5n) {
    const __j5c = __j5src[__j5i];
    // Strings: double-quoted pass through; single-quoted convert to double.
    if (__j5c === '"' || __j5c === "'") {
      if (__j5i > __j5runStart) __j5out += __j5src.substring(__j5runStart, __j5i);
      const __j5q = __j5c;
      __j5out += '"';
      __j5i = __j5i + 1;
      // Same run technique for the string BODY: `__j5strStart` marks a
      // pending escape-free run; only a backslash escape or a bare `"`
      // (which must be re-escaped for the surrounding double quote) breaks
      // it. `__j5strStart` is kept in lockstep with every exit path
      // (including an unterminated string falling off the end of input) so
      // the post-loop flush below is always correct and never double-flushes.
      let __j5strStart = __j5i;
      while (__j5i < __j5n) {
        const __j5s = __j5src[__j5i];
        if (__j5s === "\\") {
          if (__j5i > __j5strStart) __j5out += __j5src.substring(__j5strStart, __j5i);
          __j5out += __j5s + (__j5i + 1 < __j5n ? __j5src[__j5i + 1] : "");
          __j5i = __j5i + 2;
          __j5strStart = __j5i;
          continue;
        }
        if (__j5s === __j5q) {
          if (__j5i > __j5strStart) __j5out += __j5src.substring(__j5strStart, __j5i);
          __j5i = __j5i + 1;
          __j5strStart = __j5i;
          break;
        }
        if (__j5s === '"') {
          if (__j5i > __j5strStart) __j5out += __j5src.substring(__j5strStart, __j5i);
          __j5out += '\\"';
          __j5i = __j5i + 1;
          __j5strStart = __j5i;
          continue;
        }
        __j5i = __j5i + 1;
      }
      // Unterminated string (loop ended via the `__j5i < __j5n` condition,
      // never hit the closing-quote `break`): flush whatever run is still
      // pending. A no-op (`__j5i === __j5strStart`) when the loop exited via
      // the closing-quote branch above, since that branch already flushed and
      // synced `__j5strStart` to `__j5i`.
      if (__j5i > __j5strStart) __j5out += __j5src.substring(__j5strStart, __j5i);
      __j5out += '"';
      __j5runStart = __j5i;
      continue;
    }
    // Line comment — dropped entirely; no output for its span.
    if (__j5c === "/" && __j5i + 1 < __j5n && __j5src[__j5i + 1] === "/") {
      if (__j5i > __j5runStart) __j5out += __j5src.substring(__j5runStart, __j5i);
      while (__j5i < __j5n && __j5src[__j5i] !== "\n") {
        __j5i = __j5i + 1;
      }
      __j5runStart = __j5i;
      continue;
    }
    // Block comment — dropped entirely; no output for its span.
    if (__j5c === "/" && __j5i + 1 < __j5n && __j5src[__j5i + 1] === "*") {
      if (__j5i > __j5runStart) __j5out += __j5src.substring(__j5runStart, __j5i);
      __j5i = __j5i + 2;
      while (
        __j5i + 1 < __j5n &&
        !(__j5src[__j5i] === "*" && __j5src[__j5i + 1] === "/")
      ) {
        __j5i = __j5i + 1;
      }
      __j5i = __j5i + 2;
      __j5runStart = __j5i;
      continue;
    }
    // Trailing comma: `,` whose next significant char is `}` or `]`.
    if (__j5c === ",") {
      if (__j5i > __j5runStart) __j5out += __j5src.substring(__j5runStart, __j5i);
      let __j5k = __j5i + 1;
      while (__j5k < __j5n) {
        const __j5w = __j5src[__j5k];
        if (__j5w === " " || __j5w === "\t" || __j5w === "\n" || __j5w === "\r") {
          __j5k = __j5k + 1;
          continue;
        }
        break;
      }
      if (__j5k < __j5n && (__j5src[__j5k] === "}" || __j5src[__j5k] === "]")) {
        __j5i = __j5i + 1;
        __j5runStart = __j5i;
        continue; // drop the comma
      }
      __j5out += ",";
      __j5i = __j5i + 1;
      __j5runStart = __j5i;
      continue;
    }
    // Hex literal 0x…: decimalize.
    if (
      __j5c === "0" &&
      __j5i + 1 < __j5n &&
      (__j5src[__j5i + 1] === "x" || __j5src[__j5i + 1] === "X")
    ) {
      if (__j5i > __j5runStart) __j5out += __j5src.substring(__j5runStart, __j5i);
      let __j5k = __j5i + 2;
      let __j5v = 0;
      while (__j5k < __j5n) {
        // Manual hex-digit decode — the prelude must not reference `parseInt`
        // (a user's `import { parseInt } from "node:util"` would shadow it).
        const __j5hc = __j5src.charCodeAt(__j5k);
        let __j5d = -1;
        if (__j5hc >= 48 && __j5hc <= 57) {
          __j5d = __j5hc - 48;
        } else if (__j5hc >= 97 && __j5hc <= 102) {
          __j5d = __j5hc - 87;
        } else if (__j5hc >= 65 && __j5hc <= 70) {
          __j5d = __j5hc - 55;
        }
        if (__j5d < 0) {
          break;
        }
        __j5v = __j5v * 16 + __j5d;
        __j5k = __j5k + 1;
      }
      __j5out += String(__j5v);
      __j5i = __j5k;
      __j5runStart = __j5i;
      continue;
    }
    // Unquoted identifier: quote it when it is a KEY (next significant char
    // is `:`); keep the JSON literals (`true`/`false`/`null`) bare.
    const __j5cc = __j5src.charCodeAt(__j5i);
    const __j5isIdStart =
      (__j5cc >= 65 && __j5cc <= 90) ||
      (__j5cc >= 97 && __j5cc <= 122) ||
      __j5c === "_" ||
      __j5c === "$";
    if (__j5isIdStart) {
      if (__j5i > __j5runStart) __j5out += __j5src.substring(__j5runStart, __j5i);
      let __j5k = __j5i;
      while (__j5k < __j5n) {
        const __j5wc = __j5src.charCodeAt(__j5k);
        const __j5isId =
          (__j5wc >= 65 && __j5wc <= 90) ||
          (__j5wc >= 97 && __j5wc <= 122) ||
          (__j5wc >= 48 && __j5wc <= 57) ||
          __j5src[__j5k] === "_" ||
          __j5src[__j5k] === "$";
        if (!__j5isId) {
          break;
        }
        __j5k = __j5k + 1;
      }
      // The identifier is a verbatim slice (no per-char transform) — one
      // `substring` instead of the old `__j5word += __j5src[__j5k]` char loop.
      const __j5word = __j5src.substring(__j5i, __j5k);
      let __j5m = __j5k;
      while (__j5m < __j5n) {
        const __j5w = __j5src[__j5m];
        if (__j5w === " " || __j5w === "\t" || __j5w === "\n" || __j5w === "\r") {
          __j5m = __j5m + 1;
          continue;
        }
        break;
      }
      if (__j5m < __j5n && __j5src[__j5m] === ":") {
        __j5out += '"' + __j5word + '"';
      } else {
        __j5out += __j5word;
      }
      __j5i = __j5k;
      __j5runStart = __j5i;
      continue;
    }
    // Plain character (brace/bracket/colon/digit/space/…): NOT copied here —
    // it stays inside the pending run and is flushed in ONE `substring` the
    // next time a special branch fires (or at the final flush below).
    __j5i = __j5i + 1;
  }
  if (__j5i > __j5runStart) __j5out += __j5src.substring(__j5runStart, __j5i);
  return __j5out;
}

class JSON5 {
  static parse(text: any): any {
    try {
      return JSON.parse(__json5_sanitize(String(text)));
    } catch (e: any) {
      return 0;
    }
  }
  static stringify(value: any): any {
    return JSON.stringify(value);
  }
}

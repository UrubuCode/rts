# A key resolved once, and where a memo is allowed to live

**The question this answers**, measured 2026-08-23: why `JSON.stringify` of a
four-member object cost 4 160 ns, and why the answer turned out to have nothing
to do with JSON.

It also records two things that are worth more than the fix — a **pre-existing
defect the investigation stumbled over**, and a **method error that made two
innocent changes look like regressions**.

---

## 1. What was slow, and it was not what the file said

`entry/json/write.rs` carried a note reading *"whatever the ~800 ns per member
is, it is not this"*. Both halves were wrong. The figure came from dividing a
**fixed** cost by a member count, and the cost is not in that loop at all.

Measured by varying the SHAPE instead of the count, which is what separates a
per-item cost from a per-call one:

| | ns |
|---|---:|
| `JSON.stringify(42)` | 225 |
| `JSON.stringify({})` | 695 |
| `JSON.stringify({a:1})` | 1 417 |
| `JSON.stringify({a…h})` | 4 763 |
| `JSON.stringify([1,2,3,4])` | 778 |

So: **225 ns for any call at all, +470 for being an object, +480 per member** —
and only 74 per array ELEMENT. An element and a member write the same number, and
the member costs six to ten times as much. The whole difference is the key.

| read | ns |
|---|---:|
| `o.a + o.b + o.c + o.d`, literal keys | 39 |
| `o[k] × 4`, `k` from `Object.keys` | 1 086 |

Twenty-seven times — and, decisively, **it scaled with the length of the name**:
115 ns for a one-character key, 331 for 64 characters, 891 for 256. About three
nanoseconds a character.

A property read has no business costing anything per character of the name it
reads. That measurement is what turned a guess into a diagnosis.

---

## 2. The cause

`o[k]` where `k` is a string reaches `Context::key_of_text_cell`, which ended in

```rust
self.interner.intern(text, &mut self.keys)
```

`Interner::intern` hashes the text and probes a table. **On every access.** The
name is interned once and the number never changes, and the engine was paying to
rediscover it every time.

---

## 3. How real engines answer this, and where they put the answer

Checked against three, because the placement is the whole difficulty and getting
it from a design instinct rather than from practice is how the first attempt here
went sideways:

- **V8** computes a string's hash lazily and stores it **in the string's own
  header**, and *internalizes* strings used as property keys so that a lookup
  compares pointers rather than text.
  ([Optimizing hash tables: hiding the hash code](https://v8.dev/blog/hash-code))
- **SpiderMonkey** canonicalizes to **atoms** (`JSAtom`) — one atom per
  `(length, chars)` — and improved string-to-atom cost by **caching recently
  atomized strings**. Atoms are also GC roots.
  ([SpiderMonkey Newsletter 6](https://spidermonkey.dev/blog/2020/08/28/newsletter-6.html),
  [SpiderMonkey Internals](https://udn.realityripple.com/docs/Mozilla/Projects/SpiderMonkey/Internals))
- **JavaScriptCore** keeps the hash on the `StringImpl` beside an `isAtom` flag.

The common shape is: **memoize on the string itself, and keep the memo out of
what makes two strings equal.**

---

## 4. Where a memo may live here, which is the reusable part

This engine has three places a per-string fact could go, and they are not
equivalent. The rule that came out of this is the point of the document:

| place | who else owns it | safe for a memo? |
|---|---|---|
| a region cell's **payload slot** | the collector walks it as a possible reference; a shape assigns properties to slots | **no** — two owners already |
| an **`Aside` keyed by cell** | nothing clears it when a cell is recycled, and there is no hook in the sweep | **no** — a stale entry answers for a different object |
| the **`Str` in the slab** | nobody | **yes** |

A `Str` is allocated and freed with the string it belongs to. A memo on it cannot
outlive its text, cannot be reached by a conservative scan, and cannot be
inherited by a recycled cell. That is why `Str::key` is where it is — the same
place V8 keeps a hash — and not because the alternatives were tried and failed.

Two details that generalise beyond this field:

- **It is a `Cell`.** Resolution happens through a shared reference:
  `key_of_text_cell` holds the text out of `Context::cells` while borrowing the
  interner and the key registry mutably, which Rust allows because they are
  different fields. Asking for the slab mutably would collide.
  `entry::symbol::key_of` records the same borrow shape for its own memo.
- **It is excluded from `PartialEq`, `Eq` and `Hash`,** which is why those are
  written out by hand. A memo of something *derived* from the text must not
  change what makes two texts the same, or one string lands in two hash buckets
  depending on whether anything had looked at it. V8 excludes its hash field for
  the same reason.

**Validated rather than trusted**: the memo is read back through
`KeyRegistry::key`, which answers `None` for a number the registry never issued.
Nothing can put a wrong number there today — `remember_key` writes what `intern`
just answered — and the check costs a comparison, which is the right price for
keeping that true if a second writer ever appears.

---

## 5. What it bought

| read | before | after |
|---|---:|---:|
| `o[k]`, one character | 104 ns | **63** |
| `o[k]`, 64 characters | 289 ns | **63** |
| `o[k]`, 256 characters | 798 ns | **63** |
| `o.a`, a literal key — **control** | 26 ns | 27 |
| `o[k] × 4` over `Object.keys` | 927 ns | 746 |
| `JSON.stringify`, four members | 4 160 ns | 3 891 |

**A property read no longer costs anything per character of the name.** That is
the shape of the win, more than any single row: 798 → 63 is 12.7×, and it would
have been larger still for a longer name.

**`JSON.stringify` moved least, and the reason is the next question rather than a
disappointment.** `own_keys` hands back FRESH string cells, so the memo is cold on
every call — the key resolution has stopped being the cost and building the key
strings has become it. That belongs to `entry/json/write.rs` and to
`Context::key_text_value`, which already memoises key CELLS for a different
caller.

---

## 6. The defect this investigation found, which is not this one

**A clean tree in a DEBUG build corrupts memory after about sixty thousand
allocations. The same tree in release does not.**

```js
const ks = ["a", "b"];
for (let i = 0; i < 60000; i++) { const t = "k" + i; const o = {}; o[t] = i; }
typeof ks.map      // "function" in release, "unknown" in debug
```

`"unknown"` is not a `typeof` result the language has, so the value is garbage —
an array's method reached through a corrupted read. Reproduced on `416dc6f0` with
no local changes, in both directions, several times.

Nothing here explains it and nothing here should: it is filed as what it is. What
matters for anyone reading this document is the next section.

---

## 7. The method error, because it cost two rounds

The corruption above was blamed on a change twice, and both times the change was
innocent. The mistake was in how it was tested: **a modified DEBUG build was
compared against an unmodified RELEASE baseline**, and the difference between the
two profiles was read as the effect of the change.

The first attempt — the memo in a payload slot — was reverted on that false
diagnosis, and a commit went out asserting it had corrupted memory. It had not.
That claim is corrected in `text/mod.rs` and in `entry/json/write.rs`.

The rule this leaves, and it is the reason this section exists at all:

> **Compare like with like.** A baseline binary is only a baseline for the
> profile it was built in. Keeping `target/baseline.exe` around makes a
> release-to-release comparison easy and a debug-to-debug one easy to forget —
> and a profile difference looks exactly like a regression.

Both attempts here were finally checked as debug-against-debug **and**
release-against-release, and both are identical to the clean tree in both.

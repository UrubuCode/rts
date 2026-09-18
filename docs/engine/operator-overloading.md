# Operator overloading, opt-in by symbol

```ts
import { operators } from "rts";

class Vec2 {
  constructor(public x: number, public y: number) {}
  [operators.add](other: unknown, reversed: boolean): Vec2 { /* … */ }
  [operators.mul](k: number, reversed: boolean): Vec2 { /* … */ }
}

a + b;   // a[operators.add](b, false)
2 * v;   // v[operators.mul](2, true) — the reflected form
```

## Why opt-in, and why a symbol

The engine this one replaced turned `a + b` into `a.add(b)` whenever a method
called `add` existed. That is not JavaScript, and it broke ordinary programs:
a `Set` has `.add`, so `set + ""` stopped producing text. **No existing
program may change behaviour**, so an object is overloaded only if it declares
a key that nothing but this surface provides.

The keys are twelve symbols on `rts`'s `operators` namespace:
`add sub mul div mod pow` (binary arithmetic), `lt le gt ge` (relational),
`eq` (`==`/`!=`) and `neg` (unary `-`). Each is unique, made once per runtime,
described as `rts.operators.<name>`, and is **not** in the `Symbol.for`
registry — a program cannot reach one by spelling a string.

## The rules

1. The question is asked **only where an operand is an object**, and before
   `ToPrimitive`. Two primitives never consult anything. When nothing
   declared the symbol, the answer is the specification's, bit for bit.
2. Binary `a OP b`, with at least one object: if `a` is an object and `a[sym]`
   (an ordinary Get, prototype chain included) is callable, the answer is
   `a[sym](b, false)`; otherwise, if `b` is an object and `b[sym]` is
   callable, `b[sym](a, true)`; otherwise the specification's path.
3. Relational operators: each symbol answers its own operator. There is no
   derivation — `gt` is never the negation of `le`. The answer goes through
   `ToBoolean`.
4. `eq` is asked only when **both** operands are objects, so `x == null` and
   the compiler's settled arms for `== null`/`== undefined` stay correct with
   no change. `!=` is the negation of `==`. `===` and `!==` are identity and
   are never overloaded.
5. Unary `-` on an object with a callable `[neg]` answers `obj[neg]()`.
6. `+=`, `-=` and the other compound assignments lower to the same entry
   points, so they overload the same way.
7. A throw inside the method — or inside a getter the Get reached —
   propagates like any throw.

`+x`, `x++` and `x--` are **not** overloadable, and are not `*`: they were
emitted as `x * 1`, which stopped being the same program the day `*` could be
answered by an object. They go through `__rts_unary_plus`, which is that
multiply with the overload question left out.

## Where it lives

`crates/rts-core/src/entry/overload.rs` owns the symbols and the check;
`subtract`, `multiply`, `divide`, `remainder`, `exponent`, `add`, the four
relational entries, `loose_equals` and `negate` each ask it first.
`crates/rts-std` builds the `rts` module lazily, and building it is what mints
the symbols and arms the check.

## Cost

Numbers pay nothing: the first test is on the tag, with no borrow. An object
operand in a program that never imported `rts` pays one borrow and one `None`
test. With `rts` imported, an object with no overload pays one symbol-keyed
property lookup — a miss — per operand before `ToPrimitive`. Not measured in
release yet; `docs/codegen/entry-tax.md` part five is why the check sits in
the object branch and nowhere earlier.

## The limitation

`tsc` and every editor report `a + b` between two classes as a type error
(`TS2365`). TypeScript has no syntax for declaring an operator overload, and
RTS runs the program because it does not type-check it. `rts emit-types`
declares the twelve symbols as `unique symbol`, which is what makes
`[operators.add](…) {}` type-check as a method name; the operator expression
itself needs a cast (`(a as any) + b`) or a `// @ts-expect-error` to pass
`tsc`.

Also worth knowing: a `Proxy` operand sees one extra `get` trap, for the
operator's symbol, in a program that imported `rts`.

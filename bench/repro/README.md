# The inline-cache cliff

Two files that differ by **one line** — `ic_miss_slow.ts` declares
`function padZ(w: number): number { return w + 1; }` and never calls it in the
measured path — and by a factor of 17 in what a property read costs.

```
RTS_TIMING=1 rts run bench/repro/ic_miss_fast.ts   # read own  14.4 ns/op, 75 cache misses
RTS_TIMING=1 rts run bench/repro/ic_miss_slow.ts   # read own 240.9 ns/op, 1 135 690 cache misses
```

`rts-timing cache misses` is the count `Compiled::resolves` keeps: how many
times a cached read failed to recognise the layout it saw and called
`rts_cache_resolve` to resolve the property by name. One per iteration of the
measured loop means the cache never recognises anything, which is what the
second number is.

**Why these files and not a smaller pair.** Smaller ones do not reproduce it,
and that is the part still unknown. Measured and ruled out as the trigger: the
number of top-level bindings, the number of distinct shapes and property keys,
the number of functions in the file, storing a closure in an object, calling
through a property, an outer-scope variable, prior allocation pressure, and a
`try`/`catch` around the call. Named function declarations flip it in this
shape and anonymous arrows do not, which is the only asymmetry found so far.

They are kept as files rather than as a test because the assertion a test would
make — "a property read costs about the same either way" — is exactly what is
not true yet. Delete them with the fix.

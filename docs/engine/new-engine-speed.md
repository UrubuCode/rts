# How fast the new engine is, measured against the old one

**2026-08-04.** Both release builds. The old engine's binary was rebuilt first —
the one on disk was two days old and `git log` showed its sources had changed
since, which would have made the comparison a measurement of the wrong program.

## The number

One kernel, in the only subset both engines can run:

```js
let total = 0;
for (let i = 0; i < N; i = i + 1) { total = total + i; }
return total;
```

| | per pass | for 20 M passes |
|---|---:|---:|
| old engine | **0.73 ns** | ~15 ms |
| new engine | **94 ns** | ~1 880 ms |

**The new engine is about 130× slower on this kernel.** Compilation is the other
direction and is not close: 0.16 ms against a process that spends 63 ms before
it runs anything — though those measure different things, and the section on
what this does not measure says why.

## How each number was obtained

The old engine runs as a process, so its cost includes startup. An empty
program takes 63 ms, and the kernel's own cost is the difference — which is only
meaningful if it scales, so that was checked rather than assumed:

| passes | wall | minus startup |
|---:|---:|---:|
| 20 M | 78.8 ms | 14.7 ms |
| 40 M | 90.5 ms | 26.5 ms |
| 80 M | 122.7 ms | 58.6 ms |

Doubling the passes doubles the work, so the loop is real and not folded away.
The new engine was checked the same way: 1 M passes take 96.7 ms and 20 M take
1 880 ms, which is linear to within 3 %.

Both harnesses report **best and median**, not mean. The best run is the one
least interfered with, and the median says whether that best was typical.

## Where the 130× is, measured rather than reasoned

The explanation on offer was that every operator is a call into the runtime,
because no type pass exists and nothing has been proved about any operand. That
is a claim, so it was falsified against the alternative — that the cost is per
pass rather than per operator:

| operators in the loop | ns per pass |
|---:|---:|
| 3 | 94.2 |
| 4 | 119.3 |
| 5 | 143.1 |

**+24.5 ns per operator, linear.** The passes are identical and only the
operator count changes, so the cost is the calls. Dividing the first row by the
four calls a pass actually makes — `<`, `+`, `+`, and the `ToBoolean` the
condition needs — gives 23.6 ns each, which agrees.

So the gap is not code quality and not the code generator. It is that `i + 1` is
a function call that crosses an ABI boundary and borrows a thread-local context,
where the old engine emits an add.

## What this does not measure, and must not be read as

- **Anything but arithmetic over locals.** The new engine has no functions, no
  objects, no strings and no property access. The kernel was chosen as the
  intersection of the two engines, and says nothing about a program that uses
  what only one of them has.
- **A finished engine against a finished engine.** The old one has had years of
  work; the new one reached its first executable program yesterday. The 130× is
  what the design costs *before* the work that makes those calls unnecessary —
  a floor, not a verdict.
- **Compilation, comparably.** 0.16 ms is a function call in this process;
  63 ms is a process starting, reading a file, and building a prelude. They are
  not the same activity and the ratio between them means nothing.

## What it is evidence for

That the next phase is the type pass, and that its target is now specific rather
than aspirational: **every operator whose operands can be proved numeric stops
being a call.** The measurement says what each one costs, so it also says what
proving one is worth — about 24 ns per site per pass.

It is also evidence that the machine is not the problem. The old engine reaches
0.73 ns per pass through the same code generator.

## Reproducing it

```bash
cargo build --release -p rts-host-rwk --example kernel --example perop
target/release/examples/kernel.exe 20000000 5
target/release/examples/perop.exe 2000000
```

For the old engine, a `.ts` file holding the kernel wrapped in a function and
called, run with `target/release/rts.exe run <file>`, timed against an empty
file for startup.

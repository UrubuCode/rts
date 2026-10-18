# Fifteen published techniques, priced against this engine, and none bought

**2026-08-29.** Five research fronts over primary sources — V8, JSC,
SpiderMonkey, HotSpot, Dart AOT, .NET NativeAOT, OCaml, Go, wasmtime/Cranelift —
for the three costs this engine has measured: the per-call activation record,
allocation, and root reporting. Every technique found was then confronted with
what this tree has already measured.

**Fifteen proposals, fifteen refutations.** That is the result, and it is worth
writing down because the next reader will otherwise find the same fifteen, and
because two of them are attractive enough to be tried twice.

The refusals are not "we know better". Each names the specific thing that makes
the technique price differently here.

---

## What the research CORRECTED, and this is the part that matters

Three findings contradict documents in this repository, and all three make the
keystone work **cheaper** than `the-unwired-keystone.md` assumes.

### 1. The conservative scan is the enabler, not the obstacle

`native-call-floor.md` §3a prices a GC trap on merging the three activation
stacks: `callees` and `pending_arguments` are roots scanned as flat `&[u64]`
slices. That trap is real for the design that KEEPS the side stacks.

It disappears for the design that moves the record into the callee's own machine
frame, which is what V8, JSC and SpiderMonkey all do — two stores into the
argument region the caller is already writing, no capacity check, no pop, and
the `ret` destroys it. `roots::scan_stack` already walks the machine stack
looking for encoded values carrying a reference tag, so **a word written into a
frame is already a root, for free**.

JSC is the existence proof that the two are orthogonal: its collector scans the
stack and registers conservatively AND its frames are precise, at the same time.

### 2. `the-unwired-keystone.md` decides the walk's shape on a false premise

It concludes the chain must come from unwind information rather than `rbp`,
because a Rust frame between two compiled ones may not keep a frame pointer.

- wasmtime walks exactly that case, on Windows, with `preserve_frame_pointers`
  and pure `rbp` arithmetic — two loads per frame — and never traverses a host
  frame at all, because the entry and exit frame pointers recorded at the
  crossing say where to skip. Crossings are recorded per ACTIVATION, not per
  call: JSC writes `VM::topCallFrame`, HotSpot the `last_Java_frame` anchor, V8
  `thread_local_top().c_entry_fp_`, wasmtime `last_wasm_exit_fp/pc`. Three
  stores at a boundary, not one per call.
- The premise is removable anyway: `-C force-frame-pointers=yes` makes rustc
  keep `rbp` in every frame.

And one sentence in that document is false: it says the machine layer's
`unwind/` already produces our unwind tables. `crates/rts-cranelift/src/unwind/`
is the planner for JavaScript `try`/`catch` protected regions and has nothing to
do with `.pdata`/`.xdata`. Neither `cranelift-jit` nor `cranelift-object` 0.131
emits or registers unwind info, so `RtlVirtualUnwind` would treat every compiled
function as a leaf today and read `[rsp]` as the return address — wrong for any
frame that has locals.

### 3. Cranelift already emits half of it

- `isa/x64/abi.rs::gen_prologue_frame_setup` emits `push rbp; mov rbp, rsp`, so
  `[rbp]` is the caller's frame pointer and `[rbp+8]` the return address —
  wasmtime's arithmetic exactly.
- `flags.unwind_info()` defaults to true, so `CompiledCode::create_unwind_info`
  would answer `Some(WindowsX64(..))` today. Nothing consumes it.
- `MachBufferFinalized::user_stack_maps()` already delivers precise maps with
  offsets resolved against SP.

---

## The refutations, by what they refuse

**Frame slots for callee and argc — refused by a measurement already in this
tree.** The small version of the same change is `native-call-floor.md` §5b: it
was implemented in full, every path checked byte-identical, measured over eleven
alternations, and reverted. `c.m(a)` improved 9.1% while `set.has(7)` regressed
7.0%, and the function-call loop went 561.7 to 607.3 ms, **8% worse**. The
likely mechanism is that `invoke` became inlinable once its failure path moved,
which changes register allocation at each of its four call sites. A change that
makes a program 8% slower and cannot be explained is what the honesty floor
exists to stop.

**TLAB and JSC's bump-and-pop — refused by regime.** Both are bump allocators.
This engine's steady state is a FREE LIST: `Region::alloc` asks the free head
first, so after the first cycle every allocation is a pop and not a bump. The
sequences are real and the numbers are real; they price a path this heap does
not take.

**Stack maps everywhere — refused by arithmetic already done.** Go's +10% binary
size and Dart's figures price the branch this engine has not taken;
`rts_cranelift::gc::describe_frames` has zero callers outside its own tests, so
the 5% Go reports "saving" is a refund of a cost never paid here. And this
engine is not conservative in Boehm's sense: `collect::conservative_roots` tests
the NaN-box TAG, so a double, an i32 and a boolean are rejected outright. It is
a filtered scan, and most of the literature's conservative-scanning costs do not
apply to it.

**Dart's switchable calls and the Global Dispatch Table — refused for lack of a
number.** Both mechanisms verify byte-for-byte against Dart's source. Neither
has a published nanosecond. The GDT figure that circulates is about 2% of
INSTRUCTION SIZE, not of time.

**The heap sizing rule — refused as already shipped.** Princeton's
`((c1*R) + (c2*H)) / (H - R)`, with "if R / H is larger than .5, increase heap
size", is the rule, verified from the source slides. It is `a39d42ab`, committed
hours before the research ran. Independent arrival at the same rule is the
useful half of that refutation.

---

## What this leaves

Not a technique. A correction to the ordering.

The keystone — a stack walk plus per-activation crossing records — is the
precondition the plan already named, and this research says it is **smaller than
the plan priced it**: `rbp` walking at two loads per frame, three stores per host
crossing rather than one per call, and a prologue Cranelift already emits.

What it does NOT say is that removing the three per-call `Vec`s is safe once the
walk exists. §5b measured a regression in that neighbourhood that nobody has
explained, and an unexplained 8% is a reason to build the walk first and measure
again — not a reason to assume a second attempt lands differently.

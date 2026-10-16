//! Loop versioning: a clone of an innermost loop whose accumulator is a machine double.
//!
//! # What this buys, measured before it was written
//!
//! `function f(o, n) { let a = 0; for (let i = 0; i < n; i++) a += o.x; return a; }` costs
//! ~2.9 ns per pass against ~0.9 for the same loop with `a += 1.5`. The difference is the
//! accumulator: `a + o.x` may concatenate, so `a` is a tagged loop-carried value and every
//! pass guards it, adds, and boxes the sum again.
//!
//! # The idea, and why it needs no deoptimiser
//!
//! The loop is compiled TWICE, in the same function. A preheader tests, once, what the clone
//! assumes; if every test holds it enters the clone, and otherwise the original, untouched.
//! What the clone cannot assume once is tested where it is used, and a failure there is a
//! jump to the ORIGINAL loop -- a block of the same graph over the same frame -- so nothing
//! is reconstructed and nothing is re-run.
//!
//! ```text
//!   preheader ──(each probe is a number?)──┬── yes ──▶ CLONE: the accumulator is a double
//!        │                                 └── no ───▶ ORIGINAL (untouched)
//!
//!   CLONE, at a `+` whose operand is neither hoisted nor known:
//!        operand is a number? ── yes ─▶ double add, stay in the clone
//!                             └─ no ──▶ the ORIGINAL generic `+`, once, then the rest of
//!                                       this pass in the ORIGINAL loop
//! ```
//!
//! # Three kinds of operand, from cheapest to dearest
//!
//! 1. **A value defined before the loop** (`n`, a parameter `c`): an SSA value cannot change,
//!    so it is tested once in the preheader and read as a double throughout. Nothing is
//!    tested per pass.
//! 2. **A property read of such a value** (`o.x`, `o` defined before the loop): hoisted the
//!    same way, by `NumberPeek`, which asks the site's cache for the property and runs no
//!    getter. This is sound ONLY if nothing in the clone can change the property, so it is
//!    taken only when every instruction left in the clone is quiet -- no write, no call into
//!    code the program wrote -- judged from the inferred types after the clone is built. If
//!    it is not quiet the reads are not hoisted and the third kind is used.
//! 3. **Anything else** (an element, a property read that cannot be hoisted): tested on every
//!    pass where it is read, with the exit described above.
//!
//! # What makes the clone sound
//!
//! * It is entered only when each probe holds. A probe that does not hold, for any reason --
//!   a string, a bigint, an object with `valueOf`, a getter, a missing property -- runs the
//!   original loop, which is the program as written.
//! * Inside it the accumulator is only ever assigned a double, so inference types the header
//!   parameter a double and the machine keeps it unboxed.
//! * The only `+` that could have produced a string is the one with an operand nothing
//!   proved; each such operand is a probe or is tested where it is read.
//! * An operand read in the loop is read ONCE per pass, in its original position, so a getter
//!   runs exactly as often as before. A hoisted read is only taken when it cannot be a getter
//!   and nothing in the clone can write.
//!
//! # What is declined, and why
//!
//! Anything this cannot prove sound is left alone: a loop that is not innermost, one with a
//! protected region, a closure made inside, a suspension; a per-pass site whose block has
//! more than its terminator after the `+` (the transfer would need the values live across
//! the split); a loop that reads a value defined inside it from outside it (the exit would
//! read the original's copy). Each is listed where it is checked.

use std::collections::{BTreeMap, BTreeSet};

use rts_cranelift::fault::Position;
use rts_mir::cfg::{Block, BlockId, Callee, Const, Func, Inst, InstId, Op, Terminator, ValueId};
use rts_mir::infer::infer;
use rts_mir::{Domain, Effect};

use crate::domain::{Js, JsConst, JsPrim, Type};

/// Versions every innermost loop it can. Answers how many it did.
pub fn version_loops(func: &mut Func, domain: &Js) -> usize {
    // A PROTECTED REGION or a parking frame changes where a jump may land and what a copy
    // of a block means; none of it is worth the risk for a loop that is not in one.
    if !func.regions.is_empty() || func.may_suspend {
        return 0;
    }
    let add = domain.prim(JsPrim::Add);
    if !func
        .insts
        .iter()
        .any(|inst| matches!(&inst.op, Op::Prim { prim, .. } if *prim == add))
    {
        return 0;
    }
    let original_blocks = func.blocks.len();
    let headers: Vec<usize> = {
        let shape = Shape::of(func);
        (0..original_blocks)
            .filter(|&b| {
                shape.reach[b] && shape.preds[b].iter().any(|&u| shape.dominates(b, u))
            })
            .collect()
    };
    let mut done = 0;
    for header in headers {
        if let Some(versioned) = try_version(func, domain, header) {
            *func = versioned;
            done += 1;
        }
    }
    done
}

/// Whether a primitive reads its operands as numbers where it can.
///
/// The operators for which an operand that is a double lets the machine emit the
/// instruction instead of a guarded call. A value replaced by its double is replaced ONLY as
/// an operand of one of these: it stays what it was everywhere else.
fn numeric_use(which: JsPrim) -> bool {
    matches!(
        which,
        JsPrim::Add
            | JsPrim::Subtract
            | JsPrim::Multiply
            | JsPrim::Divide
            | JsPrim::Remainder
            | JsPrim::Exponent
            | JsPrim::LessThan
            | JsPrim::GreaterThan
            | JsPrim::LessOrEqual
            | JsPrim::GreaterOrEqual
            | JsPrim::BitAnd
            | JsPrim::BitOr
            | JsPrim::BitXor
            | JsPrim::ShiftLeft
            | JsPrim::ShiftRight
            | JsPrim::ShiftRightUnsigned
    )
}

/// The control-flow facts a decision reads, computed once per attempt.
struct Shape {
    succs: Vec<Vec<usize>>,
    preds: Vec<Vec<usize>>,
    reach: Vec<bool>,
    idom: Vec<Option<usize>>,
}

impl Shape {
    fn of(func: &Func) -> Self {
        let n = func.blocks.len();
        let succs: Vec<Vec<usize>> = (0..n)
            .map(|b| {
                func.blocks[b]
                    .terminator
                    .as_ref()
                    .map(|end| end.successors().iter().map(|s| s.0 as usize).collect())
                    .unwrap_or_default()
            })
            .collect();
        // Post-order by an explicit stack: a function can be deeper than the call stack likes.
        let mut seen = vec![false; n];
        let mut order = Vec::with_capacity(n);
        let mut stack = vec![(0usize, 0usize)];
        seen[0] = true;
        while let Some((b, i)) = stack.last().copied() {
            if i < succs[b].len() {
                stack.last_mut().expect("just read").1 += 1;
                let next = succs[b][i];
                if !seen[next] {
                    seen[next] = true;
                    stack.push((next, 0));
                }
            } else {
                order.push(b);
                stack.pop();
            }
        }
        let mut post = vec![usize::MAX; n];
        for (at, &b) in order.iter().enumerate() {
            post[b] = at;
        }
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        for b in 0..n {
            if seen[b] {
                for &s in &succs[b] {
                    preds[s].push(b);
                }
            }
        }
        let mut idom: Vec<Option<usize>> = vec![None; n];
        idom[0] = Some(0);
        loop {
            let mut changed = false;
            for &b in order.iter().rev() {
                if b == 0 {
                    continue;
                }
                let mut chosen: Option<usize> = None;
                for &p in &preds[b] {
                    if idom[p].is_none() {
                        continue;
                    }
                    chosen = Some(match chosen {
                        None => p,
                        Some(current) => {
                            let (mut x, mut y) = (p, current);
                            while x != y {
                                while post[x] < post[y] {
                                    x = idom[x].expect("processed");
                                }
                                while post[y] < post[x] {
                                    y = idom[y].expect("processed");
                                }
                            }
                            x
                        }
                    });
                }
                if chosen != idom[b] {
                    idom[b] = chosen;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Self {
            succs,
            preds,
            reach: seen,
            idom,
        }
    }

    /// Whether `a` dominates `b`. Both must be reachable.
    fn dominates(&self, a: usize, b: usize) -> bool {
        if !self.reach[b] || !self.reach[a] {
            return false;
        }
        let mut at = b;
        loop {
            if at == a {
                return true;
            }
            match self.idom[at] {
                Some(up) if up != at => at = up,
                _ => return false,
            }
        }
    }
}

/// One `+` tested on every pass, because its operand is neither known nor hoistable.
#[derive(Clone)]
struct Site {
    /// The block that holds it; the `+` is that block's last instruction.
    block: usize,
    /// Which parameter of the header the accumulator operand is.
    param: usize,
    /// The operand nothing proved, tested where it is read.
    x: ValueId,
}

/// A property read hoisted out of the loop.
#[derive(Clone)]
struct Peek {
    /// The receiver, defined before the loop.
    object: ValueId,
    /// The key constant's declared index.
    key: u32,
    /// The read's result, whose every use is an operand of a numeric operator.
    result: ValueId,
    /// The read, which the clone leaves out.
    read: InstId,
}

/// What the clone assumes and how it is told apart.
#[derive(Clone, Default)]
struct Plan {
    /// Header parameters that become doubles, and whether their entry value is tested.
    params: Vec<(usize, bool)>,
    sites: Vec<Site>,
    /// Values defined before the loop, tested once and read as doubles.
    values: Vec<ValueId>,
    peeks: Vec<Peek>,
}

/// Everything about one loop that a plan is made from.
struct Facts<'a> {
    func: &'a Func,
    domain: &'a Js,
    types: rts_mir::infer::Types<Type>,
    shape: Shape,
    h: usize,
    body: BTreeSet<usize>,
    entry_block: usize,
    entry_args: Vec<ValueId>,
    header_params: Vec<ValueId>,
    def_block: Vec<Option<usize>>,
    def_inst: Vec<Option<InstId>>,
    readers: Vec<Vec<usize>>,
    /// Values read by anything but an operand of a numeric operator.
    other_use: BTreeSet<ValueId>,
    at: Position,
}

impl Facts<'_> {
    fn outside(&self, value: ValueId) -> bool {
        !self.def_block[value.0 as usize].is_some_and(|b| self.body.contains(&b))
    }

    fn numeric(&self, value: ValueId) -> bool {
        matches!(self.types.of(value), Type::Int32 | Type::Double)
    }

    fn param_index(&self, value: ValueId) -> Option<usize> {
        self.header_params.iter().position(|q| *q == value)
    }
}

fn try_version(func: &Func, domain: &Js, h: usize) -> Option<Func> {
    if h == 0 {
        return None;
    }
    let shape = Shape::of(func);
    let n = func.blocks.len();
    let latches: Vec<usize> = shape.preds[h]
        .iter()
        .copied()
        .filter(|&u| shape.dominates(h, u))
        .collect();
    if latches.is_empty() {
        return None;
    }
    // THE NATURAL LOOP: the header, and everything that reaches a latch without passing it.
    let mut body: BTreeSet<usize> = BTreeSet::new();
    body.insert(h);
    let mut work: Vec<usize> = latches.clone();
    while let Some(b) = work.pop() {
        if body.insert(b) {
            work.extend(shape.preds[b].iter().copied());
        }
    }
    // INNERMOST ONLY: a loop that holds another back edge is an outer loop, and cloning it
    // would clone the inner one with it.
    for &u in &body {
        for &v in &shape.succs[u] {
            if v != h && body.contains(&v) && shape.dominates(v, u) {
                return None;
            }
        }
    }
    let mut at: Option<Position> = None;
    for &b in &body {
        for inst in &func.blocks[b].insts {
            let held = &func.insts[inst.0 as usize];
            match &held.op {
                // A copy would duplicate a point or a parking, and the closure sites are
                // numbered by the lowering that made them.
                Op::Suspend { .. } | Op::Guard { .. } => return None,
                // A CALL IS NOT WORTH DUPLICATING: the clone exists to make arithmetic
                // cheaper, and a loop that calls out pays for the call either way. Declining
                // also keeps one call site per call, which gates count in the emitted IR.
                Op::Call { .. } => return None,
                Op::Prim { prim, .. } => {
                    match domain.meaning(*prim) {
                        Some(
                            JsPrim::MakeClosure
                            | JsPrim::EnvNew
                            | JsPrim::AwaitDrain
                            | JsPrim::Construct,
                        ) => {
                            return None;
                        }
                        Some(JsPrim::Add) if at.is_none() => at = Some(held.at.clone()),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if matches!(
            func.blocks[b].terminator,
            Some(Terminator::Fall(_) | Terminator::CleanupDone) | None
        ) {
            return None;
        }
    }
    let at = at?;

    // EXACTLY ONE EDGE FROM OUTSIDE, so there is one place to put the preheader.
    let mut entry: Option<(usize, Vec<ValueId>)> = None;
    let mut outside_edges = 0;
    for &p in &shape.preds[h] {
        if body.contains(&p) {
            continue;
        }
        let end = func.blocks[p].terminator.as_ref()?;
        match end {
            Terminator::Jump { target, args } if target.0 as usize == h => {
                outside_edges += 1;
                entry = Some((p, args.clone()));
            }
            Terminator::Branch {
                then_block,
                then_args,
                else_block,
                else_args,
                ..
            } => {
                if then_block.0 as usize == h {
                    outside_edges += 1;
                    entry = Some((p, then_args.clone()));
                }
                if else_block.0 as usize == h {
                    outside_edges += 1;
                    entry = Some((p, else_args.clone()));
                }
            }
            _ => {}
        }
    }
    if outside_edges != 1 {
        return None;
    }
    let (entry_block, entry_args) = entry?;

    let types = infer(func, domain);
    let mut def_block: Vec<Option<usize>> = vec![None; func.values as usize];
    let mut def_inst: Vec<Option<InstId>> = vec![None; func.values as usize];
    let mut readers: Vec<Vec<usize>> = vec![Vec::new(); func.values as usize];
    let mut other_use: BTreeSet<ValueId> = BTreeSet::new();
    for b in 0..n {
        for param in &func.blocks[b].params {
            def_block[param.0 as usize] = Some(b);
        }
        for inst in &func.blocks[b].insts {
            let held = &func.insts[inst.0 as usize];
            def_block[held.result.0 as usize] = Some(b);
            def_inst[held.result.0 as usize] = Some(*inst);
            let numeric = matches!(
                &held.op,
                Op::Prim { prim, .. } if domain.meaning(*prim).is_some_and(numeric_use)
            );
            for read in func.reads(*inst) {
                readers[read.0 as usize].push(b);
                if !numeric {
                    other_use.insert(read);
                }
            }
        }
        if let Some(end) = &func.blocks[b].terminator {
            for read in end.reads() {
                readers[read.0 as usize].push(b);
                other_use.insert(read);
            }
        }
    }
    // LOOP-CLOSED: nothing outside reads a value defined inside. The exits of the clone go
    // to the original exit blocks, which would read the ORIGINAL loop's copy.
    for (value, who) in readers.iter().enumerate() {
        let Some(defined) = def_block[value] else {
            continue;
        };
        if body.contains(&defined) && who.iter().any(|reader| !body.contains(reader)) {
            return None;
        }
    }

    let header_params = func.blocks[h].params.clone();
    let facts = Facts {
        func,
        domain,
        types,
        shape,
        h,
        body,
        entry_block,
        entry_args,
        header_params,
        def_block,
        def_inst,
        readers,
        other_use,
        at,
    };
    // A PEEK first, because it is the dearest to be wrong about and the cheapest per pass:
    // the plan without it is the fallback when the clone is not quiet.
    let with = make_plan(&facts, true)?;
    let hoists = !with.peeks.is_empty();
    if let Some(done) = attempt(&facts, with) {
        return Some(done);
    }
    if hoists {
        let without = make_plan(&facts, false)?;
        return attempt(&facts, without);
    }
    None
}

/// Whether a numeric operator's two operands can both be numbers worth speculating on.
fn pairs_with_accumulator(facts: &Facts, which: JsPrim, other: Option<ValueId>, plan: &Plan) -> bool {
    match (which, other) {
        // `+` concatenates unless both are numbers, so an operand is only worth testing
        // beside an operand the clone will make a double.
        (JsPrim::Add, Some(other)) => {
            facts.numeric(other)
                || facts
                    .param_index(other)
                    .is_some_and(|j| plan.params.iter().any(|(held, _)| *held == j))
        }
        (JsPrim::Add, None) => false,
        _ => true,
    }
}

fn make_plan(facts: &Facts, allow_peek: bool) -> Option<Plan> {
    let func = facts.func;
    let domain = facts.domain;
    let add = domain.prim(JsPrim::Add);
    // THE ACCUMULATORS: a header parameter an `+` reads, that nothing proves a number yet,
    // and that enters as a number or as something one test can decide.
    let mut plan = Plan::default();
    for (j, param) in facts.header_params.iter().enumerate() {
        if *facts.types.of(*param) != Type::Anything {
            continue;
        }
        let read_by_add = facts.body.iter().any(|&b| {
            func.blocks[b].insts.iter().any(|inst| {
                matches!(
                    &func.insts[inst.0 as usize].op,
                    Op::Prim { prim, args } if *prim == add && args.contains(param)
                )
            })
        });
        if !read_by_add {
            continue;
        }
        match facts.types.of(facts.entry_args[j]) {
            Type::Int32 | Type::Double => plan.params.push((j, false)),
            Type::Anything => plan.params.push((j, true)),
            _ => {}
        }
    }
    if plan.params.is_empty() {
        return None;
    }

    let mut values: BTreeSet<ValueId> = BTreeSet::new();
    let mut peeks: Vec<Peek> = Vec::new();
    let mut sites: Vec<Site> = Vec::new();
    for &b in &facts.body {
        let insts = &func.blocks[b].insts;
        for (index, inst) in insts.iter().enumerate() {
            let held = &func.insts[inst.0 as usize];
            let Op::Prim { prim, args } = &held.op else {
                continue;
            };
            let Some(which) = domain.meaning(*prim) else {
                continue;
            };
            if !numeric_use(which) {
                continue;
            }
            for (k, &v) in args.iter().enumerate() {
                let other = (args.len() == 2).then(|| args[1 - k]);
                if !pairs_with_accumulator(facts, which, other, &plan) {
                    continue;
                }
                if facts.numeric(v) || *facts.types.of(v) != Type::Anything {
                    continue;
                }
                if Some(v) == other {
                    continue;
                }
                if facts.outside(v) {
                    values.insert(v);
                    continue;
                }
                // A READ OF A PROPERTY, hoistable when its receiver is defined before the
                // loop, its key is a constant, and every use of it is a numeric operand.
                if allow_peek
                    && let Some(peek) = peek_of(facts, v)
                {
                    if !peeks.iter().any(|held| held.result == peek.result) {
                        peeks.push(peek);
                    }
                    continue;
                }
                // OTHERWISE a per-pass site, and only for the `+` an accumulator feeds.
                if which != JsPrim::Add || index + 1 != insts.len() {
                    continue;
                }
                let Some(other) = other else { continue };
                let Some(param) = facts.param_index(other) else {
                    continue;
                };
                if !plan.params.iter().any(|(held, _)| *held == param) {
                    continue;
                }
                if site_ok(facts, b) && !sites.iter().any(|held| held.block == b) {
                    sites.push(Site { block: b, param, x: v });
                }
            }
        }
    }
    plan.values = values.into_iter().collect();
    plan.peeks = peeks;
    plan.sites = sites;
    Some(plan)
}

fn peek_of(facts: &Facts, value: ValueId) -> Option<Peek> {
    let func = facts.func;
    let read = facts.def_inst[value.0 as usize]?;
    let Op::Prim { prim, args } = &func.insts[read.0 as usize].op else {
        return None;
    };
    if facts.domain.meaning(*prim) != Some(JsPrim::FieldRead) || args.len() != 2 {
        return None;
    }
    if !facts.def_block[value.0 as usize].is_some_and(|b| facts.body.contains(&b)) {
        return None;
    }
    if facts.other_use.contains(&value) {
        return None;
    }
    let (object, key) = (args[0], args[1]);
    if !facts.outside(object) {
        return None;
    }
    let key_inst = facts.def_inst[key.0 as usize]?;
    let Op::Const(Const::Declared(index)) = &func.insts[key_inst.0 as usize].op else {
        return None;
    };
    if !matches!(facts.domain.declared(*index), Some(JsConst::Key(_))) {
        return None;
    }
    // EVERY USE is the `+` or comparison it was written in, whose other operand this plan
    // makes a number: a use it cannot account for leaves the read where it is.
    Some(Peek {
        object,
        key: *index,
        result: value,
        read,
    })
}

/// Whether the `+` ending block `b` can be left through a transfer to the original loop.
fn site_ok(facts: &Facts, b: usize) -> bool {
    let func = facts.func;
    if b == facts.h {
        return false;
    }
    let Some(Terminator::Jump { target, .. }) = &func.blocks[b].terminator else {
        return false;
    };
    // NOTHING DEFINED IN THIS BLOCK IS READ OUTSIDE IT: the transfer leaves the clone at the
    // `+` and enters the original at its successor, so a value the original's later blocks
    // read from here would not exist on that path.
    if func.blocks[b].insts.iter().any(|held| {
        let result = func.insts[held.0 as usize].result;
        facts.readers[result.0 as usize]
            .iter()
            .any(|&reader| reader != b)
    }) {
        return false;
    }
    // WHERE THE TRANSFER LANDS reads only what it can have: its own parameters, its own
    // results, and what was defined before the loop. See the module's note.
    let landing = target.0 as usize;
    if facts.body.contains(&landing) && landing != facts.h {
        return landing_is_self_contained(facts, landing);
    }
    true
}

/// Whether the blocks reachable from `landing` without passing the header read only values
/// they define themselves, or that exist before the loop.
fn landing_is_self_contained(facts: &Facts, landing: usize) -> bool {
    let func = facts.func;
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let mut work = vec![landing];
    while let Some(b) = work.pop() {
        if b == facts.h || !facts.body.contains(&b) || !seen.insert(b) {
            continue;
        }
        work.extend(facts.shape.succs[b].iter().copied());
    }
    for &x in &seen {
        let mut reads: Vec<ValueId> = Vec::new();
        for inst in &func.blocks[x].insts {
            reads.extend(func.reads(*inst));
        }
        if let Some(end) = &func.blocks[x].terminator {
            reads.extend(end.reads());
        }
        for value in reads {
            if let Some(defined) = facts.def_block[value.0 as usize]
                && facts.body.contains(&defined)
                && defined != x
            {
                return false;
            }
        }
    }
    true
}

/// Builds the clone, keeps only what inference says is a double, and answers the graph.
fn attempt(facts: &Facts, mut plan: Plan) -> Option<Func> {
    for _ in 0..4 {
        let built = build(facts, &plan)?;
        let after = infer(&built.func, facts.domain);
        let keep: Vec<(usize, bool)> = plan
            .params
            .iter()
            .copied()
            .filter(|(j, _)| {
                matches!(
                    after.of(built.vmap[&facts.header_params[*j]]),
                    Type::Int32 | Type::Double
                )
            })
            .collect();
        if keep.len() == plan.params.len() {
            // A HOISTED PEEK IS SOUND ONLY IF NOTHING IN THE CLONE CAN CHANGE THE PROPERTY.
            if !plan.peeks.is_empty() && !quiet(facts, &built, &after) {
                return None;
            }
            return rts_mir::verify(&built.func).ok().map(|()| built.func);
        }
        if keep.is_empty() {
            return None;
        }
        plan.sites.retain(|site| keep.iter().any(|(j, _)| *j == site.param));
        plan.params = keep;
    }
    None
}

/// Whether every instruction that stays in the clone neither writes nor runs the program's
/// code. The slow exits are not counted: control never comes back from them.
fn quiet(facts: &Facts, built: &Built, types: &rts_mir::infer::Types<Type>) -> bool {
    let func = &built.func;
    for &b in &built.clone_blocks {
        if built.slow_blocks.contains(&b) {
            continue;
        }
        for inst in &func.blocks[b].insts {
            let held = &func.insts[inst.0 as usize];
            match &held.op {
                Op::Const(_) => {}
                Op::Prim { prim, args } => {
                    let of_args: Vec<Type> = args.iter().map(|v| types.of(*v).clone()).collect();
                    let effect = facts.domain.effect_of(*prim, &of_args);
                    if effect.has(Effect::CALLS_USER)
                        || effect.has(Effect::WRITES)
                        || effect.has(Effect::SUSPENDS)
                    {
                        return false;
                    }
                }
                Op::Call { .. } | Op::Suspend { .. } | Op::Guard { .. } => return false,
            }
        }
    }
    true
}

fn mint(work: &mut Func) -> ValueId {
    let value = ValueId(work.values);
    work.values += 1;
    value
}

fn new_block(work: &mut Func) -> usize {
    work.blocks.push(Block {
        params: Vec::new(),
        insts: Vec::new(),
        terminator: None,
    });
    work.block_regions.push(None);
    work.blocks.len() - 1
}

fn emit(work: &mut Func, block: usize, result: ValueId, op: Op, effect: Effect, at: Position) {
    work.insts.push(Inst {
        op,
        result,
        at,
        effect,
    });
    let id = InstId(work.insts.len() as u32 - 1);
    work.blocks[block].insts.push(id);
}

/// The graph with the loop cloned.
struct Built {
    func: Func,
    /// Where each value of the loop went.
    vmap: BTreeMap<ValueId, ValueId>,
    /// The clone's blocks, including the halves of a split `+`.
    clone_blocks: Vec<usize>,
    /// The ones that leave for the original loop.
    slow_blocks: BTreeSet<usize>,
}

/// What the preheader tests, in order.
enum Test {
    /// The entry value of an accumulator.
    Param(usize),
    /// A value defined before the loop.
    Value(ValueId),
    /// A hoisted property read, by index into the plan.
    Peek(usize),
}

fn build(facts: &Facts, plan: &Plan) -> Option<Built> {
    let func = facts.func;
    let domain = facts.domain;
    let h = facts.h;
    let body = &facts.body;
    let mut work = func.clone();
    let pure = Effect::PURE;
    let number_test = domain.prim(JsPrim::NumberTest);
    let number_of = domain.prim(JsPrim::NumberOf);
    let number_peek = domain.prim(JsPrim::NumberPeek);
    let at = facts.at.clone();

    // THE PREHEADER CHAIN comes first, so its blocks are lowered before anything that reads
    // what they define.
    let mut tests: Vec<Test> = Vec::new();
    for (j, dynamic) in &plan.params {
        if *dynamic {
            tests.push(Test::Param(*j));
        }
    }
    for value in &plan.values {
        tests.push(Test::Value(*value));
    }
    for index in 0..plan.peeks.len() {
        tests.push(Test::Peek(index));
    }
    let chain: Vec<usize> = (0..=tests.len()).map(|_| new_block(&mut work)).collect();
    let q: Vec<ValueId> = func.blocks[h].params.iter().map(|_| mint(&mut work)).collect();
    work.blocks[chain[0]].params = q.clone();
    // The double each test proves, minted now because the clone reads them.
    let doubles: Vec<ValueId> = tests.iter().map(|_| mint(&mut work)).collect();
    let mut hoisted: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    for (test, double) in tests.iter().zip(&doubles) {
        match test {
            Test::Param(_) => {}
            Test::Value(value) => {
                hoisted.insert(*value, *double);
            }
            Test::Peek(index) => {
                hoisted.insert(plan.peeks[*index].result, *double);
            }
        }
    }
    let peeked: BTreeSet<InstId> = plan.peeks.iter().map(|peek| peek.read).collect();

    let mut bmap: BTreeMap<usize, usize> = BTreeMap::new();
    for &b in body {
        bmap.insert(b, new_block(&mut work));
    }
    let mut vmap: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    for &b in body {
        for param in &func.blocks[b].params {
            let fresh = mint(&mut work);
            vmap.insert(*param, fresh);
        }
        for inst in &func.blocks[b].insts {
            let result = func.insts[inst.0 as usize].result;
            let fresh = mint(&mut work);
            vmap.insert(result, fresh);
        }
    }
    let m = |v: ValueId| vmap.get(&v).copied().unwrap_or(v);
    // AN OPERAND OF A NUMERIC OPERATOR is its double where a test made it one.
    let operand = |numeric: bool, v: ValueId| match numeric {
        true => hoisted.get(&v).copied().unwrap_or_else(|| m(v)),
        false => m(v),
    };
    let to = |b: BlockId| match bmap.get(&(b.0 as usize)) {
        Some(clone) => BlockId(*clone as u32),
        None => b,
    };
    let remap = |end: &Terminator| -> Terminator {
        match end {
            Terminator::Jump { target, args } => Terminator::Jump {
                target: to(*target),
                args: args.iter().map(|v| m(*v)).collect(),
            },
            Terminator::Branch {
                condition,
                then_block,
                then_args,
                else_block,
                else_args,
            } => Terminator::Branch {
                condition: m(*condition),
                then_block: to(*then_block),
                then_args: then_args.iter().map(|v| m(*v)).collect(),
                else_block: to(*else_block),
                else_args: else_args.iter().map(|v| m(*v)).collect(),
            },
            Terminator::Return(value) => Terminator::Return(value.map(&m)),
            Terminator::Raise(value) => Terminator::Raise(m(*value)),
            other => other.clone(),
        }
    };

    let mut clone_blocks: Vec<usize> = bmap.values().copied().collect();
    let mut slow_blocks: BTreeSet<usize> = BTreeSet::new();
    for &b in body {
        let clone = bmap[&b];
        work.blocks[clone].params = func.blocks[b].params.iter().map(|v| m(*v)).collect();
        let site = plan.sites.iter().find(|site| site.block == b);
        let insts = func.blocks[b].insts.clone();
        for (index, id) in insts.iter().enumerate() {
            let held = &func.insts[id.0 as usize];
            if site.is_some() && index + 1 == insts.len() {
                break;
            }
            if peeked.contains(id) {
                continue;
            }
            let op = match &held.op {
                Op::Const(value) => Op::Const(*value),
                Op::Prim { prim, args } => {
                    let numeric = domain.meaning(*prim).is_some_and(numeric_use);
                    Op::Prim {
                        prim: *prim,
                        args: args.iter().map(|v| operand(numeric, *v)).collect(),
                    }
                }
                Op::Call {
                    callee,
                    receiver,
                    args,
                } => Op::Call {
                    callee: match callee {
                        Callee::Dynamic(value) => Callee::Dynamic(m(*value)),
                        other => *other,
                    },
                    receiver: receiver.map(&m),
                    args: args.iter().map(|v| m(*v)).collect(),
                },
                Op::Suspend { .. } | Op::Guard { .. } => return None,
            };
            emit(&mut work, clone, m(held.result), op, held.effect, held.at.clone());
        }
        let end = func.blocks[b].terminator.as_ref()?;
        let Some(site) = site else {
            work.blocks[clone].terminator = Some(remap(end));
            continue;
        };
        let last = *insts.last()?;
        let plus = &func.insts[last.0 as usize];
        let Op::Prim { prim, args } = &plus.op else {
            return None;
        };
        let tested_value = m(site.x);
        let test = mint(&mut work);
        emit(
            &mut work,
            clone,
            test,
            Op::Prim {
                prim: number_test,
                args: vec![tested_value],
            },
            pure,
            plus.at.clone(),
        );
        let fast = new_block(&mut work);
        let slow = new_block(&mut work);
        clone_blocks.push(fast);
        clone_blocks.push(slow);
        slow_blocks.insert(slow);
        work.blocks[clone].terminator = Some(Terminator::Branch {
            condition: test,
            then_block: BlockId(fast as u32),
            then_args: Vec::new(),
            else_block: BlockId(slow as u32),
            else_args: Vec::new(),
        });
        // THE FAST SIDE: the operand is a number, so it is read as a double and the `+` is
        // a double add whose result is the clone's own value.
        let as_double = mint(&mut work);
        emit(
            &mut work,
            fast,
            as_double,
            Op::Prim {
                prim: number_of,
                args: vec![tested_value, test],
            },
            pure,
            plus.at.clone(),
        );
        let fast_args: Vec<ValueId> = args
            .iter()
            .map(|v| if *v == site.x { as_double } else { operand(true, *v) })
            .collect();
        emit(
            &mut work,
            fast,
            m(plus.result),
            Op::Prim {
                prim: *prim,
                args: fast_args,
            },
            pure,
            plus.at.clone(),
        );
        work.blocks[fast].terminator = Some(remap(end));
        // THE SLOW SIDE: the original `+`, once, in the original order, and then the
        // ORIGINAL loop from the point after it.
        let generic = mint(&mut work);
        emit(
            &mut work,
            slow,
            generic,
            Op::Prim {
                prim: *prim,
                args: args.iter().map(|v| operand(true, *v)).collect(),
            },
            plus.effect,
            plus.at.clone(),
        );
        let Terminator::Jump { target, args: out } = end else {
            return None;
        };
        work.blocks[slow].terminator = Some(Terminator::Jump {
            target: *target,
            args: out
                .iter()
                .map(|v| if *v == plus.result { generic } else { m(*v) })
                .collect(),
        });
    }

    // THE PREHEADER. Each test branches to the next on a number and to the ORIGINAL header
    // on anything else; the last block turns every passed test into its double and enters
    // the clone.
    let header = BlockId(h as u32);
    let mut verdicts: Vec<ValueId> = Vec::new();
    let mut proved_args: Vec<ValueId> = Vec::new();
    for (step, test) in tests.iter().enumerate() {
        let verdict = mint(&mut work);
        let (prim, args) = match test {
            Test::Param(j) => (number_test, vec![q[*j]]),
            Test::Value(value) => (number_test, vec![*value]),
            Test::Peek(index) => {
                let peek = &plan.peeks[*index];
                let key = mint(&mut work);
                emit(
                    &mut work,
                    chain[step],
                    key,
                    Op::Const(Const::Declared(peek.key)),
                    pure,
                    at.clone(),
                );
                (number_peek, vec![peek.object, key])
            }
        };
        proved_args.push(args[0]);
        emit(
            &mut work,
            chain[step],
            verdict,
            Op::Prim { prim, args },
            match test {
                Test::Peek(_) => Effect::READS,
                _ => pure,
            },
            at.clone(),
        );
        work.blocks[chain[step]].terminator = Some(Terminator::Branch {
            condition: verdict,
            then_block: BlockId(chain[step + 1] as u32),
            then_args: Vec::new(),
            else_block: header,
            else_args: q.clone(),
        });
        verdicts.push(verdict);
    }
    let last = *chain.last()?;
    let mut entering = q.clone();
    for (step, test) in tests.iter().enumerate() {
        emit(
            &mut work,
            last,
            doubles[step],
            Op::Prim {
                prim: number_of,
                args: vec![proved_args[step], verdicts[step]],
            },
            pure,
            at.clone(),
        );
        if let Test::Param(j) = test {
            entering[*j] = doubles[step];
        }
    }
    work.blocks[last].terminator = Some(Terminator::Jump {
        target: BlockId(bmap[&h] as u32),
        args: entering,
    });
    match work.blocks[facts.entry_block].terminator.as_mut()? {
        Terminator::Jump { target, .. } => {
            if target.0 as usize == h {
                *target = BlockId(chain[0] as u32);
            }
        }
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => {
            if then_block.0 as usize == h {
                *then_block = BlockId(chain[0] as u32);
            }
            if else_block.0 as usize == h {
                *else_block = BlockId(chain[0] as u32);
            }
        }
        _ => return None,
    }
    Some(Built {
        func: work,
        vmap,
        clone_blocks,
        slow_blocks,
    })
}

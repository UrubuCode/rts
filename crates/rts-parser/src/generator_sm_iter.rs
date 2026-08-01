//! `for…of`, `for…in` and `do…while` as STATES of the generator state machine.
//!
//! ## Why these three were missing, and what it cost
//!
//! [`crate::generator_sm`]'s `lower_stmt` modelled `while`, `for(;;)`, `if`,
//! `try`, `break`/`continue`, `return` and the `yield` forms — and returned
//! `None` for everything else, which aborts the build and falls back to the
//! EAGER buffer (`generator_desugar`). The eager buffer can only express a
//! `yield` in STATEMENT position: it rewrites it to `__gen_buf.push(v)`. A
//! `yield` in VALUE position (`const a = yield x`, the value a later
//! `.next(v)` sends back) survives that rewrite and reaches the lowering as a
//! raw node, dying with `expression raw/unrecognized: Yield(…)`.
//!
//! That is exactly the failure a real bundle hits: `for (const x of src) { const
//! a = yield f(x); }` is ordinary transpiled JS, and every one of those bailed.
//! Modelling the three remaining loop forms removes the whole family.
//!
//! ## How a `for…of` becomes states
//!
//! The machine cannot keep a live cursor in a Rust-side structure — everything
//! that survives a suspension must live in the generator FRAME, and a frame slot
//! holds one PolyValue word. So the loop rides the runtime's own lazy
//! for-of protocol, whose cursor IS a single word:
//!
//! ```text
//!   cur:    __ito_N = ITER_OPEN(SRC);            goto header
//!   header: __itv_N = ITER_NEXT(__ito_N)
//!           if (ITER_DONE(__itv_N)) goto after else goto body
//!   body:   <binding> = __itv_N; <body…>;        goto header
//!   brk:    ITER_CLOSE(__ito_N);                 goto after
//!   after:
//! ```
//!
//! `ITER_OPEN`/`NEXT`/`CLOSE` are the SAME `__rtsadp_iter_*` trampolines the
//! ordinary (non-generator) for-of lowering drives, so an array, a string, a
//! Set/Map and a live custom `[Symbol.iterator]` all iterate identically inside
//! and outside a generator, and a non-iterable source throws the same TypeError.
//! Reusing the eager `yield*` helpers (`GEN_DELEGATE_*`) would NOT have done
//! that: they recognize only GenState/Vec/String and report a Map or a custom
//! iterator as immediately DONE — a silently empty loop, the exact class of
//! wrong answer the honesty floor forbids.
//!
//! `break` lands on its OWN state that runs `ITER_CLOSE` (spec IteratorClose)
//! before reaching `after`; normal exhaustion goes straight to `after`, so a
//! completed iterator is never asked to `return()`.
//!
//! `for…in` is the same walk over the key array `FOR_IN_KEYS(obj)` produces —
//! the same trampoline the ordinary for-in lowering uses (own enumerable keys;
//! a non-object source yields `[]`, so the loop runs zero times, which is JS).
//!
//! ## What is deliberately NOT modelled (bails, never guesses)
//!
//! * a destructuring binding (`for (const [a,b] of …)`) — the frame stores ONE
//!   word per slot and the machine has no destructuring lowering;
//! * `for await (… of …)` — the async-generator driver's business;
//! * a `yield` inside the SOURCE expression (`for (const x of (yield s))`) —
//!   suspending mid-evaluation of the header is not a state boundary the
//!   machine has;
//! * for-in's mid-iteration `delete` guard (the ordinary lowering re-checks
//!   `[[HasProperty]]` per key; the machine walks the key snapshot).
//!
//! Each of those returns `None`, which aborts the build and keeps TODAY's
//! behaviour — never a silently different loop.

use swc_ecma_ast::{DoWhileStmt, Expr, ForHead, ForInStmt, ForOfStmt, Pat, Stmt};

use crate::generator_sm::{
    G, SmBuilder, assign_stmt, block_stmts, call, call_stmt, expr_has_yield, ident_expr,
};
use crate::generator_sm_loops::{LoopTarget, TargetKind};

/// `__RTS_ITER_OPEN(src)` — open the lazy for-of cursor (a PolyValue word).
const ITER_OPEN: &str = "__RTS_ITER_OPEN";
/// `__RTS_ITER_NEXT(cursor)` — next value word, or the EMPTY sentinel.
const ITER_NEXT: &str = "__RTS_ITER_NEXT";
/// `__RTS_ITER_DONE(word)` — 1 when the word is the EMPTY sentinel.
const ITER_DONE: &str = "__RTS_ITER_DONE";
/// `__RTS_ITER_CLOSE(cursor)` — spec IteratorClose on early exit.
const ITER_CLOSE: &str = "__RTS_ITER_CLOSE";
/// `__RTS_FOR_IN_KEYS(obj)` — the own-enumerable key array for `for…in`.
const FOR_IN_KEYS: &str = "__RTS_FOR_IN_KEYS";
/// `__RTS_GEN_DELEGATE_START(src)` — normalize a `yield*` source to an iterator.
const DELEGATE_START: &str = "__RTS_GEN_DELEGATE_START";
/// `__RTS_GEN_DELEGATE_NEXT(it)` — one step of the delegated iterator.
const DELEGATE_NEXT: &str = "__RTS_GEN_DELEGATE_NEXT";
/// `__RTS_GEN_DELEGATE_DONE(it)` — 1 when the delegate is exhausted.
const DELEGATE_DONE: &str = "__RTS_GEN_DELEGATE_DONE";
/// `__RTS_GEN_DELEGATE_RET(it)` — the delegate's `return` value.
const DELEGATE_RET: &str = "__RTS_GEN_DELEGATE_RET";

impl SmBuilder {
    /// `for (<binding> of <src>) { … }`.
    pub(crate) fn lower_for_of(&mut self, f: &ForOfStmt, cur: usize) -> Option<usize> {
        if f.is_await {
            return None; // `for await` — the async-generator driver's business.
        }
        let binding = head_binding(&f.left)?;
        if expr_has_yield(&f.right) {
            return None;
        }
        self.lower_iter_walk(&binding, (*f.right).clone(), &f.body, cur)
    }

    /// `for (<binding> in <obj>) { … }` — the same walk over the key array.
    pub(crate) fn lower_for_in(&mut self, f: &ForInStmt, cur: usize) -> Option<usize> {
        let binding = head_binding(&f.left)?;
        if expr_has_yield(&f.right) {
            return None;
        }
        let keys = call(FOR_IN_KEYS, vec![(*f.right).clone()]);
        self.lower_iter_walk(&binding, keys, &f.body, cur)
    }

    /// The shared cursor walk both for-of and for-in compile to. `src` is the
    /// expression handed to `ITER_OPEN` (the iterable itself, or the key array).
    fn lower_iter_walk(
        &mut self,
        binding: &str,
        src: Expr,
        body: &Stmt,
        cur: usize,
    ) -> Option<usize> {
        let label = self.pending_label.take();
        let uid = self.tmp_counter;
        self.tmp_counter += 1;
        let cursor = format!("__ito_{uid}");
        let value = format!("__itv_{uid}");
        self.intern_local(&cursor);
        self.intern_local(&value);
        self.intern_local(binding);

        // cur: __ito = ITER_OPEN(src);  goto header
        self.push_stmt(cur, assign_stmt(&cursor, call(ITER_OPEN, vec![src])));
        let header = self.new_state();
        self.set_goto(cur, header);
        let body_entry = self.new_state();
        let brk = self.new_state();
        let after = self.new_state();

        // header: __itv = ITER_NEXT(__ito); if (ITER_DONE(__itv)) after else body
        self.push_stmt(
            header,
            assign_stmt(&value, call(ITER_NEXT, vec![ident_expr(&cursor)])),
        );
        self.set_cond(
            header,
            call(ITER_DONE, vec![ident_expr(&value)]),
            after,
            body_entry,
        );

        // body: <binding> = __itv; … ; goto header
        self.push_stmt(body_entry, assign_stmt(binding, ident_expr(&value)));
        let stmts = block_stmts(body)?;
        self.loop_stack.push(LoopTarget {
            kind: TargetKind::Loop,
            label,
            brk,
            cont: header,
            try_depth: self.try_depth,
        });
        let body_exit = self.lower_seq(stmts, body_entry);
        self.loop_stack.pop();
        let body_exit = body_exit?;
        self.set_goto(body_exit, header);

        // brk: IteratorClose, then leave. Normal exhaustion skips this state, so
        // a completed iterator is never asked to `return()`.
        self.push_stmt(brk, call_stmt(call(ITER_CLOSE, vec![ident_expr(&cursor)])));
        self.set_goto(brk, after);
        Some(after)
    }

    /// `yield* SRC` — LAZY delegation, in BOTH positions.
    ///
    /// Instead of materializing SRC (eager, and unbounded for an infinite
    /// delegate), the loop normalizes SRC into an iterator and re-yields one
    /// value at a time, suspending in between. The delegate's iteration state
    /// lives runtime-side keyed by handle, so only the HANDLE enters the frame
    /// and the delegation survives the outer generator's suspension.
    ///
    /// ```text
    ///   cur:    __dlg = DELEGATE_START(SRC);           goto header
    ///   header: __dlv = DELEGATE_NEXT(__dlg)
    ///           if (DELEGATE_DONE(__dlg)) goto after else goto body
    ///   body:   yield __dlv;                           goto header
    ///   after:  [bind = DELEGATE_RET(__dlg)]
    /// ```
    ///
    /// `bind` is what separates the two positions. In STATEMENT position
    /// (`yield* g();`) the delegate's return value is discarded and `bind` is
    /// `None`. In VALUE position (`const r = yield* g()`, which the normalizer
    /// reduces to this form) `bind` names the local that receives the
    /// delegate's `return` — which the spec defines as the `value` of the
    /// source's `done:true` result, and which is NOT the last yielded value.
    pub(crate) fn lower_delegate(
        &mut self,
        src: Expr,
        cur: usize,
        bind: Option<&str>,
    ) -> Option<usize> {
        let uid = self.tmp_counter;
        self.tmp_counter += 1;
        let dlg = format!("__dlg_{uid}");
        let dval = format!("__dlv_{uid}");
        self.intern_local(&dlg);
        self.intern_local(&dval);

        self.push_stmt(cur, assign_stmt(&dlg, call(DELEGATE_START, vec![src])));
        let header = self.new_state();
        self.set_goto(cur, header);
        let body = self.new_state();
        let after = self.new_state();

        self.push_stmt(
            header,
            assign_stmt(
                &dval,
                call(DELEGATE_NEXT, vec![ident_expr(G), ident_expr(&dlg)]),
            ),
        );
        self.set_cond(
            header,
            call(DELEGATE_DONE, vec![ident_expr(&dlg)]),
            after,
            body,
        );
        // body: re-yield the delegate's value, then back to the header.
        self.set_yield(body, ident_expr(&dval), header);

        if let Some(name) = bind {
            self.intern_local(name);
            self.push_stmt(
                after,
                assign_stmt(name, call(DELEGATE_RET, vec![ident_expr(&dlg)])),
            );
        }
        Some(after)
    }

    /// `do { … } while (test);` — the body runs BEFORE the first test, so the
    /// entry edge goes to the body and the test state closes the cycle.
    pub(crate) fn lower_do_while(&mut self, d: &DoWhileStmt, cur: usize) -> Option<usize> {
        let label = self.pending_label.take();
        if expr_has_yield(&d.test) {
            return None;
        }
        let body_entry = self.new_state();
        // The test state is created BEFORE the body so a `continue` can target
        // it — `continue` in a do-while re-tests, it does not restart the body.
        let test_state = self.new_state();
        let after = self.new_state();
        self.set_goto(cur, body_entry);

        let stmts = block_stmts(&d.body)?;
        self.loop_stack.push(LoopTarget {
            kind: TargetKind::Loop,
            label,
            brk: after,
            cont: test_state,
            try_depth: self.try_depth,
        });
        let body_exit = self.lower_seq(stmts, body_entry);
        self.loop_stack.pop();
        let body_exit = body_exit?;
        self.set_goto(body_exit, test_state);
        self.set_cond(test_state, (*d.test).clone(), body_entry, after);
        Some(after)
    }
}

/// The single NAME a `for (… of/in …)` head binds. `None` for a destructuring
/// pattern or a `using` declaration — the frame stores one word per slot, so
/// there is nothing to bind them to (honest bail, see the module docs).
fn head_binding(head: &ForHead) -> Option<String> {
    match head {
        ForHead::VarDecl(vd) => {
            if vd.decls.len() != 1 {
                return None;
            }
            match &vd.decls[0].name {
                Pat::Ident(id) => Some(id.id.sym.to_string()),
                _ => None,
            }
        }
        // `for (x of …)` over an ALREADY-declared binding.
        ForHead::Pat(p) => match p.as_ref() {
            Pat::Ident(id) => Some(id.id.sym.to_string()),
            _ => None,
        },
        ForHead::UsingDecl(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::head_binding;
    use swc_ecma_ast::{ForHead, Ident, Pat};

    fn id(n: &str) -> Ident {
        Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: n.into(),
            optional: false,
        }
    }

    #[test]
    fn plain_ident_head_binds() {
        let h = ForHead::Pat(Box::new(Pat::Ident(id("x").into())));
        assert_eq!(head_binding(&h).as_deref(), Some("x"));
    }

    #[test]
    fn destructuring_head_refuses() {
        let h = ForHead::Pat(Box::new(Pat::Array(swc_ecma_ast::ArrayPat {
            span: Default::default(),
            elems: Vec::new(),
            optional: false,
            type_ann: None,
        })));
        assert_eq!(head_binding(&h), None);
    }
}

//! A large module body, compiled as several functions instead of one.
//!
//! # What this is for, measured
//!
//! A module's top level is one function, and a file with hundreds of top-level
//! statements makes it a very large one. `bench/analytic.ts` — 277 `bench(…)`
//! calls at the top level — put its entry at 6 731 instructions against 218 for
//! the next largest function, and that ONE function was the whole critical path
//! of compilation: 293 ms of machine-compile CPU over 396 functions took 183 ms
//! of wall on sixteen threads, because about 175 of them were this function
//! alone, where the backtracking register allocator is superlinear
//! (`docs/codegen/plan-2026-09-27.md`, release, 2026-09-27). Split by hand into
//! functions of eight to thirty-two statements, the same phase took about 50 ms
//! — and the size of a piece made no difference, so the number below is a
//! convenience and not a tuning.
//!
//! # What is moved, and why only that
//!
//! A run of consecutive top-level statements becomes
//! `function <module N>() { … }` followed by a call of it, in place. Only a
//! statement that DECLARES NOTHING at the module's level is moved, because a
//! declaration inside a function is that function's: a `const`, `let`, `class`,
//! `function`, `var`, `using` or `with` stays where it is and ends the run. What
//! is left — calls, assignments, loops, `if`, `try`, `switch` — reads and writes
//! the module's bindings by name, and a name a nested function mentions already
//! lives in the module's environment rather than in a register, which is the
//! same storage whether the reader is a function the program wrote or one of
//! these.
//!
//! # What refuses a statement, and what refuses the body
//!
//! - A `var` anywhere inside it (`for (var i …)`, a `var` in a block): it is
//!   hoisted to the function it is written in, which would stop being the
//!   module.
//! - `await`: the piece would have to be `async` and the entry would wait on
//!   its promise, which adds ticks a program can observe in the order of its
//!   events.
//! - `this` outside an ordinary function, and `arguments` anywhere: both name
//!   something about the ACTIVATION, and the activation would change.
//! - `return`, which a host-wrapped script may hold at its top level.
//! - A function written at exactly the position the piece would take: the MIR
//!   stage keys a function by where it was written.
//!
//! The whole body is left alone when it is non-strict (a sloppy function call
//! gives `this` the global object, and block-level functions hoist), when it
//! mentions `eval` (direct `eval` reads the scope it is written in), or when it
//! is not large enough to be worth a call per piece.
//!
//! # What a program can see
//!
//! One thing: a stack trace taken under a moved statement has one more frame,
//! named `<module N>`. The name cannot be written by a program, so it cannot
//! collide with one.
//!
//! Rejected: splitting inside the emitter, one `FuncBuilder` per piece over the
//! same scope. It would also take the declarations, which this cannot — and it
//! is a second way to open a function in `function.rs`, for a gain the
//! measurement does not ask for: the declarations of a large module are a small
//! part of its instructions, and the calls are the rest.

use rts_cranelift::fault::Position;

use super::Ctx;
use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use crate::syntax::{Expr, ExprKind, Function, FunctionBody, Stmt, StmtKind};

/// How many statements one piece holds.
const PIECE: usize = 16;

/// The fewest movable statements a body has for the split to happen at all.
const WORTH_IT: usize = 48;

/// The shortest run worth a function and a call.
const SHORTEST: usize = 4;

/// `body` with its runs of movable statements moved into functions, or `body`
/// unchanged where the split does not apply.
pub(super) fn split(body: Vec<Stmt>, ctx: &mut Ctx) -> Vec<Stmt> {
    // `RTS_CHUNK=0` leaves every body whole, for measuring the split against the
    // same binary — the switch `RTS_MIR` is for the other door.
    if ctx.sloppy || std::env::var_os("RTS_CHUNK").is_some_and(|held| held == "0") {
        return body;
    }
    let eval = ctx.names.intern("eval");
    let arguments = ctx.names.intern("arguments");
    if super::capture::mentions(&body, eval) {
        return body;
    }
    let movable: Vec<bool> = body
        .iter()
        .map(|statement| is_movable(statement, arguments))
        .collect();
    if movable.iter().filter(|held| **held).count() < WORTH_IT {
        return body;
    }

    let mut out = Vec::with_capacity(body.len());
    let mut run: Vec<Stmt> = Vec::new();
    let mut made = 0usize;
    for (statement, movable) in body.into_iter().zip(movable) {
        if movable {
            run.push(statement);
            if run.len() == PIECE {
                close(&mut run, &mut out, &mut made, ctx);
            }
        } else {
            close(&mut run, &mut out, &mut made, ctx);
            out.push(statement);
        }
    }
    close(&mut run, &mut out, &mut made, ctx);
    out
}

/// Ends the run in hand: a function and its call where it is long enough, the
/// statements as they were where it is not.
fn close(run: &mut Vec<Stmt>, out: &mut Vec<Stmt>, made: &mut usize, ctx: &mut Ctx) {
    if run.len() < SHORTEST {
        out.append(run);
        return;
    }
    let at = run[0].at;
    let name = ctx.names.intern(&format!("<module {made}>"));
    *made += 1;
    let function = Function {
        name: Some(name),
        parameters: Vec::new(),
        rest_parameter: None,
        directives: Vec::new(),
        body: FunctionBody::Block(std::mem::take(run)),
        returns: None,
        captures_this: false,
        is_async: false,
        is_generator: false,
        advertised_length: None,
        at,
    };
    out.push(Stmt {
        kind: StmtKind::Function(Box::new(function)),
        at,
    });
    out.push(Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Call {
                callee: Box::new(Expr {
                    kind: ExprKind::Ident(name),
                    at,
                }),
                arguments: Vec::new(),
                optional: false,
            },
            at,
        }),
        at,
    });
}

/// Whether a top-level statement may run inside a function of its own — see
/// the module header for each refusal.
fn is_movable(statement: &Stmt, arguments: crate::names::Name) -> bool {
    match &statement.kind {
        StmtKind::Declare { .. }
        | StmtKind::Using { .. }
        | StmtKind::Function(_)
        | StmtKind::Class(_)
        | StmtKind::With { .. }
        | StmtKind::Return(_)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::Empty
        | StmtKind::Debugger => return false,
        _ => {}
    }
    let one = std::slice::from_ref(statement);
    if !super::function::vars_of(one).is_empty()
        || super::suspends::body_suspends(one)
        || super::capture::mentions(one, arguments)
    {
        return false;
    }
    let mut probe = Probe {
        at: statement.at,
        refused: false,
    };
    probe.statement(statement, Own { this: true, flow: true });
    !probe.refused
}

/// What of the statement's own activation the code being walked still belongs
/// to. The two differ at an ARROW: it reads the enclosing `this`, and its
/// `return` is its own — the first draft carried one flag for both, read a
/// `return` inside an arrow as the module's, and admitted 3 statements of 315
/// in a file where every row passes an arrow. The split existed and never ran,
/// and the clock is what said so.
#[derive(Clone, Copy)]
struct Own {
    /// `this` here is the statement's: true until an ordinary function.
    this: bool,
    /// `return` here leaves the statement's activation: true until ANY function.
    flow: bool,
}

/// Looks for the two things no existing walk answers: `this` or `return` read
/// by the statement itself, and a function written at the position the piece
/// would take.
struct Probe {
    at: Position,
    refused: bool,
}

impl Probe {
    fn statement(&mut self, statement: &Stmt, own: Own) {
        if self.refused {
            return;
        }
        if own.flow && matches!(statement.kind, StmtKind::Return(_)) {
            self.refused = true;
            return;
        }
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.statement(inner, own),
            StmtChild::Expr(value) => self.expression(value, own),
            StmtChild::Binding(binding) => {
                if let Some(value) = &binding.value {
                    self.expression(value, own);
                }
            }
            StmtChild::Catch(clause) => {
                for inner in &clause.body {
                    self.statement(inner, own);
                }
            }
            StmtChild::Function(inner) => self.function(inner, own),
            // A class's methods are functions of their own and its `this` is an
            // instance's; what is left is a heritage or a computed key reading
            // the module's `this`, which is `undefined` in strict code on both
            // sides of the move.
            StmtChild::Class(_) => {}
        });
    }

    fn expression(&mut self, value: &Expr, own: Own) {
        if self.refused {
            return;
        }
        if own.this && matches!(value.kind, ExprKind::This) {
            self.refused = true;
            return;
        }
        walk_expr(value, &mut |child| match child {
            Child::Expr(inner) => self.expression(inner, own),
            Child::Function(inner) => self.function(inner, own),
            Child::Class(_) => {}
        });
    }

    fn function(&mut self, function: &Function, own: Own) {
        if function.at == self.at {
            self.refused = true;
            return;
        }
        // An arrow reads the enclosing `this`; an ordinary function has its own.
        // A `return` is the function's in either.
        let own = Own {
            this: own.this && function.captures_this,
            flow: false,
        };
        match &function.body {
            FunctionBody::Block(body) => {
                for inner in body {
                    self.statement(inner, own);
                }
            }
            FunctionBody::Expression(value) => self.expression(value, own),
        }
    }
}

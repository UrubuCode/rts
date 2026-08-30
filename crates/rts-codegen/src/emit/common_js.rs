//! `module`, `exports`, `require`, `__filename` and `__dirname`, as ordinary
//! bindings of a module's own scope.
//!
//! # Why every module gets them, rather than a file being one kind or the other
//!
//! Node decides between CommonJS and ES modules per file — by extension, and by
//! the nearest `package.json` — and a program that guesses wrong is refused
//! rather than run. That split exists because the two systems disagree about
//! *evaluation*: an ES module is linked and hoisted before it runs, a CommonJS
//! one executes on first `require`.
//!
//! This engine has already answered that question a third way, and the answer
//! predates CommonJS being here at all: `rts-host`'s `graph.rs` collects the
//! whole graph up front and emits every file into ONE compilation, dependencies
//! first. So there is no second evaluation model to choose between — the thing
//! the split protects against does not exist here.
//!
//! What is left of the difference is a naming question: are these five names in
//! scope, and where do a module's exports come from. Both can be true of one
//! file, so both are. `import` and `require` may appear in the same module, and
//! `exports.a = 1` beside an `export const b = 2` publishes both.
//!
//! # What that costs, and why it is nothing
//!
//! A name is bound only when the body mentions it ([`mentioned`]), so a
//! module that writes neither pays no instruction and gets no binding — which
//! also means nothing shadows a program's own `const require = …`. A module
//! that DOES declare one of the five keeps its own declaration: the binding
//! below would be a duplicate, and the program's is the one it wrote.
//!
//! # The divergence, stated
//!
//! `require` here cannot LOAD. It reads the table the graph filled, which is
//! the same wall `rts-node`'s `createRequire` documents: a file outside the
//! program's graph would be compiled into a region of its own, and its exports
//! would be cells this program cannot touch. So `require` of something the
//! graph never saw raises, rather than reading it from disk.
//!
//! And a required module has already RUN — with the rest of the graph, before
//! the entry — where Node runs it at the first `require`. A module whose body
//! is behind `if (condition) require("./x")` runs anyway. That is the same
//! divergence `dynamic_specifiers` already states for `import()`, and it is
//! stated again here rather than inherited quietly.

use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;

use super::{Ctx, EmitResult, Scope};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::Stmt;

/// The five names a CommonJS module body may read without declaring them.
pub const NAMES: [&str; 5] = ["module", "exports", "require", "__filename", "__dirname"];

/// Which of the five a body mentions, interned.
///
/// The mention walk is [`super::capture::mentions`] — the same one that decides
/// whether a function binds `arguments`, rather than a second walk that could
/// come to disagree with it about what a mention is.
pub fn mentioned(body: &[Stmt], ctx: &mut Ctx) -> Vec<Name> {
    NAMES
        .iter()
        .map(|text| ctx.names.intern(text))
        .filter(|name| super::capture::mentions(body, *name))
        .collect()
}

/// Binds what the body mentions, at the top of a module.
///
/// # Why `exports` and `module.exports` are the same object to begin with
///
/// Because that is what CommonJS is: `exports` is a local alias for whatever
/// `module.exports` held at entry, and the two part company the moment a body
/// assigns `module.exports = …`. Binding them to one object here reproduces
/// that exactly — including the trap the alias is famous for, where
/// `exports = {}` rebinds a local and publishes nothing.
///
/// A name the program itself declares is left alone: `declared` carries what the
/// body's own declarations bind, and a second binding of one of those would be
/// this emitter shadowing a program's `const require = createRequire(…)` with
/// its own.
pub fn emit_prologue(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    body: &[Stmt],
    specifier: &str,
    filename: &str,
    dirname: &str,
    declared: &[Name],
) -> EmitResult<()> {
    let wanted = mentioned(body, ctx);
    if wanted.is_empty() {
        return Ok(());
    }
    let named = |ctx: &mut Ctx, text: &str| ctx.names.intern(text);
    let exports_name = named(ctx, "exports");
    let module_name = named(ctx, "module");

    // One object for both names, and it is made when EITHER is mentioned: a
    // body that only writes `exports.a` still needs the object the epilogue
    // publishes, and one that only writes `module.exports` needs `module` to
    // have something in it.
    let wants_exports = wanted.contains(&exports_name) || wanted.contains(&module_name);
    let exports_object = match wants_exports {
        false => None,
        true => {
            let size = integer(builder, 1);
            let object = super::expr::call(builder, ctx, RuntimeOp::ObjectNew, &[size])?[0];
            if wanted.contains(&exports_name) && !declared.contains(&exports_name) {
                super::binding::declare(builder, scope, ctx, exports_name, object)?;
            }
            if wanted.contains(&module_name) && !declared.contains(&module_name) {
                let size = integer(builder, 1);
                let holder = super::expr::call(builder, ctx, RuntimeOp::ObjectNew, &[size])?[0];
                let key = key_of(builder, ctx, "exports");
                let estrito = super::property::estrito(builder, ctx);
                super::expr::call(builder, ctx, RuntimeOp::SetProperty, &[holder, key, object, estrito])?;
                super::binding::declare(builder, scope, ctx, module_name, holder)?;
            }
            Some(object)
        }
    };

    // Published HERE as well as after the body, and the reason is `return`.
    //
    // A CommonJS module may exit early — Node wraps one in a function, so a
    // top-level `return` is an early exit rather than a syntax error, and this
    // engine parses it that way (`parse/mod.rs`). A body that returns has no
    // reachable point left for the epilogue to emit into, so a module whose
    // last act is `if (!supported) return;` would publish NOTHING and every
    // `require` of it would answer an empty namespace.
    //
    // Publishing the object at entry fixes that for the shape the corpus
    // writes, because `exports.a = …` MUTATES this object: whatever the body
    // put on it is there afterwards, published or not. The divergence it does
    // not fix is stated rather than papered over — a module that REPLACES
    // `module.exports` and then returns early publishes the original object,
    // since nothing runs to notice the replacement.
    // The value itself and not a read of the binding: a body that mentions only
    // `module` never binds `exports`, and reading a name that is not there would
    // refuse the module rather than publish it.
    if let Some(object) = exports_object {
        let own = ctx.literal(specifier);
        let own = integer(builder, u64::from(own));
        super::expr::call(builder, ctx, RuntimeOp::ModulePublishCommon, &[own, object])?;
    }

    let require_name = named(ctx, "require");
    if wanted.contains(&require_name) && !declared.contains(&require_name) {
        let own = ctx.literal(specifier);
        let own = integer(builder, u64::from(own));
        let made = super::expr::call(builder, ctx, RuntimeOp::RequireFunction, &[own])?[0];
        super::binding::declare(builder, scope, ctx, require_name, made)?;
    }

    // Two strings the HOST resolved. Not computed here from the specifier:
    // where a file is and what its directory is called are the host's
    // questions, and this crate deciding them would be a path resolver in the
    // language layer — the second answer `graph.rs` exists to prevent.
    for (text, value) in [("__filename", filename), ("__dirname", dirname)] {
        let name = named(ctx, text);
        if !wanted.contains(&name) || declared.contains(&name) {
            continue;
        }
        let held = super::expr::string_literal(builder, ctx, value)?;
        super::binding::declare(builder, scope, ctx, name, held)?;
    }
    Ok(())
}

/// Publishes `module.exports` after the body has run.
///
/// Emitted only for a body that mentions `module` or `exports`, which is what
/// keeps a module's `common` entry absent — and that absence is what makes
/// `require` of an ES module answer its namespace. See `rts-core`'s
/// `entry::common_js` for the other side of that.
///
/// Reads `module.exports` rather than the `exports` local, because the body may
/// have replaced it. A module that assigned `module.exports = f` and one that
/// filled `exports` both end with the right value in the same place.
pub fn emit_epilogue(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    body: &[Stmt],
    specifier: &str,
    declared: &[Name],
) -> EmitResult<()> {
    let wanted = mentioned(body, ctx);
    let module_name = ctx.names.intern("module");
    let exports_name = ctx.names.intern("exports");
    // A module that DECLARED its own `module`/`exports` publishes nothing: those
    // are the program's own variables and mean nothing to a module system.
    let mine = |name: Name| wanted.contains(&name) && !declared.contains(&name);
    let value = if mine(module_name) {
        let holder = super::binding::read(builder, scope, ctx, module_name)?;
        let key = key_of(builder, ctx, "exports");
        super::expr::call(builder, ctx, RuntimeOp::GetProperty, &[holder, key])?[0]
    } else if mine(exports_name) {
        super::binding::read(builder, scope, ctx, exports_name)?
    } else {
        return Ok(());
    };
    let own = ctx.literal(specifier);
    let own = integer(builder, u64::from(own));
    super::expr::call(builder, ctx, RuntimeOp::ModulePublishCommon, &[own, value])?;
    Ok(())
}

/// A property key as the number the shape tree is keyed by.
fn key_of(builder: &mut FuncBuilder, ctx: &mut Ctx, text: &str) -> ValueId {
    let name = ctx.names.intern(text);
    integer(builder, u64::from(ctx.key_of(name)))
}

/// An integer operand: a literal's number, a key, or a count.
///
/// Not a tagged value, for the reason `module.rs` gives about the same operand:
/// none of the three is something the program could compute, and emitting one
/// tagged would claim it could.
fn integer(builder: &mut FuncBuilder, bits: u64) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(bits),
    });
    builder.use_const(id)
}

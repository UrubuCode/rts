//! `import`, as statements the body already knows how to run.
//!
//! # Why an import becomes a declaration
//!
//! Because that is what it is, everywhere except in when it happens.
//! `import { test } from "rts:test"` introduces `test` in the module's scope,
//! and the only thing that distinguishes it from `const test = …` is that the
//! right-hand side is not written in the program.
//!
//! So this synthesises the missing right-hand side — a call to the runtime
//! carrying which specifier and which name — and hands the declarations to the
//! ordinary body emitter. Nothing downstream learns what a module is: a
//! captured import is captured by the machinery that captures a `const`, and a
//! read of one is a read of a local.
//!
//! # What this does NOT do
//!
//! Resolve a specifier, read a file, order an evaluation, or link a cycle. The
//! runtime answers a specifier from a table the host filled, so `"rts:test"`
//! works and `"./other.ts"` answers `undefined` — visibly, rather than by being
//! quietly bound to something else. A real module system is the thing that
//! replaces this, and it will replace the runtime side rather than this file:
//! what an import MEANS for a scope is decided here and does not change.
//!
//! # What an `export` is, on the other side of the same table
//!
//! A write. An import READS the specifier table, so an export WRITES it — under
//! the module's own specifier, which the host resolves and hands down. That way
//! one table decides what a specifier resolves to, for a host-provided module
//! and a compiled one alike; a second mechanism holding compiled modules'
//! exports would be two answers to that question, and they would disagree the
//! first time a program re-exported a host module.
//!
//! Exports were refused entirely before this, and the reason given was that an
//! ignored `export` compiles a program whose exports nobody can see. That is
//! still true and is why nothing here is silent about a shape it cannot lower.

use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;

use super::{Ctx, EmitError, EmitResult, Scope};
use crate::runtime::RuntimeOp;
use crate::names::Name;
use crate::syntax::{Import, ImportBinding};

/// Binds everything one `import` introduces.
///
/// Emitted into the body's own scope, in source order among the statements —
/// which is not where the specification hoists them, and is the divergence to
/// state: a module's imports are all bound before any of its statements run, so
/// a program reading an imported name ABOVE the `import` line sees `undefined`
/// here and the value there. Every file in the corpus this serves writes its
/// imports first.
pub fn emit_import(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    import: &Import,
) -> EmitResult<()> {
    // The specifier is a string the program wrote, so it is already in the
    // literal table — the same table `string_const` reads, and the number is
    // what crosses rather than the text.
    let specifier = ctx.literal(&import.source);
    let specifier = number(builder, u64::from(specifier));

    for binding in &import.bindings {
        let (local, value) = match binding {
            ImportBinding::Named { exported, local } => {
                let name = ctx.names.intern(exported);
                let key = number(builder, u64::from(ctx.key_of(name)));
                let read = super::expr::call(
                    builder,
                    ctx,
                    RuntimeOp::ModuleBinding,
                    &[specifier, key],
                )?[0];
                (*local, read)
            }
            // `import d from "m"` is `import { default as d }`, which is what
            // the specification says it is — and saying it here rather than in
            // the runtime keeps "default" a name in the key registry like any
            // other rather than a case the lookup knows about.
            ImportBinding::Default(local) => {
                let name = ctx.names.intern("default");
                let key = number(builder, u64::from(ctx.key_of(name)));
                let read = super::expr::call(
                    builder,
                    ctx,
                    RuntimeOp::ModuleBinding,
                    &[specifier, key],
                )?[0];
                (*local, read)
            }
            ImportBinding::Namespace(local) => {
                let read = super::expr::call(
                    builder,
                    ctx,
                    RuntimeOp::ModuleNamespace,
                    &[specifier],
                )?[0];
                (*local, read)
            }
        };
        super::binding::declare(builder, scope, ctx, local, value)?;
    }
    Ok(())
}

/// `import.meta`.
///
/// # Why the module's own specifier is what crosses
///
/// Because it is the only thing that identifies the module, and the emitter is
/// the last place that knows it: the object itself holds a URL and whether this
/// module is the program's entry, and a compiler knows neither. Both come down
/// as a declaration from the host, which resolved the file in the first place.
///
/// A script is refused rather than answered. `import.meta` is a module-only
/// syntax in the language, and a script given one would be a name with nothing
/// behind it — the rule this repository states for a surface that cannot do what
/// its name means.
pub fn emit_import_meta(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
) -> EmitResult<ValueId> {
    let Some(own) = ctx.module_specifier.clone() else {
        return Err(refuse("`import.meta` outside a module"));
    };
    let own = ctx.literal(&own);
    let own = number(builder, u64::from(own));
    Ok(super::expr::call(builder, ctx, RuntimeOp::ImportMeta, &[own])?[0])
}

/// `import(specifier)` — a promise for a module's namespace.
///
/// # Why the specifier is not resolved here
///
/// A static `import` is resolved by the host before this crate ever sees it —
/// `rts-host`'s loader rewrites the tree, which is why [`emit_import`] can hand
/// the runtime a literal. A dynamic one cannot be: `import(name)` computes its
/// argument, so there is nothing in the tree to rewrite. So the VALUE crosses,
/// together with which module asked, and the host answers what `"./x"` means
/// from that pair — the same division of labour, decided at run time because
/// that is when the question exists.
///
/// # The second argument is ignored, and said so
///
/// `import(specifier, { with: … })` carries import attributes, which decide how
/// a non-JavaScript module is interpreted. Nothing here interprets one, so
/// evaluating the options object and discarding it would be a promise that the
/// attributes were honoured. It is refused instead.
pub fn emit_import_call(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    specifier: &crate::syntax::Expr,
    options: Option<&crate::syntax::Expr>,
) -> EmitResult<ValueId> {
    if options.is_some() {
        return Err(refuse("`import()` with import attributes"));
    }
    let Some(own) = ctx.module_specifier.clone() else {
        return Err(refuse("`import()` outside a module"));
    };
    let value = super::expr::emit_expr(builder, scope, ctx, specifier)?;
    let own = ctx.literal(&own);
    let own = number(builder, u64::from(own));
    Ok(super::expr::call(builder, ctx, RuntimeOp::ModuleImport, &[value, own])?[0])
}

/// One name this module makes visible, and where its value comes from.
///
/// Collected while lowering the module's items and emitted after its body, so
/// the value published is the one the module finished with. See
/// [`emit_publications`] for why that is the order.
#[derive(Clone, Debug)]
pub struct Publication {
    /// The name the outside sees. A string, not a `Name`, because
    /// `export { x as "a b" }` is legal.
    pub exported: String,
    /// What supplies it.
    pub source: PublicationSource,
}

/// Where a published value comes from.
#[derive(Clone, Debug)]
pub enum PublicationSource {
    /// A binding this module has. `export const x = 1` and `export { x }` are
    /// both this, because both leave `x` in the module's own scope.
    Local(Name),
    /// `export { x } from "m"` — forwarded without ever being bound here, which
    /// is what the specification says a re-export is.
    Reexport {
        /// Which module.
        specifier: String,
        /// The name in it.
        name: String,
    },
    /// `export * as ns from "m"` — the other module's whole namespace, under
    /// one name.
    Namespace {
        /// Which module.
        specifier: String,
    },
    /// `export * from "m"` — every name it exports, under those same names.
    ///
    /// No `exported` name of its own, which is why it is its own variant rather
    /// than a `Namespace` with a blank: what it publishes is a SET, decided by
    /// the other module at run time.
    All {
        /// Which module.
        specifier: String,
    },
}

/// Writes this module's exports into the table an `import` reads.
///
/// # Why after the body rather than at each `export`
///
/// Because an export is observable only once the module has been evaluated, and
/// publishing at the declaration would snapshot a value the module was about to
/// change. Publishing at the end makes `export let n = 0; n = 1;` answer `1`,
/// which is what a live binding would answer — as close to live as a table of
/// values can be, and the same divergence [`super::super::runtime`]'s
/// `ModuleBinding` already states from the other side.
///
/// The one thing this would get wrong is the evaluation ORDER of
/// `export default <expression>`, which must run where it is written. That is
/// why the lowering binds such an expression to a local in place and publishes
/// the local from here: the order is the body's and the publication is uniform.
pub fn emit_publications(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    module: &str,
    publications: &[Publication],
) -> EmitResult<()> {
    if publications.is_empty() {
        return Ok(());
    }
    let own = ctx.literal(module);
    let own = number(builder, u64::from(own));
    for publication in publications {
        // The one publication that names no export: it copies a set. Emitted
        // here and skipped below, because everything after this reads
        // `publication.exported` as the key to publish under.
        if let PublicationSource::All { specifier } = &publication.source {
            let from = ctx.literal(specifier);
            let from = number(builder, u64::from(from));
            super::expr::call(builder, ctx, RuntimeOp::ModulePublishAll, &[own, from])?;
            continue;
        }
        let value = match &publication.source {
            PublicationSource::Local(local) => super::binding::read(builder, scope, ctx, *local)?,
            PublicationSource::Reexport { specifier, name } => {
                let from = ctx.literal(specifier);
                let from = number(builder, u64::from(from));
                let interned = ctx.names.intern(name);
                let key = number(builder, u64::from(ctx.key_of(interned)));
                super::expr::call(builder, ctx, RuntimeOp::ModuleBinding, &[from, key])?[0]
            }
            PublicationSource::Namespace { specifier } => {
                let from = ctx.literal(specifier);
                let from = number(builder, u64::from(from));
                super::expr::call(builder, ctx, RuntimeOp::ModuleNamespace, &[from])?[0]
            }
            // Handled above, where it is emitted: it publishes a set rather
            // than a value, so there is nothing for this match to produce.
            PublicationSource::All { .. } => continue,
        };
        let interned = ctx.names.intern(&publication.exported);
        let key = number(builder, u64::from(ctx.key_of(interned)));
        super::expr::call(builder, ctx, RuntimeOp::ModulePublish, &[own, key, value])?;
    }
    Ok(())
}

/// Refuses an export shape this lowering does not have, by name.
pub fn refuse(construct: &'static str) -> EmitError {
    EmitError::Unsupported { construct }
}

/// An integer operand: which specifier, or which key.
///
/// Not a tagged value, for the reason a property key is not one: neither is
/// something the program could compute, and emitting one tagged would claim it
/// could.
fn number(builder: &mut FuncBuilder, bits: u64) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(bits),
    });
    builder.use_const(id)
}

/// Turns one `export` into the statements it runs and the names it publishes.
///
/// # Why the two are separated
///
/// An `export const x = 1` does two things — it declares `x` here, and it makes
/// `x` visible outside — and only the first has an order that matters. So the
/// declaration joins the body where it was written, keeping evaluation order
/// exactly as the source has it, and the publication joins a list emitted once
/// at the end. See [`emit_publications`] for why the end is the right place.
pub fn lower_export(
    export: &crate::syntax::Export,
    body: &mut Vec<crate::syntax::Stmt>,
    publications: &mut Vec<Publication>,
    ctx: &mut Ctx,
) -> EmitResult<()> {
    use crate::syntax::{ExportDefault, ExportKind, StmtKind};
    match &export.kind {
        ExportKind::Declaration(statement) => {
            for name in declared_names(statement) {
                publications.push(Publication {
                    exported: ctx.names.text(name).to_owned(),
                    source: PublicationSource::Local(name),
                });
            }
            body.push((**statement).clone());
        }
        ExportKind::Named {
            specifiers,
            source: None,
            ..
        } => {
            for specifier in specifiers {
                // `export { x }` names a binding this module has, and the name
                // is an identifier there even when the EXPORTED name is not.
                let local = ctx.names.intern(&specifier.local);
                publications.push(Publication {
                    exported: specifier.exported.clone(),
                    source: PublicationSource::Local(local),
                });
            }
        }
        ExportKind::Named {
            specifiers,
            source: Some(from),
            ..
        } => {
            for specifier in specifiers {
                // Nothing is bound locally: a re-export forwards, and this
                // module never sees the name. That is the specification's own
                // distinction and it is why this is a separate source rather
                // than a synthesised import.
                //
                // `export * as ns from "m"` arrives HERE rather than as
                // `ExportKind::All` — the parser gives it a specifier whose
                // local name is `"*"` — and forwarding it by that name asked
                // `m` for an export called `*`, which no module has. It
                // published `undefined` under a key that was really there, so
                // `Object.keys` showed the name and reading it answered
                // nothing: the shape of wrongness that looks like it works.
                let source = match specifier.local.as_str() {
                    "*" => PublicationSource::Namespace {
                        specifier: from.clone(),
                    },
                    name => PublicationSource::Reexport {
                        specifier: from.clone(),
                        name: name.to_owned(),
                    },
                };
                publications.push(Publication {
                    exported: specifier.exported.clone(),
                    source,
                });
            }
        }
        ExportKind::Default(ExportDefault::Declaration(statement)) => {
            let Some(name) = declared_names(statement).into_iter().next() else {
                // `export default function () {}` binds nothing of its own, so
                // it becomes the EXPRESSION form below: a `const` in the
                // reserved space no program can spell, declared where the
                // declaration was written. This was refused, on the reasoning
                // that a synthetic name could collide — which was already
                // answered two arms down, by `@@default`, for exactly the same
                // problem. One answer, reached from both spellings.
                //
                // The divergence that leaves, stated rather than discovered: a
                // function DECLARATION is hoisted and a `const` is not, so a
                // module reading its own anonymous default above the `export`
                // line sees nothing here and the function there. Nothing can
                // read it from OUTSIDE before the module has run, which is
                // where the name is observable.
                let kind = match &statement.kind {
                    StmtKind::Function(function) => {
                        crate::syntax::ExprKind::Function(function.clone())
                    }
                    StmtKind::Class(class) => crate::syntax::ExprKind::Class(class.clone()),
                    // Nothing else can be an `export default` declaration: the
                    // grammar admits a function, a class, and an expression,
                    // and the third arrives at the arm below.
                    _ => return Err(refuse("an anonymous `export default` declaration")),
                };
                let local = ctx.names.intern("@@default");
                body.push(crate::syntax::Stmt {
                    kind: StmtKind::Declare {
                        kind: crate::syntax::BindingKind::Const,
                        bindings: vec![crate::syntax::Binding {
                            target: crate::syntax::Pattern::Name(local),
                            value: Some(crate::syntax::Expr {
                                kind,
                                at: statement.at,
                            }),
                            claim: None,
                        }],
                    },
                    at: export.at,
                });
                publications.push(Publication {
                    exported: "default".to_owned(),
                    source: PublicationSource::Local(local),
                });
                return Ok(());
            };
            publications.push(Publication {
                exported: "default".to_owned(),
                source: PublicationSource::Local(name),
            });
            body.push((**statement).clone());
        }
        ExportKind::Default(ExportDefault::Expr(expression)) => {
            // Bound to a local IN PLACE, so the expression runs where it was
            // written — the one thing publishing at the end would otherwise get
            // wrong. The name is in the reserved space no program can spell, for
            // the same reason a synthetic name is refused above.
            let local = ctx.names.intern("@@default");
            body.push(crate::syntax::Stmt {
                kind: StmtKind::Declare {
                    kind: crate::syntax::BindingKind::Const,
                    bindings: vec![crate::syntax::Binding {
                        target: crate::syntax::Pattern::Name(local),
                        value: Some(expression.clone()),
                        // No type annotation was written, and inventing one
                        // would be a claim about a value only the program knows.
                        claim: None,
                    }],
                },
                at: export.at,
            });
            publications.push(Publication {
                exported: "default".to_owned(),
                source: PublicationSource::Local(local),
            });
        }
        ExportKind::All {
            source,
            alias: Some(alias),
            ..
        } => publications.push(Publication {
            exported: alias.clone(),
            source: PublicationSource::Namespace {
                specifier: source.clone(),
            },
        }),
        // `export * from "m"` publishes every name `m` exports, which is a SET
        // the compiler cannot know: `m`'s body decides it and has already run by
        // the time this does, since the graph is ordered dependencies-first.
        // So it is one call — `ModulePublishAll` — rather than a list emitted
        // here. It was refused until that operation existed, because a version
        // publishing nothing compiles a module whose exports are silently
        // missing.
        ExportKind::All {
            source,
            alias: None,
            ..
        } => publications.push(Publication {
            // Not a name. What this publishes is decided at run time, and the
            // emitter reads the SOURCE to know that.
            exported: String::new(),
            source: PublicationSource::All {
                specifier: source.clone(),
            },
        }),
    }
    Ok(())
}

/// Every name a statement introduces, for the declaration forms an `export` may
/// wrap.
fn declared_names(statement: &crate::syntax::Stmt) -> Vec<Name> {
    use crate::syntax::StmtKind;
    let mut names = Vec::new();
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                binding.target.bound_names(&mut names);
            }
        }
        StmtKind::Function(function) => names.extend(function.name),
        StmtKind::Class(class) => names.extend(class.name),
        // Nothing else is a declaration, and the parser does not produce one
        // here — an `export` over anything else is a syntax error before this.
        _ => {}
    }
    names
}

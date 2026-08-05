//! SWC expressions into ours.

use swc_common::Spanned;
use swc_ecma_ast as swc;

use super::{Cx, Result, position, unsupported};
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Expr, ExprKind, Literal, LogicalOp, Property, PropertyKey,
    Spreadable, TemplatePart, UnaryOp, UpdateOp, UpdatePosition,
};
use crate::values::Singleton;

/// One expression.
pub(crate) fn expr(cx: &mut Cx, node: &swc::Expr) -> Result<Expr> {
    let at = position(node.span());
    let kind = match node {
        swc::Expr::Ident(ident) => ExprKind::Ident(cx.name(&ident.sym)),
        swc::Expr::Lit(literal) => ExprKind::Literal(lit(literal)?),
        swc::Expr::This(_) => ExprKind::This,

        swc::Expr::Bin(bin) => {
            if let Some(op) = logical_op(bin.op) {
                ExprKind::Logical {
                    op,
                    left: Box::new(expr(cx, &bin.left)?),
                    right: Box::new(expr(cx, &bin.right)?),
                }
            } else {
                ExprKind::Binary {
                    op: binary_op(bin.op, at)?,
                    left: Box::new(expr(cx, &bin.left)?),
                    right: Box::new(expr(cx, &bin.right)?),
                }
            }
        }

        swc::Expr::Unary(unary) => ExprKind::Unary {
            op: unary_op(unary.op),
            operand: Box::new(expr(cx, &unary.arg)?),
        },

        swc::Expr::Update(update) => ExprKind::Update {
            op: if update.op == swc::UpdateOp::PlusPlus {
                UpdateOp::Increment
            } else {
                UpdateOp::Decrement
            },
            position: if update.prefix {
                UpdatePosition::Prefix
            } else {
                UpdatePosition::Postfix
            },
            target: Box::new(expr(cx, &update.arg)?),
        },

        swc::Expr::Cond(cond) => ExprKind::Conditional {
            condition: Box::new(expr(cx, &cond.test)?),
            then_branch: Box::new(expr(cx, &cond.cons)?),
            else_branch: Box::new(expr(cx, &cond.alt)?),
        },

        swc::Expr::Seq(seq) => ExprKind::Sequence {
            operands: seq
                .exprs
                .iter()
                .map(|e| expr(cx, e))
                .collect::<Result<_>>()?,
        },

        swc::Expr::Paren(paren) => return expr(cx, &paren.expr),

        swc::Expr::Member(member) => return member_expr(cx, member, false),

        swc::Expr::Array(array) => ExprKind::Array {
            elements: array
                .elems
                .iter()
                .map(|element| match element {
                    None => Ok(None),
                    Some(item) => spreadable(cx, item).map(Some),
                })
                .collect::<Result<_>>()?,
        },

        swc::Expr::Object(object) => ExprKind::Object {
            properties: object
                .props
                .iter()
                .map(|p| property(cx, p))
                .collect::<Result<_>>()?,
        },

        swc::Expr::Call(call) => match &call.callee {
            swc::Callee::Expr(callee) => ExprKind::Call {
                callee: Box::new(expr(cx, callee)?),
                arguments: arguments(cx, &call.args)?,
                optional: false,
            },
            swc::Callee::Super(_) => ExprKind::SuperCall {
                arguments: arguments(cx, &call.args)?,
            },
            swc::Callee::Import(_) => {
                let mut args = call.args.iter();
                let specifier = match args.next() {
                    Some(first) => Box::new(expr(cx, &first.expr)?),
                    None => return unsupported("import() with no specifier", at),
                };
                ExprKind::ImportCall {
                    specifier,
                    options: match args.next() {
                        Some(second) => Some(Box::new(expr(cx, &second.expr)?)),
                        None => None,
                    },
                }
            }
        },

        swc::Expr::New(new) => ExprKind::New {
            callee: Box::new(expr(cx, &new.callee)?),
            arguments: match &new.args {
                Some(args) => arguments(cx, args)?,
                None => Vec::new(),
            },
        },

        swc::Expr::Assign(assign) => ExprKind::Assign {
            target: assign_target(cx, &assign.left)?,
            value: Box::new(expr(cx, &assign.right)?),
            op: assign_op(assign.op, at)?,
        },

        swc::Expr::Tpl(template) => ExprKind::Template {
            parts: template.quasis.iter().map(quasi).collect(),
            expressions: template
                .exprs
                .iter()
                .map(|e| expr(cx, e))
                .collect::<Result<_>>()?,
        },

        swc::Expr::TaggedTpl(tagged) => ExprKind::TaggedTemplate {
            tag: Box::new(expr(cx, &tagged.tag)?),
            parts: tagged.tpl.quasis.iter().map(quasi).collect(),
            expressions: tagged
                .tpl
                .exprs
                .iter()
                .map(|e| expr(cx, e))
                .collect::<Result<_>>()?,
        },

        swc::Expr::Await(await_) => ExprKind::Await(Box::new(expr(cx, &await_.arg)?)),

        swc::Expr::Yield(yield_) => ExprKind::Yield {
            value: match &yield_.arg {
                Some(value) => Some(Box::new(expr(cx, value)?)),
                None => None,
            },
            delegate: yield_.delegate,
        },

        swc::Expr::Fn(function) => {
            ExprKind::Function(Box::new(super::item::function_expr(cx, function)?))
        }
        swc::Expr::Arrow(arrow) => ExprKind::Function(Box::new(super::item::arrow(cx, arrow)?)),
        swc::Expr::Class(class) => ExprKind::Class(Box::new(super::item::class_expr(cx, class)?)),

        swc::Expr::SuperProp(super_prop) => ExprKind::SuperMember {
            property: Box::new(match &super_prop.prop {
                swc::SuperProp::Ident(ident) => PropertyKey::Named(cx.name(&ident.sym)),
                swc::SuperProp::Computed(computed) => {
                    PropertyKey::Computed(expr(cx, &computed.expr)?)
                }
            }),
        },

        swc::Expr::MetaProp(meta) => match meta.kind {
            swc::MetaPropKind::NewTarget => ExprKind::NewTarget,
            swc::MetaPropKind::ImportMeta => ExprKind::ImportMeta,
        },

        swc::Expr::PrivateName(private) => ExprKind::PrivateName(cx.name(&private.name)),

        swc::Expr::OptChain(chain) => return opt_chain(cx, chain),

        // TypeScript wrappers: the claim is carried where a claim belongs.
        swc::Expr::TsAs(as_) => {
            return Ok(Expr::new(
                ExprKind::Asserted {
                    value: Box::new(expr(cx, &as_.expr)?),
                    claim: super::item::claim(cx, &as_.type_ann),
                },
                at,
            ));
        }
        swc::Expr::TsNonNull(non_null) => return expr(cx, &non_null.expr),
        swc::Expr::TsSatisfies(satisfies) => return expr(cx, &satisfies.expr),
        swc::Expr::TsTypeAssertion(assertion) => {
            return Ok(Expr::new(
                ExprKind::Asserted {
                    value: Box::new(expr(cx, &assertion.expr)?),
                    claim: super::item::claim(cx, &assertion.type_ann),
                },
                at,
            ));
        }
        swc::Expr::TsConstAssertion(const_) => return expr(cx, &const_.expr),
        swc::Expr::TsInstantiation(inst) => return expr(cx, &inst.expr),

        swc::Expr::Invalid(_) => return unsupported("an expression SWC could not read", at),
        swc::Expr::JSXElement(_)
        | swc::Expr::JSXFragment(_)
        | swc::Expr::JSXEmpty(_)
        | swc::Expr::JSXMember(_)
        | swc::Expr::JSXNamespacedName(_) => return unsupported("JSX", at),
    };

    Ok(Expr::new(kind, at))
}

fn member_expr(cx: &mut Cx, member: &swc::MemberExpr, optional: bool) -> Result<Expr> {
    let at = position(member.span);
    let object = Box::new(expr(cx, &member.obj)?);
    let kind = match &member.prop {
        swc::MemberProp::Ident(ident) => ExprKind::Member {
            object,
            property: cx.name(&ident.sym),
            optional,
        },
        swc::MemberProp::PrivateName(private) => ExprKind::Member {
            object,
            property: cx.name(&private.name),
            optional,
        },
        swc::MemberProp::Computed(computed) => ExprKind::Index {
            object,
            index: Box::new(expr(cx, &computed.expr)?),
            optional,
        },
    };
    Ok(Expr::new(kind, at))
}

/// An optional chain, wrapped in the boundary that says how far it reaches.
fn opt_chain(cx: &mut Cx, chain: &swc::OptChainExpr) -> Result<Expr> {
    let at = position(chain.span);
    let inner = match &*chain.base {
        swc::OptChainBase::Member(member) => member_expr(cx, member, chain.optional)?,
        swc::OptChainBase::Call(call) => Expr::new(
            ExprKind::Call {
                callee: Box::new(expr(cx, &call.callee)?),
                arguments: arguments(cx, &call.args)?,
                optional: chain.optional,
            },
            position(call.span),
        ),
    };
    Ok(Expr::new(ExprKind::Chain(Box::new(inner)), at))
}

pub(crate) fn arguments(cx: &mut Cx, args: &[swc::ExprOrSpread]) -> Result<Vec<Spreadable>> {
    args.iter().map(|arg| spreadable(cx, arg)).collect()
}

fn spreadable(cx: &mut Cx, item: &swc::ExprOrSpread) -> Result<Spreadable> {
    let value = expr(cx, &item.expr)?;
    Ok(match item.spread {
        Some(_) => Spreadable::Spread(value),
        None => Spreadable::Single(value),
    })
}

fn quasi(element: &swc::TplElement) -> TemplatePart {
    TemplatePart {
        cooked: element
            .cooked
            .as_ref()
            .map(|c| c.to_string_lossy().to_string()),
        raw: element.raw.to_string(),
    }
}

fn lit(literal: &swc::Lit) -> Result<Literal> {
    Ok(match literal {
        swc::Lit::Num(number) => Literal::Number(number.value),
        swc::Lit::Str(string) => Literal::String(string.value.to_string_lossy().to_string()),
        swc::Lit::Bool(boolean) => Literal::Boolean(boolean.value),
        swc::Lit::Null(_) => Literal::Singleton(Singleton::Null),
        // Both are carried as the text that was written. SWC read them
        // correctly all along — it was this bridge that had nowhere to put
        // them, which is why 425 files of `test/language` were refused by a
        // parser that had not refused anything.
        swc::Lit::Regex(regex) => Literal::Regex {
            pattern: regex.exp.to_string(),
            flags: regex.flags.to_string(),
        },
        swc::Lit::BigInt(big) => Literal::BigInt(big.value.to_string()),
        swc::Lit::JSXText(text) => return unsupported("JSX text", position(text.span)),
    })
}

/// An object-literal entry, in whichever of its five shapes it was written.
fn property(cx: &mut Cx, prop: &swc::PropOrSpread) -> Result<Property> {
    match prop {
        swc::PropOrSpread::Spread(spread) => Ok(Property::Spread(expr(cx, &spread.expr)?)),
        swc::PropOrSpread::Prop(prop) => match &**prop {
            swc::Prop::Shorthand(ident) => Ok(Property::Value {
                key: PropertyKey::Named(cx.name(&ident.sym)),
                value: Expr::new(ExprKind::Ident(cx.name(&ident.sym)), position(ident.span)),
                shorthand: true,
            }),

            swc::Prop::KeyValue(pair) => {
                let key = property_key(cx, &pair.key)?;
                // `__proto__: v` is the one spelling that sets the prototype.
                // Shorthand, computed and method spellings are ordinary
                // properties with that name, which is why this is checked here
                // and not on the key.
                if let PropertyKey::Named(name) = &key
                    && cx.names.text(*name) == "__proto__"
                {
                    return Ok(Property::Prototype(expr(cx, &pair.value)?));
                }
                Ok(Property::Value {
                    key,
                    value: expr(cx, &pair.value)?,
                    shorthand: false,
                })
            }

            swc::Prop::Method(method) => Ok(Property::Method {
                key: property_key(cx, &method.key)?,
                function: Box::new(super::item::function(cx, &method.function)?),
            }),

            swc::Prop::Getter(getter) => Ok(Property::Getter {
                key: property_key(cx, &getter.key)?,
                function: Box::new(super::item::accessor(
                    cx,
                    getter.body.as_ref(),
                    &[],
                    position(getter.span),
                )?),
            }),

            swc::Prop::Setter(setter) => Ok(Property::Setter {
                key: property_key(cx, &setter.key)?,
                function: Box::new(super::item::accessor(
                    cx,
                    setter.body.as_ref(),
                    std::slice::from_ref(&setter.param),
                    position(setter.span),
                )?),
            }),

            swc::Prop::Assign(assign) => unsupported(
                "`{a = 1}` outside a destructuring target",
                position(assign.key.span),
            ),
        },
    }
}

pub(crate) fn property_key(cx: &mut Cx, key: &swc::PropName) -> Result<PropertyKey> {
    Ok(match key {
        swc::PropName::Ident(ident) => PropertyKey::Named(cx.name(&ident.sym)),
        swc::PropName::Str(string) => PropertyKey::Named(cx.name(&string.value.to_string_lossy())),
        swc::PropName::Num(number) => {
            // A numeric key is a property key, which is a string. Which string
            // is decided by Number::toString, so this is the one place the
            // bridge must not simply format the double its own way.
            PropertyKey::Named(cx.name(&number_key(number.value)))
        }
        swc::PropName::Computed(computed) => PropertyKey::Computed(expr(cx, &computed.expr)?),
        swc::PropName::BigInt(big) => {
            return unsupported("a BigInt property key", position(big.span));
        }
    })
}

/// How a numeric property key prints.
///
/// `{ 1: x }` is the property `"1"`, and `{ 1.0: x }` is the same property,
/// because the key is the *value* run through Number::toString. Rust's default
/// `f64` formatting agrees for integers, which is the case that matters for
/// array-like keys; §5.5 records that the general algorithm is the shortest
/// round-tripping decimal, and this is where that will have to arrive.
fn number_key(value: f64) -> String {
    if value.fract() == 0.0 && value.is_finite() && value.abs() < 1e21 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Reinterpret the left of an assignment.
///
/// SWC hands back a pattern where it could decide and an expression otherwise.
/// The pattern side is the cover grammar already resolved for us; the expression
/// side is a place to write.
fn assign_target(cx: &mut Cx, target: &swc::AssignTarget) -> Result<AssignTarget> {
    match target {
        swc::AssignTarget::Simple(simple) => match simple {
            swc::SimpleAssignTarget::Ident(ident) => Ok(AssignTarget::Place(Box::new(Expr::new(
                ExprKind::Ident(cx.name(&ident.id.sym)),
                position(ident.span),
            )))),
            swc::SimpleAssignTarget::Member(member) => Ok(AssignTarget::Place(Box::new(
                member_expr(cx, member, false)?,
            ))),
            swc::SimpleAssignTarget::Paren(paren) => {
                Ok(AssignTarget::Place(Box::new(expr(cx, &paren.expr)?)))
            }
            swc::SimpleAssignTarget::TsAs(as_) => {
                Ok(AssignTarget::Place(Box::new(expr(cx, &as_.expr)?)))
            }
            swc::SimpleAssignTarget::TsNonNull(non_null) => {
                Ok(AssignTarget::Place(Box::new(expr(cx, &non_null.expr)?)))
            }
            other => unsupported("this assignment target", position(other.span())),
        },
        swc::AssignTarget::Pat(pattern) => match pattern {
            swc::AssignTargetPat::Array(array) => {
                Ok(AssignTarget::Pattern(super::pat::array_target(cx, array)?))
            }
            swc::AssignTargetPat::Object(object) => Ok(AssignTarget::Pattern(
                super::pat::object_target(cx, object)?,
            )),
            swc::AssignTargetPat::Invalid(invalid) => unsupported(
                "an assignment target SWC could not read",
                position(invalid.span),
            ),
        },
    }
}

fn logical_op(op: swc::BinaryOp) -> Option<LogicalOp> {
    Some(match op {
        swc::BinaryOp::LogicalAnd => LogicalOp::And,
        swc::BinaryOp::LogicalOr => LogicalOp::Or,
        swc::BinaryOp::NullishCoalescing => LogicalOp::Coalesce,
        _ => return None,
    })
}

fn binary_op(op: swc::BinaryOp, at: rts_cranelift::fault::Position) -> Result<BinaryOp> {
    Ok(match op {
        swc::BinaryOp::Add => BinaryOp::Add,
        swc::BinaryOp::Sub => BinaryOp::Sub,
        swc::BinaryOp::Mul => BinaryOp::Mul,
        swc::BinaryOp::Div => BinaryOp::Div,
        swc::BinaryOp::Mod => BinaryOp::Rem,
        swc::BinaryOp::Exp => BinaryOp::Exponent,
        swc::BinaryOp::BitAnd => BinaryOp::BitAnd,
        swc::BinaryOp::BitOr => BinaryOp::BitOr,
        swc::BinaryOp::BitXor => BinaryOp::BitXor,
        swc::BinaryOp::LShift => BinaryOp::Shl,
        swc::BinaryOp::RShift => BinaryOp::Shr,
        swc::BinaryOp::ZeroFillRShift => BinaryOp::UShr,
        swc::BinaryOp::EqEqEq => BinaryOp::StrictEqual,
        swc::BinaryOp::NotEqEq => BinaryOp::StrictNotEqual,
        swc::BinaryOp::EqEq => BinaryOp::LooseEqual,
        swc::BinaryOp::NotEq => BinaryOp::LooseNotEqual,
        swc::BinaryOp::Lt => BinaryOp::Less,
        swc::BinaryOp::LtEq => BinaryOp::LessEqual,
        swc::BinaryOp::Gt => BinaryOp::Greater,
        swc::BinaryOp::GtEq => BinaryOp::GreaterEqual,
        swc::BinaryOp::In => BinaryOp::In,
        swc::BinaryOp::InstanceOf => BinaryOp::InstanceOf,
        swc::BinaryOp::LogicalAnd | swc::BinaryOp::LogicalOr | swc::BinaryOp::NullishCoalescing => {
            return unsupported("a short-circuiting operator reached as a binary one", at);
        }
    })
}

fn unary_op(op: swc::UnaryOp) -> UnaryOp {
    match op {
        swc::UnaryOp::Minus => UnaryOp::Negate,
        swc::UnaryOp::Plus => UnaryOp::Plus,
        swc::UnaryOp::Bang => UnaryOp::Not,
        swc::UnaryOp::Tilde => UnaryOp::BitNot,
        swc::UnaryOp::TypeOf => UnaryOp::TypeOf,
        swc::UnaryOp::Void => UnaryOp::Void,
        swc::UnaryOp::Delete => UnaryOp::Delete,
    }
}

fn assign_op(op: swc::AssignOp, at: rts_cranelift::fault::Position) -> Result<AssignOp> {
    Ok(match op {
        swc::AssignOp::Assign => AssignOp::Plain,
        swc::AssignOp::AndAssign => AssignOp::Logical(LogicalOp::And),
        swc::AssignOp::OrAssign => AssignOp::Logical(LogicalOp::Or),
        swc::AssignOp::NullishAssign => AssignOp::Logical(LogicalOp::Coalesce),
        other => {
            let binary = match other {
                swc::AssignOp::AddAssign => BinaryOp::Add,
                swc::AssignOp::SubAssign => BinaryOp::Sub,
                swc::AssignOp::MulAssign => BinaryOp::Mul,
                swc::AssignOp::DivAssign => BinaryOp::Div,
                swc::AssignOp::ModAssign => BinaryOp::Rem,
                swc::AssignOp::ExpAssign => BinaryOp::Exponent,
                swc::AssignOp::BitAndAssign => BinaryOp::BitAnd,
                swc::AssignOp::BitOrAssign => BinaryOp::BitOr,
                swc::AssignOp::BitXorAssign => BinaryOp::BitXor,
                swc::AssignOp::LShiftAssign => BinaryOp::Shl,
                swc::AssignOp::RShiftAssign => BinaryOp::Shr,
                swc::AssignOp::ZeroFillRShiftAssign => BinaryOp::UShr,
                _ => return unsupported("this compound assignment", at),
            };
            // The construction path refuses a comparing operator, so a bad
            // mapping above becomes an error here rather than a tree that says
            // something the language cannot.
            match AssignOp::compound(binary) {
                Some(op) => op,
                None => return unsupported("a compound assignment of a comparison", at),
            }
        }
    })
}

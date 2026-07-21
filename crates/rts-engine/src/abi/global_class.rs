//! Global class metadata — for built-in JS classes (`Date`, `Error`, …).
//!
//! A `GlobalClassSpec` registers a class that lives in global scope without
//! an explicit import: `new Date()`, `d.getFullYear()`, etc.
//!
//! Members are tagged by `MemberKind`:
//! - `Function`      → static method  (`Date.now()`)
//! - `Constructor`   → ctor overload   (`new Date()` / `new Date(ms)`)
//! - `InstanceMethod`→ instance method (`d.getFullYear()`)
//!
//! Codegen consults `GLOBAL_CLASS_SPECS` in `lower_new` (for constructors)
//! and after the user-class dispatch path (for instance methods).

use super::member::NamespaceMember;

/// A globally-scoped JS class backed by RTS runtime symbols.
#[derive(Debug, Clone, Copy)]
pub struct GlobalClassSpec {
    /// JS class name, e.g. `"Date"`, `"Error"`.
    pub name: &'static str,
    /// Human-readable summary used by `rts apis`.
    pub doc: &'static str,
    /// All members: static functions, constructors, instance methods.
    pub members: &'static [NamespaceMember],
    /// Runtime symbol of the `x instanceof <Class>` predicate (`fn(handle) ->
    /// i64`). Lets codegen resolve instanceof for NON-primordial classes from
    /// the Registry without naming the class. `None` = no own predicate.
    pub instanceof_predicate: Option<&'static str>,
}

impl GlobalClassSpec {
    /// Returns all `Constructor` members, ordered by arity.
    pub fn constructors(&self) -> impl Iterator<Item = &NamespaceMember> {
        use super::member::MemberKind;
        self.members
            .iter()
            .filter(|m| m.kind == MemberKind::Constructor)
    }

    /// Returns the constructor whose arity matches `n_args`, if any.
    pub fn constructor_for_arity(&self, n_args: usize) -> Option<&NamespaceMember> {
        self.constructors().find(|m| m.args.len() == n_args)
    }

    /// Returns an instance method by name, first-by-name (arity-agnostic). Kept
    /// for call sites that do not know the call arity; prefer
    /// [`resolve_instance_method`](Self::resolve_instance_method) when the arity
    /// is known so overloads resolve correctly.
    pub fn instance_method(&self, name: &str) -> Option<&NamespaceMember> {
        use super::member::MemberKind;
        self.members
            .iter()
            .find(|m| m.kind == MemberKind::InstanceMethod && m.name == name)
    }

    /// Resolves an instance method by JS name (or one of its `aliases`) and the
    /// call's explicit-argument count, honouring arity overloads, optional
    /// trailing args (`default_args`) and variadics. Resolution order:
    ///
    /// 1. an overload whose explicit arity matches `n_args` exactly;
    /// 2. an overload that *accepts* `n_args` (within `[min, explicit]` via
    ///    optional defaults, or `>= fixed` when `variadic`);
    /// 3. first-by-name, so a single non-overloaded row always resolves.
    ///
    /// Instance members carry the receiver in `args[0]`, so the explicit param
    /// count is `args.len() - 1`. This replaces the ad-hoc arity `find` that was
    /// duplicated across the codegen call sites (docs/specs/rts-engine-dispatch.md §4.3).
    pub fn resolve_instance_method(&self, name: &str, n_args: usize) -> Option<&NamespaceMember> {
        use super::member::MemberKind;
        let named = |m: &&NamespaceMember| {
            m.kind == MemberKind::InstanceMethod && (m.name == name || m.aliases.contains(&name))
        };
        self.members
            .iter()
            .find(|m| named(m) && explicit_arity(m) == n_args)
            .or_else(|| {
                self.members
                    .iter()
                    .find(|m| named(m) && accepts_arity(m, n_args))
            })
            .or_else(|| self.members.iter().find(named))
    }

    /// Returns a static method/constant by name, if any.
    pub fn static_member(&self, name: &str) -> Option<&NamespaceMember> {
        use super::member::MemberKind;
        self.members.iter().find(|m| {
            matches!(
                m.kind,
                MemberKind::Function | MemberKind::Constant | MemberKind::StaticMethod
            ) && m.name == name
        })
    }

    /// Returns an instance getter (property, no parens) by name, if any.
    pub fn instance_getter(&self, name: &str) -> Option<&NamespaceMember> {
        use super::member::MemberKind;
        self.members
            .iter()
            .find(|m| m.kind == MemberKind::InstanceGetter && m.name == name)
    }

    /// Returns the instance setter (writable property `inst.prop = v`) by name,
    /// if any. A getter whose name has no matching setter is read-only.
    pub fn instance_setter(&self, name: &str) -> Option<&NamespaceMember> {
        use super::member::MemberKind;
        self.members
            .iter()
            .find(|m| m.kind == MemberKind::InstanceSetter && m.name == name)
    }
}

/// Explicit (TS-visible) parameter count of an instance member: `args` minus the
/// implicit `Handle` receiver at slot 0.
fn explicit_arity(m: &NamespaceMember) -> usize {
    m.args.len().saturating_sub(1)
}

/// Smallest explicit-argument count the member accepts: the explicit arity minus
/// the trailing run of optional `default_args`. With no `default_args` (the
/// common case) every explicit param is required, so `min == explicit`.
fn min_arity(m: &NamespaceMember) -> usize {
    use super::member::DefaultArg;
    let explicit = explicit_arity(m);
    if m.default_args.is_empty() {
        return explicit;
    }
    let optional_tail = m
        .default_args
        .iter()
        .rev()
        .take_while(|d| !matches!(d, DefaultArg::Required))
        .count();
    explicit.saturating_sub(optional_tail)
}

/// Whether a member accepts a call of `n_args` explicit arguments: a variadic
/// member absorbs any count at/above its fixed params; otherwise `n_args` must
/// fall within `[min_arity, explicit_arity]`.
fn accepts_arity(m: &NamespaceMember, n_args: usize) -> bool {
    let explicit = explicit_arity(m);
    if m.variadic {
        return n_args >= explicit.saturating_sub(1);
    }
    (min_arity(m)..=explicit).contains(&n_args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::member::{DefaultArg, MemberFlags, MemberKind};
    use crate::abi::types::AbiType;

    /// Builds an `InstanceMethod` row with `args` (incl. the receiver at slot 0),
    /// optional `aliases`/`variadic`/`default_args`.
    const fn im(
        name: &'static str,
        args: &'static [AbiType],
        aliases: &'static [&'static str],
        variadic: bool,
        default_args: &'static [DefaultArg],
    ) -> NamespaceMember {
        NamespaceMember {
            name,
            kind: MemberKind::InstanceMethod,
            symbol: "__RTS_FN_GL_TEST_X",
            args,
            returns: AbiType::I64,
            doc: "",
            ts_signature: "",
            emit: None,
            pure: false,
            aliases,
            variadic,
            default_args,
            flags: MemberFlags::NONE,
        }
    }

    // indexOf overloaded by arity: indexOf(x) and indexOf(x, from). Both carry
    // the Handle receiver at slot 0.
    const RECV: AbiType = AbiType::Handle;
    const IDX1: NamespaceMember = im("indexOf", &[RECV, AbiType::I64], &[], false, &[]);
    const IDX2: NamespaceMember = im(
        "indexOf",
        &[RECV, AbiType::I64, AbiType::I64],
        &[],
        false,
        &[],
    );
    const TRIM: NamespaceMember = im(
        "trimStart",
        &[RECV],
        &["trimLeft", "trim_start"],
        false,
        &[],
    );
    const PUSH: NamespaceMember = im("push", &[RECV, AbiType::I64], &[], true, &[]);
    // slice(start, end?) — end optional via default_args parallel to args.
    const SLICE: NamespaceMember = im(
        "slice",
        &[RECV, AbiType::I64, AbiType::I64],
        &[],
        false,
        &[
            DefaultArg::Required,
            DefaultArg::Required,
            DefaultArg::Int(i64::MIN),
        ],
    );

    fn spec(members: &'static [NamespaceMember]) -> GlobalClassSpec {
        GlobalClassSpec {
            name: "Test",
            doc: "",
            members,
            instanceof_predicate: None,
        }
    }

    #[test]
    fn arity_overload_picks_exact() {
        static MS: &[NamespaceMember] = &[IDX1, IDX2];
        let s = spec(MS);
        // 1 arg -> indexOf(x); 2 args -> indexOf(x, from)
        assert_eq!(
            s.resolve_instance_method("indexOf", 1).unwrap().args.len(),
            2
        );
        assert_eq!(
            s.resolve_instance_method("indexOf", 2).unwrap().args.len(),
            3
        );
    }

    #[test]
    fn first_by_name_fallback_when_no_exact() {
        static MS: &[NamespaceMember] = &[IDX1, IDX2];
        let s = spec(MS);
        // 5 args matches no arity -> first-by-name (IDX1)
        assert_eq!(
            s.resolve_instance_method("indexOf", 5).unwrap().args.len(),
            2
        );
    }

    #[test]
    fn alias_resolves() {
        static MS: &[NamespaceMember] = &[TRIM];
        let s = spec(MS);
        assert!(s.resolve_instance_method("trimLeft", 0).is_some());
        assert!(s.resolve_instance_method("trim_start", 0).is_some());
        assert!(s.resolve_instance_method("nope", 0).is_none());
    }

    #[test]
    fn variadic_accepts_any_count() {
        static MS: &[NamespaceMember] = &[PUSH];
        let s = spec(MS);
        // fixed explicit params = 1 (the variadic slot); accepts 0,1,5...
        for n in [0usize, 1, 5] {
            assert!(s.resolve_instance_method("push", n).is_some(), "push/{n}");
        }
    }

    #[test]
    fn optional_tail_accepts_shorter_call() {
        static MS: &[NamespaceMember] = &[SLICE];
        let s = spec(MS);
        // explicit arity 2, end optional -> accepts 1 and 2
        assert!(s.resolve_instance_method("slice", 1).is_some());
        assert!(s.resolve_instance_method("slice", 2).is_some());
    }
}

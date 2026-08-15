//! Source text in, tree out.
//!
//! SWC does the lexing and the parsing; this module decides what its output
//! *means* in terms of the tree next door. That split is deliberate and is the
//! reason there is no hand-written parser here.
//!
//! # Why not write the parser
//!
//! Because of automatic semicolon insertion. ASI is not formatting — a newline
//! after `return` ends the statement, and the restricted productions (`return`,
//! `throw`, `break`, `continue`, postfix `++`, `=>`, `yield`) change *what
//! parses*. Getting it wrong produces a program that compiles and means
//! something else. SWC implements it, has been run against test262, and is
//! already this repository's front end.
//!
//! # Translating is not the job; reinterpreting is
//!
//! Two grammars in JavaScript are *covers*: one sequence of tokens that becomes
//! one of two unrelated things depending on what follows.
//!
//! - `{a = 1}` is a SyntaxError as an object literal and a pattern slot with a
//!   default as a destructuring target.
//! - `(a, b)` is a parenthesised comma expression or an arrow's parameter list.
//! - `[a, b]` on the left of `=` is not an array literal at all.
//!
//! SWC resolves the first two and hands us the decided form. The third it hands
//! us as an expression, so [`expr::assign_target`] reinterprets it — and the
//! tree's own [`crate::syntax::Pattern::is_valid_binding`] is what catches a
//! reinterpretation into the wrong role.
//!
//! # What is refused, and why that is a feature
//!
//! Every construct this bridge does not yet handle returns
//! [`ParseError::Unsupported`] naming it. Nothing is silently dropped and
//! nothing is approximated, because a tree that is quietly missing a subtree is
//! the failure mode that produces a wrong program instead of an error.

mod expr;
mod item;
mod members;
mod pat;

use std::fmt;

use rts_cranelift::fault::Position;
use swc_common::{FileName, SourceMap, sync::Lrc};
use swc_ecma_parser::{EsSyntax, Lexer, Parser, StringInput, Syntax, TsSyntax};

use crate::names::Names;
use crate::syntax::{Goal, Program};

/// Why a source text did not become a tree.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParseError {
    /// The text is not a legal program. Carries what SWC said.
    Syntax(String),

    /// The text is legal and this bridge does not handle it yet.
    ///
    /// Separate from [`ParseError::Syntax`] because they mean opposite things
    /// about whose fault it is, and collapsing them would report our gaps as the
    /// user's mistakes.
    Unsupported {
        /// What was written.
        construct: &'static str,
        /// Where.
        at: Position,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Syntax(message) => write!(f, "syntax error: {message}"),
            ParseError::Unsupported { construct, .. } => {
                write!(f, "not yet lowered by the bridge: {construct}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// What every conversion in this module returns.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Refuse a construct by name.
pub(crate) fn unsupported<T>(construct: &'static str, at: Position) -> Result<T> {
    Err(ParseError::Unsupported { construct, at })
}

/// The interning table and the goal, carried through the conversion.
///
/// Held together because almost every step needs to intern something, and
/// threading two parameters through a recursive descent is how one of them ends
/// up forgotten.
pub struct Cx<'a> {
    /// Where identifiers are interned.
    pub names: &'a mut Names,
    /// Which language this is.
    pub goal: Goal,
    /// Every namespace name a `var N;` has already been emitted for, so a
    /// second `namespace N { … }` block in the same file merges into the first
    /// object instead of shadowing it with a second hoisted declaration.
    pub(crate) declared_namespaces: std::collections::HashSet<crate::names::Name>,
    /// The private names each enclosing class body declares, innermost last.
    ///
    /// A private name is resolved **lexically**, not by spelling: `#x` in one
    /// class and `#x` in another are different fields, and an inner class body
    /// may still name the outer class's. So the answer is a stack searched
    /// innermost-first, which is the same shape `emit::scope` uses for ordinary
    /// bindings and for the same reason.
    pub(crate) private_scopes: Vec<(u32, std::collections::HashSet<String>)>,
    /// How many class bodies have been entered, so each gets its own space.
    pub(crate) classes_seen: u32,
}

impl Cx<'_> {
    /// Intern a piece of source text.
    pub(crate) fn name(&mut self, text: &str) -> crate::names::Name {
        self.names.intern(text)
    }

    /// A private name, interned with the `#` it was written with.
    ///
    /// SWC hands over `x` for `#x`, so interning it plainly made `this.#x` and
    /// `this.x` the *same* name — one `Member` node, indistinguishable, for two
    /// things that are not the same at all: one is a property anyone can read
    /// and the other is reachable only from inside the class body. That is a
    /// misread rather than a missing feature, and it silently made a private
    /// field public.
    ///
    /// Keeping the `#` is enough to separate them everywhere at once, because a
    /// real property named `#x` cannot be reached by `.` — only by `o["#x"]`,
    /// which is a different node.
    pub(crate) fn private_name(&mut self, text: &str) -> crate::names::Name {
        // `@@#`, not `#`. The runtime excludes a reserved key from enumeration by
        // its PREFIX, and `#` alone is a prefix a program can write: `o["#main"]`
        // is an ordinary property and would have vanished from `Object.keys`.
        // `@@` is the space already reserved for keys no program can spell — see
        // `rts-core`'s `symbol` module — so a private name joins it rather
        // than opening a second one that collides with real text.
        //
        // The class's number is IN the key, which is what makes two classes
        // that both write `#x` two different fields. Interning by text alone
        // made them one, and that was a documented divergence rather than an
        // oversight: `class Box { #x = 7 }` and `class Sub extends Box { #x =
        // 20 }` answered `20:20` where every other engine answers `7:20`,
        // because the subclass's field overwrote the base's on the same object.
        //
        // Innermost-first, and a name declared nowhere takes the innermost
        // scope: an inner class body may legally read the OUTER class's private
        // field, and reading one no enclosing class declares is a SyntaxError
        // this crate does not check (`PLAN.md` L10) rather than a key to invent.
        let space = self
            .private_scopes
            .iter()
            .rev()
            .find(|(_, declared)| declared.contains(text))
            .or_else(|| self.private_scopes.last())
            .map_or(0, |(space, _)| *space);
        self.names.intern(&format!("@@#{space}#{text}"))
    }

    /// Enters a class body, with the private names it declares.
    ///
    /// The set is collected BEFORE any member is parsed, and it has to be: a
    /// method written first may read a `#y` declared last, and a stack filled as
    /// members are walked would still be empty when that read is interned.
    pub(crate) fn enter_class(&mut self, declared: std::collections::HashSet<String>) {
        self.classes_seen += 1;
        self.private_scopes.push((self.classes_seen, declared));
    }

    /// Leaves it.
    pub(crate) fn leave_class(&mut self) {
        self.private_scopes.pop();
    }
}

/// A source position from a SWC span.
///
/// SWC's `BytePos` is 1-based and 0 means "no position", which is also what
/// [`Position::UNKNOWN`] means — so the mapping is the identity and this
/// function exists to say that once, where it can be checked, rather than at
/// every call site.
pub(crate) fn position(span: swc_common::Span) -> Position {
    Position(span.lo.0)
}

/// Which language the source is written in.
///
/// Not a formality. TypeScript is a superset, so its syntax **accepts programs
/// JavaScript rejects** — `enum`, annotations, `x!`. Reading a `.js` file with
/// TypeScript syntax therefore turns some syntax errors into successful parses,
/// which is exactly wrong for a file the user called JavaScript.
///
/// Measured, not assumed: reading test262's `test/language` — pure JavaScript,
/// with 4170 files the corpus says must *fail* to parse — with TypeScript
/// syntax accepted 1269 of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Dialect {
    /// TypeScript, falling back to ECMAScript where the two disagree.
    #[default]
    TypeScript,
    /// ECMAScript only. What a `.js` file gets.
    JavaScript,
}

/// Parse a program, in the default (TypeScript) dialect.
pub fn parse(source: &str, goal: Goal, names: &mut Names) -> Result<Program> {
    parse_as(source, goal, Dialect::TypeScript, names)
}

/// Parse a program in a named dialect.
pub fn parse_as(source: &str, goal: Goal, dialect: Dialect, names: &mut Names) -> Result<Program> {
    let source = strip_shebang(source);

    let candidates: &[Syntax] = match dialect {
        Dialect::TypeScript => &[ts_syntax(), es_syntax()],
        Dialect::JavaScript => &[es_syntax()],
    };

    let mut first_error = None;
    for syntax in candidates {
        match parse_with(source, *syntax) {
            Ok(program) => {
                let mut cx = Cx {
                    names,
                    goal,
                    declared_namespaces: std::collections::HashSet::new(),
                    private_scopes: Vec::new(),
                    classes_seen: 0,
                };
                let tree = item::program(&mut cx, &program, goal)?;
                // An early error is a *syntax* error: a program with one is
                // refused before any of it runs. So it is reported here, on the
                // way out of parsing, rather than by whatever reads the tree —
                // by then something has already decided the program exists.
                if let Err(early) = crate::check::check(&tree, names) {
                    return Err(ParseError::Syntax(early.message));
                }
                return Ok(tree);
            }
            Err(message) => first_error.get_or_insert(message),
        };
    }

    Err(ParseError::Syntax(
        first_error.unwrap_or_else(|| "could not parse".to_owned()),
    ))
}

/// Parse a program as a module.
pub fn parse_module(source: &str, names: &mut Names) -> Result<Program> {
    parse(source, Goal::Module, names)
}

/// Parse a program as a script.
pub fn parse_script(source: &str, names: &mut Names) -> Result<Program> {
    parse(source, Goal::Script, names)
}

/// Remove a `#!` line, keeping its line break so line numbers do not shift.
///
/// Public because a HOST that wraps the source has to strip it FIRST: the
/// wrapper puts `function __rts_script() {` in front, and a `#!` that is no
/// longer at position zero is not a hashbang at all — it is a syntax error, and
/// that is what `syntax/315_hashbang.ts` was. Exported rather than restated
/// there, because two definitions of "what counts as a hashbang" is exactly the
/// drift rule 3 exists to prevent.
///
/// JavaScript has **four** line terminators, not one: line feed, carriage
/// return, and the two Unicode separators `U+2028` and `U+2029`. Looking only
/// for `\n` leaves the rest of a `#!` line attached to the program, which is a
/// syntax error in a file that is perfectly valid — found by test262's
/// `comments/hashbang/line-terminator-*` tests, which is what a corpus is for.
pub fn strip_shebang(source: &str) -> &str {
    let Some(rest) = source.strip_prefix("#!") else {
        return source;
    };
    match rest.find(['\n', '\r', '\u{2028}', '\u{2029}']) {
        Some(offset) => &source[offset + 2..],
        None => "",
    }
}

fn ts_syntax() -> Syntax {
    Syntax::Typescript(TsSyntax {
        tsx: false,
        // Only the grammar position — `@expr` before a class, method, property
        // or parameter — is SWC's concern; it hands the decorator expressions
        // back either way and takes no side on legacy `experimentalDecorators`
        // versus the TC39 form. Deciding between those two is `emit/`'s job,
        // and is where it is decided — see `emit/decorator.rs`.
        decorators: true,
        dts: false,
        no_early_errors: false,
        disallow_ambiguous_jsx_like: false,
    })
}

fn es_syntax() -> Syntax {
    Syntax::Es(EsSyntax {
        jsx: false,
        fn_bind: false,
        decorators: false,
        decorators_before_export: false,
        export_default_from: false,
        import_attributes: true,
        allow_super_outside_method: false,
        allow_return_outside_function: false,
        auto_accessors: false,
        explicit_resource_management: true,
    })
}

fn parse_with(source: &str, syntax: Syntax) -> std::result::Result<swc_ecma_ast::Program, String> {
    let map: Lrc<SourceMap> = Default::default();
    let file = map.new_source_file(
        Lrc::new(FileName::Custom("rts-input.ts".into())),
        source.to_string(),
    );
    let lexer = Lexer::new(syntax, Default::default(), StringInput::from(&*file), None);
    let mut parser = Parser::new_from(lexer);

    let parsed = parser
        .parse_program()
        .map_err(|error| error.kind().msg().to_string())?;

    if let Some(error) = parser.take_errors().into_iter().next() {
        return Err(error.kind().msg().to_string());
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shebang_is_removed_but_its_line_break_is_kept() {
        // The newline stays deliberately: the code after a shebang is on line 2,
        // and a diagnostic that moved it to line 1 would point at the wrong
        // line for every program that has one.
        assert_eq!(
            strip_shebang("#!/usr/bin/env rts\nlet x = 1;"),
            "\nlet x = 1;"
        );
        assert_eq!(strip_shebang("let x = 1;"), "let x = 1;");
        assert_eq!(strip_shebang("#!only"), "");
    }

    #[test]
    fn a_shebang_ends_at_any_of_the_four_line_terminators() {
        for terminator in ['\n', '\r', '\u{2028}', '\u{2029}'] {
            let source = format!("#!rts{terminator}let x = 1;");
            let stripped = strip_shebang(&source);
            assert!(
                stripped.ends_with("let x = 1;"),
                "{terminator:?} did not end the line: {stripped:?}"
            );
            assert!(!stripped.contains("rts"), "the shebang text survived");
        }
    }

    #[test]
    fn a_syntax_error_and_an_unsupported_construct_are_different_answers() {
        let mut names = Names::new();
        let broken = parse_script("let = = ;", &mut names);
        assert!(
            matches!(broken, Err(ParseError::Syntax(_))),
            "the program is wrong, and saying so is not the same as saying we cannot read it"
        );
    }
}

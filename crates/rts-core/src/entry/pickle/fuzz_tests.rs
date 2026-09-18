//! A fuzz test without `cargo-fuzz`: `deserialize` fed bytes nobody wrote.
//!
//! What it asserts is the whole of the reader's contract with a hostile
//! stream — it answers a value or a named refusal, and it does NOT panic,
//! abort, loop or ask for memory the stream cannot justify. A panic here would
//! be an abort in a program, because `deserialize` is reached from an
//! `extern "C"` frame, which cannot unwind.
//!
//! # The inputs
//!
//! Deterministic — a xorshift with a fixed seed, so a failure is a case that
//! can be run again rather than a flake — and derived from streams that are
//! valid to begin with, because random bytes rarely get past the magic: the
//! two goldens of `tests/claude-pickle-golden.test.ts` (v1, from the old
//! engine, and v2), and a v2 stream written here from an arena holding every
//! kind the format has, a cycle and a shared reference included. From each:
//! every truncation, byte flips, byte insertions and deletions. Then random
//! bodies behind a valid header, and random bytes with no header at all.
//!
//! `cargo-fuzz` was the alternative the v1 specification named as its next
//! phase, and it needs a nightly toolchain and a harness binary this workspace
//! does not have. A fixed corpus in the ordinary test run is weaker than a
//! coverage-guided search and is run on every `cargo test`, which the other is
//! not.

use super::super::clone::{ClassName, ErrorClass, Graph, Node, Slot};
use super::super::{Context, Singletons, with_context};
use super::Failure;
use crate::object::Key;
use crate::text::Str;
use crate::value::Value;

const GOLDEN_V1: &[u8] = &[
    82, 84, 83, 80, 1, 10, 16, 6, 116, 105, 116, 117, 108, 111, 1, 110, 1, 105, 4, 102, 108, 97,
    103, 4, 110, 97, 100, 97, 5, 105, 110, 100, 101, 102, 5, 108, 105, 115, 116, 97, 5, 111,
    117, 116, 114, 97, 6, 113, 117, 97, 110, 100, 111, 6, 112, 97, 100, 114, 97, 111, 4, 101,
    114, 114, 111, 4, 109, 97, 112, 97, 8, 99, 111, 110, 106, 117, 110, 116, 111, 3, 112, 101,
    116, 2, 102, 110, 5, 99, 105, 99, 108, 111, 7, 9, 103, 111, 108, 100, 101, 110, 32, 118, 49,
    5, 0, 0, 0, 0, 0, 0, 12, 64, 5, 0, 0, 0, 0, 0, 0, 28, 64, 3, 1, 0, 9, 4, 5, 0, 0, 0, 0, 0,
    0, 240, 63, 7, 4, 100, 111, 105, 115, 2, 10, 1, 2, 105, 100, 5, 0, 0, 0, 0, 0, 0, 69, 64, 8,
    4, 19, 4, 68, 97, 116, 101, 8, 0, 104, 229, 207, 139, 1, 0, 0, 19, 6, 82, 101, 103, 69, 120,
    112, 22, 4, 0, 0, 0, 97, 98, 43, 99, 2, 0, 0, 0, 103, 105, 0, 0, 0, 0, 0, 0, 0, 0, 21, 5,
    69, 114, 114, 111, 114, 4, 7, 109, 101, 115, 115, 97, 103, 101, 4, 110, 97, 109, 101, 5,
    115, 116, 97, 99, 107, 5, 99, 97, 117, 115, 101, 7, 11, 103, 111, 108, 100, 101, 110, 32,
    98, 111, 111, 109, 7, 5, 69, 114, 114, 111, 114, 0, 0, 21, 3, 77, 97, 112, 5, 5, 35, 107,
    101, 121, 115, 5, 35, 118, 97, 108, 115, 2, 35, 104, 3, 35, 110, 120, 5, 35, 109, 97, 115,
    107, 9, 2, 7, 1, 97, 7, 1, 98, 9, 2, 5, 0, 0, 0, 0, 0, 0, 240, 63, 5, 0, 0, 0, 0, 0, 0, 0,
    64, 9, 8, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0, 0,
    240, 191, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 0, 0, 6, 2, 5, 0, 0, 0, 0, 0,
    0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 240, 191, 9, 2, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0, 0,
    0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 28, 64, 21, 3, 83, 101, 116, 4, 6, 35, 105, 116,
    101, 109, 115, 2, 35, 104, 3, 35, 110, 120, 5, 35, 109, 97, 115, 107, 9, 2, 7, 1, 120, 7, 1,
    121, 9, 8, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0,
    0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 240, 191, 6, 2, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0, 0,
    0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 0, 0, 9, 2, 5, 0, 0, 0, 0, 0, 0, 240, 191, 5, 0, 0,
    0, 0, 0, 0, 240, 191, 5, 0, 0, 0, 0, 0, 0, 28, 64, 21, 4, 71, 67, 97, 111, 2, 4, 110, 111,
    109, 101, 4, 114, 97, 99, 97, 7, 3, 82, 101, 120, 7, 9, 118, 105, 114, 97, 45, 108, 97, 116,
    97, 22, 6, 103, 68, 111, 98, 114, 111, 8, 0
];

const GOLDEN_V2: &[u8] = &[
    82, 84, 83, 80, 2, 10, 16, 0, 6, 116, 105, 116, 117, 108, 111, 0, 1, 110, 0, 1, 105, 0, 4,
    102, 108, 97, 103, 0, 4, 110, 97, 100, 97, 0, 5, 105, 110, 100, 101, 102, 0, 5, 108, 105,
    115, 116, 97, 0, 5, 111, 117, 116, 114, 97, 0, 6, 113, 117, 97, 110, 100, 111, 0, 6, 112,
    97, 100, 114, 97, 111, 0, 4, 101, 114, 114, 111, 0, 4, 109, 97, 112, 97, 0, 8, 99, 111, 110,
    106, 117, 110, 116, 111, 0, 3, 112, 101, 116, 0, 2, 102, 110, 0, 5, 99, 105, 99, 108, 111,
    7, 0, 9, 103, 111, 108, 100, 101, 110, 32, 118, 49, 5, 0, 0, 0, 0, 0, 0, 12, 64, 6, 14, 3,
    1, 0, 9, 4, 6, 2, 7, 0, 4, 100, 111, 105, 115, 2, 10, 1, 0, 2, 105, 100, 6, 84, 0, 8, 2, 19,
    4, 68, 97, 116, 101, 8, 0, 104, 229, 207, 139, 1, 0, 0, 19, 6, 82, 101, 103, 69, 120, 112,
    22, 4, 0, 0, 0, 97, 98, 43, 99, 2, 0, 0, 0, 103, 105, 0, 0, 0, 0, 0, 0, 0, 0, 14, 0, 0, 5,
    69, 114, 114, 111, 114, 1, 7, 0, 11, 103, 111, 108, 100, 101, 110, 32, 98, 111, 111, 109, 0,
    23, 2, 7, 0, 1, 97, 6, 2, 7, 0, 1, 98, 6, 4, 24, 2, 7, 0, 1, 120, 7, 0, 1, 121, 21, 0, 0, 0,
    4, 71, 67, 97, 111, 0, 2, 0, 4, 110, 111, 109, 101, 0, 4, 114, 97, 99, 97, 7, 0, 3, 82, 101,
    120, 7, 0, 9, 118, 105, 114, 97, 45, 108, 97, 116, 97, 22, 26, 0, 6, 103, 68, 111, 98, 114,
    111, 8, 0
];

/// A small, fixed-seed generator — reproducible, which is the point.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }
}

fn fresh() -> Context {
    Context::new(
        Singletons { undefined: 0, null: 1, hole: 2 },
        crate::value::Kinds::in_declaration_order(),
    )
}

/// A v2 stream of an arena holding every kind the format writes — and, when
/// `undeclared`, an instance of a class no program here declares beside it,
/// whose read is always a refusal: a path worth feeding mutations to as well.
fn every_kind(context: &mut Context, undeclared: bool) -> Vec<u8> {
    let mut graph = Graph::default();
    let key = |context: &mut Context, text: &str| Key::Name(context.interner.intern_str(text, &mut context.keys));
    let (a, b, c) = (key(context, "a"), key(context, "b"), key(context, "c"));
    let hello = graph.text(Str::from_str("héllo ☃"));
    let lone = graph.text(Str::from_utf16(&[0xD800, 0x41]));
    let big = Slot::Bits(context.bigint_value(crate::bigint::BigInt::from_words(true, &[u64::MAX, 7])));
    let date = Slot::At(graph.push(Node::Date(1_700_000_000_000.0)));
    let regexp = Slot::At(graph.push(Node::Regexp { source: "a+b".into(), flags: "gi".into(), last_index: 3.0 }));
    let view = Slot::At(graph.push(Node::View {
        kind: super::super::buffers::element::Kind::Float64,
        bytes: 1.5f64.to_le_bytes().to_vec(),
    }));
    let buffer = Slot::At(graph.push(Node::Buffer(vec![1, 2, 3])));
    let node_buffer = Slot::At(graph.push(Node::NodeBuffer(vec![4, 5])));
    let boxed = Slot::At(graph.push(Node::Boxed(Slot::Bits(Value::from_bool(true).bits()))));
    let error = Slot::At(graph.push(Node::Error {
        class: ErrorClass::Builtin("RangeError"),
        message: Some(hello),
        stack: None,
        cause: Some(Slot::Bits(Value::from_i32(-5).bits())),
        extra: vec![(c, lone)],
    }));
    let set = Slot::At(graph.push(Node::Set(vec![hello, Slot::Bits(Value::from_f64(1.5).bits())])));
    let root = graph.push(Node::Set(Vec::new()));
    let map = Slot::At(graph.push(Node::Map(vec![(Slot::At(root), set), (hello, big)])));
    let hole = Slot::Bits(Value::from_singleton(2).bits());
    // In an ARRAY, where it survives — a `Set` stores `-0` as `0`, which is the
    // language (`SameValueZero`), so a round trip through one would not be
    // byte-identical and should not be.
    let negative_zero = Slot::Bits(Value::from_f64(-0.0).bits());
    let array = Slot::At(graph.push(Node::Array {
        elements: vec![date, regexp, hole, view, buffer, node_buffer, boxed, error, Slot::At(root), negative_zero],
        extra: vec![(a, Slot::Bits(Value::from_f64(f64::NAN).bits()))],
    }));
    graph.nodes[root] = Node::Object(vec![(a, array), (b, map), (c, Slot::At(root))]);
    if !undeclared {
        return super::write::write(context, &graph, Slot::At(root)).expect("a well-formed arena writes");
    }
    let instance = graph.push(Node::Instance {
        class: ClassName { module: Str::from_str("m.ts"), name: Str::from_str("Nope"), prototype: 0, version: 3 },
        fields: vec![(a, Slot::At(root))],
    });
    let wrapper = graph.push(Node::Array { elements: vec![Slot::At(root), Slot::At(instance)], extra: Vec::new() });
    super::write::write(context, &graph, Slot::At(wrapper)).expect("a well-formed arena writes")
}

/// Every input the test feeds, derived from the seeds by the generator.
fn cases(seeds: &[Vec<u8>], random: &mut Xorshift) -> Vec<Vec<u8>> {
    let mut all = Vec::new();
    for seed in seeds {
        all.push(seed.clone());
        for length in 0..seed.len() {
            all.push(seed[..length].to_vec());
        }
        for _ in 0..600 {
            let mut mutated = seed.clone();
            for _ in 0..1 + random.below(4) {
                match random.below(3) {
                    0 if !mutated.is_empty() => {
                        let at = random.below(mutated.len());
                        mutated[at] = random.next() as u8;
                    }
                    1 => {
                        let at = random.below(mutated.len() + 1);
                        mutated.insert(at, random.next() as u8);
                    }
                    _ if !mutated.is_empty() => {
                        let at = random.below(mutated.len());
                        mutated.remove(at);
                    }
                    _ => {}
                }
            }
            all.push(mutated);
        }
    }
    for version in [1u8, 2] {
        for _ in 0..500 {
            let mut body = b"RTSP".to_vec();
            body.push(version);
            for _ in 0..random.below(64) {
                // Biased toward opcodes and small counts, which is where the
                // reader's decisions are; a uniform byte is mostly an unknown
                // opcode and stops at once.
                body.push(match random.below(3) {
                    0 => random.below(26) as u8,
                    1 => random.below(4) as u8,
                    _ => random.next() as u8,
                });
            }
            all.push(body);
        }
    }
    for _ in 0..300 {
        all.push((0..random.below(48)).map(|_| random.next() as u8).collect());
    }
    all
}

#[test]
fn a_hostile_stream_answers_a_value_or_a_named_refusal_and_never_panics() {
    let mut context = fresh();
    let seeds = vec![
        GOLDEN_V1.to_vec(),
        GOLDEN_V2.to_vec(),
        every_kind(&mut context, false),
        every_kind(&mut context, true),
    ];
    let mut random = Xorshift(0x9E37_79B9_7F4A_7C15);
    let inputs = cases(&seeds, &mut random);
    assert!(inputs.len() > 3_000, "the corpus is what the test claims: {} cases", inputs.len());

    let (_, outcomes) = with_context(context, || {
        let mut values = 0usize;
        let mut refusals = 0usize;
        for input in &inputs {
            match super::unpickle(input) {
                Ok(_) => values += 1,
                Err(Failure::Refused(message)) => {
                    assert!(!message.is_empty(), "a refusal names what was wrong");
                    refusals += 1;
                }
                Err(Failure::Thrown) => panic!("nothing in a stream can make a read run user code"),
            }
        }
        (values, refusals)
    });
    let (values, refusals) = outcomes;
    // Both paths exercised: a corpus that only ever refused would say nothing
    // about building, and one that only ever built nothing about refusing.
    assert!(values > 20 && refusals > 1_000, "{values} values, {refusals} refusals");
}

#[test]
fn the_written_stream_reads_back_to_the_same_bytes() {
    // The seed above is only a seed if it is valid: read it, write what was
    // read, and get the same bytes — determinism and round trip in one check.
    let mut context = fresh();
    let written = every_kind(&mut context, false);
    let (context, again) = with_context(context, || {
        let value = super::unpickle(&written).map_err(|failed| format!("{failed:?}"));
        value.and_then(|value| super::pickle_value(value).map_err(|failed| format!("{failed:?}")))
    });
    drop(context);
    let again = again.expect("the arena's own stream round-trips");
    assert_eq!(again, written);
}

#[test]
fn nesting_is_bounded_by_a_count_and_not_by_the_stack() {
    // One-element arrays inside one another: two bytes a level, which is the
    // cheapest nesting a hostile stream can write.
    let nested = |levels: usize| {
        let mut bytes = b"RTSP\x02".to_vec();
        for _ in 0..levels {
            bytes.extend_from_slice(&[super::format::OP_ARRAY, 1]);
        }
        bytes.push(super::format::OP_NULL);
        // Every array closes with an empty count of extra members.
        bytes.extend(std::iter::repeat_n(0u8, levels));
        bytes
    };
    let (_, answers) = with_context(fresh(), || {
        let deep = super::unpickle(&nested(50_000)).is_ok();
        let too_deep = matches!(
            super::unpickle(&nested(super::format::MAX_DEPTH + 1)),
            Err(Failure::Refused(message)) if message.contains("nesting")
        );
        (deep, too_deep)
    });
    assert_eq!(answers, (true, true), "fifty thousand levels read; past the ceiling is refused by name");
}

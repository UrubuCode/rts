#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicKind {
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intrinsic {
    pub name: &'static str,
    pub kind: IntrinsicKind,
}

pub const INTRINSICS: &[Intrinsic] = &[
    Intrinsic {
        name: "io",
        kind: IntrinsicKind::Global,
    },
    Intrinsic {
        name: "fs",
        kind: IntrinsicKind::Global,
    },
    Intrinsic {
        name: "process",
        kind: IntrinsicKind::Global,
    },
    Intrinsic {
        name: "crypto",
        kind: IntrinsicKind::Global,
    },
    Intrinsic {
        name: "global",
        kind: IntrinsicKind::Global,
    },
    Intrinsic {
        name: "buffer",
        kind: IntrinsicKind::Global,
    },
    Intrinsic {
        name: "promise",
        kind: IntrinsicKind::Global,
    },
    Intrinsic {
        name: "task",
        kind: IntrinsicKind::Global,
    },
];

pub fn is_intrinsic(name: &str) -> bool {
    INTRINSICS.iter().any(|intrinsic| intrinsic.name == name)
}

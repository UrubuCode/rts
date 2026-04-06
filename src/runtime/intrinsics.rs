#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicKind {
    Function,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intrinsic {
    pub name: &'static str,
    pub kind: IntrinsicKind,
}

pub const INTRINSICS: &[Intrinsic] = &[
    Intrinsic {
        name: "alloc",
        kind: IntrinsicKind::Function,
    },
    Intrinsic {
        name: "dealloc",
        kind: IntrinsicKind::Function,
    },
    Intrinsic {
        name: "panic",
        kind: IntrinsicKind::Function,
    },
    Intrinsic {
        name: "print",
        kind: IntrinsicKind::Function,
    },
    Intrinsic {
        name: "clockNow",
        kind: IntrinsicKind::Function,
    },
    Intrinsic {
        name: "process",
        kind: IntrinsicKind::Global,
    },
];

pub fn is_intrinsic(name: &str) -> bool {
    INTRINSICS.iter().any(|intrinsic| intrinsic.name == name)
}

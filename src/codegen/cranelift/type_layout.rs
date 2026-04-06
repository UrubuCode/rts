#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeLayout {
    pub size: usize,
    pub align: usize,
}

impl TypeLayout {
    pub fn primitive(size: usize) -> Self {
        Self { size, align: size }
    }
}

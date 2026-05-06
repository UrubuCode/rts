pub mod ir;
pub mod type_map;
pub mod type_refine;
pub mod scope;
pub mod lower;

#[cfg(test)]
mod tests;

pub use ir::{
    HirFunc, HirStmt, HirExpr, HirLit, HirType, HirBinOp, HirUnOp,
    HirParam, ClassId, HandleKind,
};
pub use type_map::CraneliftTypeHint;
pub use type_refine::{refine_type_from_init, numeric_promotion};

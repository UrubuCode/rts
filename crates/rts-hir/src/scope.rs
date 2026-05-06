use std::collections::HashMap;
use crate::ir::{HirType, ClassId};

/// Lexical scope for HIR type resolution.
///
/// Tracks variable types and function return types encountered during the
/// AST → HIR lowering pass. The scope is nested: each block pushes a frame
/// and pops it on exit.
#[derive(Debug, Default)]
pub struct Scope {
    /// Stack of variable name → type frames (innermost last).
    frames: Vec<HashMap<String, HirType>>,
    /// Function return types registered at the top of each function.
    return_types: HashMap<String, HirType>,
    /// Function parameter types — used by the MIR lowering to resolve
    /// `Call` instructions to a typed `CallUser` form.
    param_types: HashMap<String, Vec<HirType>>,
    /// Class name → ClassId registry.
    classes: HashMap<String, ClassId>,
    next_class_id: u32,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            frames: vec![HashMap::new()],
            ..Default::default()
        }
    }

    pub fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    pub fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    pub fn define(&mut self, name: impl Into<String>, ty: HirType) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.into(), ty);
        }
    }

    /// Look up the type of a variable in the scope chain.
    pub fn lookup(&self, name: &str) -> Option<&HirType> {
        for frame in self.frames.iter().rev() {
            if let Some(ty) = frame.get(name) {
                return Some(ty);
            }
        }
        None
    }

    pub fn register_return_type(&mut self, fn_name: impl Into<String>, ty: HirType) {
        self.return_types.insert(fn_name.into(), ty);
    }

    pub fn return_type_of(&self, fn_name: &str) -> Option<HirType> {
        self.return_types.get(fn_name).cloned()
    }

    pub fn register_param_types(&mut self, fn_name: impl Into<String>, tys: Vec<HirType>) {
        self.param_types.insert(fn_name.into(), tys);
    }

    pub fn param_types_of(&self, fn_name: &str) -> Option<&[HirType]> {
        self.param_types.get(fn_name).map(|v| v.as_slice())
    }

    /// Iterate registered function signatures `(name, param_tys, ret_ty)`.
    /// Skips functions whose param types weren't registered.
    pub fn iter_fn_sigs(&self) -> impl Iterator<Item = (&str, &[HirType], HirType)> {
        self.param_types.iter().map(move |(name, params)| {
            let ret = self
                .return_types
                .get(name)
                .cloned()
                .unwrap_or(HirType::Unknown);
            (name.as_str(), params.as_slice(), ret)
        })
    }

    pub fn resolve_class(&self, name: &str) -> Option<ClassId> {
        self.classes.get(name).copied()
    }

    pub fn register_class(&mut self, name: impl Into<String>) -> ClassId {
        let id = ClassId(self.next_class_id);
        self.next_class_id += 1;
        self.classes.insert(name.into(), id);
        id
    }
}

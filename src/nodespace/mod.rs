use crate::abi::AbiType;

pub mod fs;
pub mod os;
pub mod path;
pub mod process;

pub struct NodespaceSpec {
    pub node_module: &'static str,
    pub ns_prefix: &'static str,
    pub members: &'static [NodespaceMember],
}

pub struct NodespaceMember {
    pub name: &'static str,
    pub symbol: &'static str,
    pub args: &'static [AbiType],
    pub returns: AbiType,
}

pub const NODE_SPECS: &[&NodespaceSpec] = &[&fs::SPEC, &path::SPEC, &os::SPEC, &process::SPEC];

/// Resolves a codegen-qualified name like `"node_fs.readFileSync"` to its member.
pub(crate) fn node_lookup(qualified: &str) -> Option<&'static NodespaceMember> {
    let (ns_prefix, fn_name) = qualified.split_once('.')?;
    let module_name = ns_prefix.strip_prefix("node_")?;
    let spec = NODE_SPECS
        .iter()
        .copied()
        .find(|s| s.node_module == module_name)?;
    spec.members.iter().find(|m| m.name == fn_name)
}

/// Maps a `node:` import specifier to its codegen ns_prefix.
/// e.g. `"node:fs"` → `"node_fs"`
pub fn ns_prefix_for(specifier: &str) -> Option<&'static str> {
    let module_name = specifier.strip_prefix("node:")?;
    NODE_SPECS
        .iter()
        .copied()
        .find(|s| s.node_module == module_name)
        .map(|s| s.ns_prefix)
}

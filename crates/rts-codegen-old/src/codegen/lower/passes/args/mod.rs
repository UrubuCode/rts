//! Passes que reescrevem callsites pra alinhar args com assinatura
//! do callee: defaults, rest variadicos e spread literal.

pub mod arguments_object;
pub mod default_args;
pub mod new_collection_arg;
pub mod rest_args;
pub mod spread_args;

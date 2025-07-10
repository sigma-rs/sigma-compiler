use proc_macro2::TokenStream;

/// The submodules that would be useful to have in the lower-level
/// `sigma` crate are for now included as submodules of a local `sigma`
/// module
pub mod sigma {
    pub mod codegen;
    pub mod combiners;
    pub mod types;
}
mod codegen;
mod pedersen;
mod rangeproof;
mod substitution;
mod syntax;
mod transform;

pub use syntax::{SigmaCompSpec, TaggedIdent, TaggedPoint, TaggedScalar, TaggedVarDict};

/// The main function of this macro.
///
/// Parse the macro input with [`parse`](SigmaCompSpec#method.parse) to
/// produce a [`SigmaCompSpec`], and then pass that to this function to
/// output the data structures and code for the ZKP protocol
/// implementation.
///
/// If `emit_prover` is `true`, output the data structures and code for
/// the prover side.  If `emit_verifier` is `true`, output the data
/// structures and code for the verifier side.  (Typically both will be
/// `true`, but you can set one to `false` if you don't need that side
/// of the protocol.)
pub fn sigma_compiler_core(
    spec: &mut SigmaCompSpec,
    emit_prover: bool,
    emit_verifier: bool,
) -> TokenStream {
    let mut codegen = codegen::CodeGen::new(spec);

    // Apply any substitution transformations
    substitution::transform(&mut codegen, &mut spec.statements, &mut spec.vars).unwrap();

    // Apply any range statement transformations
    rangeproof::transform(&mut codegen, &mut spec.statements, &mut spec.vars).unwrap();

    codegen.generate(spec, emit_prover, emit_verifier)
}

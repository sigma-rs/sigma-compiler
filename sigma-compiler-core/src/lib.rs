use proc_macro2::TokenStream;

/// The submodules that would be useful to have in the lower-level
/// `sigma-proofs` crate are for now included as submodules of a local
/// `sigma` module.
pub mod sigma {
    pub mod codegen;
    pub mod combiners;
    pub mod types;
}
mod codegen;
mod notequals;
mod pedersen;
mod pubscalareq;
mod rangeproof;
mod substitution;
mod syntax;
mod transform;

pub use codegen::CodeGen;
pub use syntax::{SigmaCompSpec, TaggedIdent, TaggedPoint, TaggedScalar, TaggedVarDict};

use syn::Result;

/// Transform the [`StatementTree`] so that it satisfies the
/// [disjunction invariant].
///
/// [`StatementTree`]: sigma::combiners::StatementTree
/// [disjunction invariant]: sigma::combiners::StatementTree::check_disjunction_invariant
pub fn enforce_disjunction_invariant(
    codegen: &mut CodeGen,
    spec: &mut SigmaCompSpec,
) -> Result<()> {
    transform::enforce_disjunction_invariant(codegen, &mut spec.statements, &mut spec.vars)
}

/// Apply all of the compiler transformations.
///
/// The [disjunction invariant] must be true before calling this
/// function, and will remain true after each transformation (and at the
/// end of this function).  Call [enforce_disjunction_invariant] before
/// calling this function if you're not sure the disjunction invariant
/// already holds.
///
/// [disjunction invariant]: sigma::combiners::StatementTree::check_disjunction_invariant
pub fn apply_transformations(codegen: &mut CodeGen, spec: &mut SigmaCompSpec) -> Result<()> {
    // Apply any substitution transformations
    substitution::transform(codegen, &mut spec.statements, &mut spec.vars)?;

    // Apply any range statement transformations
    rangeproof::transform(codegen, &mut spec.statements, &mut spec.vars)?;

    // Apply any not-equals statement transformations
    notequals::transform(codegen, &mut spec.statements, &mut spec.vars)?;

    // Apply any public scalar equality transformations
    pubscalareq::transform(codegen, &mut spec.statements, &mut spec.vars)?;

    Ok(())
}

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

    // Enforce the disjunction invariant (do this before any other
    // transformations, since they assume the invariant holds, and will
    // maintain it)
    enforce_disjunction_invariant(&mut codegen, spec).unwrap();

    // Apply the transformations
    apply_transformations(&mut codegen, spec).unwrap();

    // Generate the code to be output
    codegen.generate(spec, emit_prover, emit_verifier)
}

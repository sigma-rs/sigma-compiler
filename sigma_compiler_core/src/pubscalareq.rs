//! A module to look for, and apply, any statement involving the
//! equality of _public_ `Scalar`s.
//!
//! Such a statement is of the form `a = 2*(c+1)` where `a` and `c` are
//! public `Scalar`s.  That is, it is a single variable name (which must
//! be a public `Scalar`, as specified in the provided
//! [`TaggedVarDict`]), an equal sign, and an [arithmetic expression]
//! involving other public `Scalar` variables, constants, parens, and
//! the operators `+`, `-`, and `*`.
//!
//! The statement is simply removed from the list of statements to be
//! proven in the zero-knowledge sigma protocol, and code is emitted for
//! the prover and verifier to each just check that the statement is
//! satisfied.
//!
//! [arithmetic expression]: super::sigma::types::expr_type

use super::codegen::CodeGen;
use super::sigma::combiners::*;
use super::sigma::types::{expr_type_tokens, AExprType};
use super::syntax::taggedvardict_to_vardict;
use super::transform::prune_statement_tree;
use super::{TaggedIdent, TaggedScalar, TaggedVarDict};
use quote::quote;
use syn::{parse_quote, Expr, Result};

/// Look for, and apply, all of the public scalar equality statements
/// specified in leaves of the [`StatementTree`].
pub fn transform(
    codegen: &mut CodeGen,
    st: &mut StatementTree,
    vars: &mut TaggedVarDict,
) -> Result<()> {
    // Construct the VarDict corresponding to vars
    let vardict = taggedvardict_to_vardict(vars);

    // Gather mutable references to all Exprs in the leaves of the
    // StatementTree.  Note that this ignores the combiner structure in
    // the StatementTree, but that's fine.
    let mut leaves = st.leaves_mut();

    // For each leaf expression, see if it looks like a public Scalar
    // equality statement
    for leafexpr in leaves.iter_mut() {
        if let Expr::Assign(syn::ExprAssign { left, right, .. }) = *leafexpr {
            if let Expr::Path(syn::ExprPath { path, .. }) = left.as_ref() {
                if let Some(id) = path.get_ident() {
                    let idstr = id.to_string();
                    if let Some(TaggedIdent::Scalar(TaggedScalar { is_pub: true, .. })) =
                        vars.get(&idstr)
                    {
                        if let (AExprType::Scalar { is_pub: true, .. }, right_tokens) =
                            expr_type_tokens(&vardict, right)?
                        {
                            // We found a public Scalar equality
                            // statement.  Add code to both the prover
                            // and the verifier to check the statement.
                            codegen.prove_append(quote! {
                                if #id != #right_tokens {
                                    return Err(SigmaError::VerificationFailure);
                                }
                            });
                            codegen.verify_append(quote! {
                                if #id != #right_tokens {
                                    return Err(SigmaError::VerificationFailure);
                                }
                            });

                            // Remove the statement from the
                            // [`StatementTree`] by replacing it with
                            // leaf_true (which will be pruned below).
                            let mut expr: Expr = parse_quote! { true };
                            std::mem::swap(&mut expr, *leafexpr);
                        }
                    }
                }
            }
        }
    }

    // Now prune the StatementTree
    prune_statement_tree(st);

    Ok(())
}

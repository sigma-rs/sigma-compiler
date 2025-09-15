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
use super::syntax::{collect_cind_points, taggedvardict_to_vardict};
use super::transform::prune_statement_tree;
use super::{TaggedIdent, TaggedScalar, TaggedVarDict};
use quote::quote;
use syn::{parse_quote, Error, Expr, Result};

/// Look for, and apply, all of the public scalar equality statements
/// specified in leaves of the [`StatementTree`].
#[allow(non_snake_case)] // so that Points can be capital letters
pub fn transform(
    codegen: &mut CodeGen,
    st: &mut StatementTree,
    vars: &mut TaggedVarDict,
) -> Result<()> {
    // Construct the VarDict corresponding to vars
    let vardict = taggedvardict_to_vardict(vars);

    // A list of the computationally independent (non-vector) Points in
    // the macro input.  There must be at least one of them in order to
    // handle public scalar equality statements inside disjunctions.
    let cind_points = collect_cind_points(vars);

    st.for_each_disjunction_branch(&mut |branch, path| {
        // Are we in the root disjunction branch?  (path is empty)
        let in_root_disjunction_branch = path.is_empty();

        // For each leaf expression, see if it looks like a public Scalar
        // equality statement
        branch.for_each_disjunction_branch_leaf(&mut |leaf| {
            if let StatementTree::Leaf(Expr::Assign(syn::ExprAssign { left, right, .. })) = leaf {
                if let Expr::Path(syn::ExprPath { path, .. }) = left.as_ref() {
                    if let Some(id) = path.get_ident() {
                        let idstr = id.to_string();
                        if let Some(TaggedIdent::Scalar(TaggedScalar {
                            is_pub: true,
                            is_vec: l_is_vec,
                            ..
                        })) = vars.get(&idstr)
                        {
                            if let (
                                AExprType::Scalar {
                                    is_pub: true,
                                    is_vec: r_is_vec,
                                    ..
                                },
                                right_tokens,
                            ) = expr_type_tokens(&vardict, right)?
                            {
                                if *l_is_vec != r_is_vec {
                                    return Err(Error::new(
                                        proc_macro2::Span::call_site(),
                                        "Only one side of the public equality statement is a vector",
                                    ));
                                }
                                // We found a public Scalar equality
                                // statement.
                                if in_root_disjunction_branch {
                                    // If we're in the root disjunction branch,
                                    // add code to both the prover and the
                                    // verifier to directly check the statement.
                                    codegen.prove_verify_append(quote! {
                                        if #id != #right_tokens {
                                            return Err(SigmaError::VerificationFailure);
                                        }
                                    });

                                    // Remove the statement from the
                                    // [`StatementTree`] by replacing it with
                                    // leaf_true (which will be pruned below).
                                    *leaf = StatementTree::leaf_true();
                                } else {
                                    // If we're not in the root disjunction
                                    // branch, replace the statement
                                    // `left_id = right_side` with the
                                    // statement `left_id*A =
                                    // (right_side)*A` for a cind Point A.
                                    if cind_points.is_empty() {
                                        return Err(Error::new(
                                            proc_macro2::Span::call_site(),
                                            "At least one cind Point must be declared to support public Scalar equality statements inside disjunctions",
                                        ));
                                    }
                                    let cind_A = &cind_points[0];

                                    *leaf = StatementTree::Leaf(parse_quote! {
                                        #id * #cind_A = (#right) * #cind_A
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    })?;

    // Now prune the StatementTree
    prune_statement_tree(st);

    Ok(())
}

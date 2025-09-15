//! A module to look for, and apply, all of the _substitutions_
//! specified in leaves of the [`StatementTree`].
//!
//! A _substitution_ is a statement of the form `a = b` or `b = 2*(c + 1)`.
//! That is, it is a single variable name (which must be a private
//! `Scalar`, as specified in the provided [`TaggedVarDict`]), an equal
//! sign, and an [arithmetic expression] involving other `Scalar`
//! variables, constants, parens, and the operators `+`, `-`, and `*`.
//!
//! Applying a substitution means replacing the variable to the left of
//! the `=` with the expression on the right of the `=` everywhere it
//! appears in the [`StatementTree`].  Any given variable may only be
//! substituted once in a [`StatementTree`].
//!
//! The expression on the right must not contain the variable on the
//! left, either directly or after other substitutions.  For example,
//! `a = a + b` is not allowed, nor is the combination of substitutions
//! `a = b + 1, b = c + 2, c = 2*a`.
//!
//! After a substitution is applied, the substituted variable will no
//! longer appear anywhere in the [`StatementTree`], and will be removed
//! from the [`TaggedVarDict`].  The leaves of the [`StatementTree`]
//! containing the substitution statements themselves will be turned
//! into the constant `true` and then pruned using
//! [`prune_statement_tree`].  The [`CodeGen`] will be used to generate
//! tests in the generated `prove` function that the `Instance` and
//! `Witness` supplied to it do in fact satisfy the statements being
//! substituted.
//!
//! It is the case that if the [disjunction invariant] is satisfied
//! before [`transform`] is called (and the caller must ensure that it
//! is), then it will be satisfied after the substitutions are applied,
//! and then also after the [`StatementTree`] is pruned.
//!
//! [arithmetic expression]: super::sigma::types::expr_type
//! [disjunction invariant]: StatementTree::check_disjunction_invariant

use super::codegen::CodeGen;
use super::sigma::combiners::*;
use super::sigma::types::expr_type_tokens;
use super::syntax::taggedvardict_to_vardict;
use super::transform::{paren_if_needed, prune_statement_tree};
use super::{TaggedIdent, TaggedScalar, TaggedVarDict};
use quote::quote;
use std::collections::{HashSet, VecDeque};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::visit_mut::{self, VisitMut};
use syn::{Error, Expr, Ident, Result};

/// Produce a [`HashSet`] of the private `Scalar`s appearing in the
/// provided [`Expr`], as specified in the provided [`TaggedVarDict`].
fn priv_scalar_set(e: &Expr, taggedvardict: &TaggedVarDict) -> HashSet<String> {
    let mut set: HashSet<String> = HashSet::new();
    let vardict = taggedvardict_to_vardict(taggedvardict);
    let mut priv_map = PrivScalarMap {
        vars: &vardict,
        closure: &mut |ident| {
            set.insert(ident.to_string());
            Ok(())
        },
        result: Ok(()),
    };
    priv_map.visit_expr(e);
    set
}

/// Apply a single substitution on an [`Expr`].
///
/// Replace all instances of the [`struct@Ident`] given by the string
/// `idstr` in `expr` with a copy of `replacement`.
fn do_substitution<'a>(expr: &mut Expr, idstr: &'a str, replacement: &'a Expr) {
    struct Subs<'a> {
        idstr: &'a str,
        replacement: &'a Expr,
    }

    impl<'a> VisitMut for Subs<'a> {
        fn visit_expr_mut(&mut self, node: &mut Expr) {
            if let Expr::Path(expath) = node {
                if let Some(id) = expath.path.get_ident() {
                    if id.to_string().as_str() == self.idstr {
                        *node = self.replacement.clone();
                        return;
                    }
                }
            }
            // Unless we bailed out above, continue with the default
            // traversal
            visit_mut::visit_expr_mut(self, node);
        }
    }

    let mut subs = Subs { idstr, replacement };
    subs.visit_expr_mut(expr);
}

/// Look for, and apply, all of the substitutions specified in leaves
/// of the [`StatementTree`].
pub fn transform(
    codegen: &mut CodeGen,
    st: &mut StatementTree,
    vars: &mut TaggedVarDict,
) -> Result<()> {
    // Construct the VarDict corresponding to vars
    let vardict = taggedvardict_to_vardict(vars);

    let mut subs: VecDeque<(Ident, Expr, HashSet<String>)> = VecDeque::new();
    let mut subs_vars: HashSet<String> = HashSet::new();

    st.for_each_disjunction_branch(&mut |branch, path| {
        // Are we in the root disjunction branch?  (path is empty)
        let in_root_disjunction_branch = path.is_empty();

        // For each leaf expression, see if it looks like a substitution of
        // a private Scalar
        branch.for_each_disjunction_branch_leaf(&mut |leaf| {
            let mut is_subs = None;
            if let StatementTree::Leaf(Expr::Assign(syn::ExprAssign { left, .. })) = leaf {
                if let Expr::Path(syn::ExprPath { path, .. }) = left.as_ref() {
                    if let Some(id) = path.get_ident() {
                        let idstr = id.to_string();
                        if let Some(TaggedIdent::Scalar(TaggedScalar { is_pub: false, .. })) =
                            vars.get(&idstr)
                        {
                            is_subs = Some(id.clone());
                        }
                    }
                }
            }
            if let Some(id) = is_subs {
                // If this leaf is a substitution of a private Scalar, add
                // it to subs, replace it in the StatementTree with the
                // constant true, and generate some code for `prove` to
                // check the statement.
                let old_leaf = std::mem::replace(leaf, StatementTree::leaf_true());
                // This "if let" is guaranteed to succeed
                if let StatementTree::Leaf(Expr::Assign(syn::ExprAssign { right, .. })) = old_leaf {
                    if let Ok((_, right_tokens)) = expr_type_tokens(&vardict, &right) {
                        let used_priv_scalars = priv_scalar_set(&right, vars);
                        if !subs_vars.insert(id.to_string()) {
                            return Err(Error::new(
                                id.span(),
                                "variable substituted multiple times",
                            ));
                        }
                        // Only if we're in the root disjunction branch,
                        // check whether the substituted Witness value
                        // actually equals the value it's being substituted
                        // for.  We can't do this for substitutions in other
                        // disjunction branches, since it may not be true
                        // there.
                        if in_root_disjunction_branch {
                            codegen.prove_append(quote! {
                                // It's OK to have a test that observably fails
                                // for illegal inputs (but is constant time for
                                // valid inputs)
                                if #id != #right_tokens {
                                    return Err(SigmaError::VerificationFailure);
                                }
                            });
                        }
                        let right = paren_if_needed(*right);
                        subs.push_back((id, right, used_priv_scalars));
                    } else {
                        return Err(Error::new(
                            right.span(),
                            format!(
                                "Unrecognized arithmetic expression in substitution: {} = {}",
                                id,
                                quote! {#right}
                            ),
                        ));
                    }
                }
            }
            Ok(())
        })
    })?;

    // Now apply each substitution to both the StatementTree and also
    // the remaining substitutions
    while !subs.is_empty() {
        let (id, expr, priv_vars) = subs.pop_front().unwrap();
        let idstr = id.to_string();
        if priv_vars.contains(&idstr) {
            return Err(Error::new(
                id.span(),
                "variable appears in its own substitution",
            ));
        }
        // Do the substitution on each remaining substitution in the
        // list
        for (_sid, sexpr, spriv_vars) in subs.iter_mut() {
            if spriv_vars.contains(&idstr) {
                do_substitution(sexpr, &idstr, &expr);
                spriv_vars.remove(&idstr);
                spriv_vars.extend(priv_vars.clone().into_iter());
            }
        }
        // Do the substitution on each leaf Expr in the StatementTree
        for leafexpr in st.leaves_mut().iter_mut() {
            do_substitution(leafexpr, &idstr, &expr);
        }
        // Remove the substituted variable from the TaggedVarDict
        vars.remove(&idstr);
    }

    // Now prune the StatementTree
    prune_statement_tree(st);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::syntax::taggedvardict_from_strs;
    use super::*;
    use syn::parse_quote;

    fn substitution_tester(
        vars: (&[&str], &[&str]),
        e: Expr,
        subbed_vars: (&[&str], &[&str]),
        subbed_e: Expr,
    ) -> Result<()> {
        let mut taggedvardict = taggedvardict_from_strs(vars);
        let mut st = StatementTree::parse(&e).unwrap();
        let mut codegen = CodeGen::new_empty();
        transform(&mut codegen, &mut st, &mut taggedvardict)?;
        let subbed_taggedvardict = taggedvardict_from_strs(subbed_vars);
        let subbed_st = StatementTree::parse(&subbed_e).unwrap();
        assert_eq!(st, subbed_st);
        assert_eq!(taggedvardict, subbed_taggedvardict);
        Ok(())
    }

    #[test]
    fn apply_substitutions_test() {
        let vars_a = (["a", "b", "pub c"].as_slice(), ["A", "B", "C"].as_slice());

        // No substitutions (left side of = is a Point, not a Scalar)
        substitution_tester(
            vars_a,
            parse_quote! {
                A = b*B + c*C
            },
            vars_a,
            parse_quote! {
                A = b*B + c*C
            },
        )
        .unwrap();
        substitution_tester(
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    B = a*A + c*C,
                )
            },
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    B = a*A + c*C,
                )
            },
        )
        .unwrap();

        // No substitutions (the left side of the = is public, not
        // private)
        substitution_tester(
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    c = a,
                )
            },
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    c = a,
                )
            },
        )
        .unwrap();

        // Error: same variable substituted more than once
        substitution_tester(
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    a = c,
                    a = b,
                )
            },
            vars_a,
            parse_quote! { true },
        )
        .unwrap_err();

        // Error: same variable substituted more than once
        substitution_tester(
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    a = c,
                    a = b,
                )
            },
            vars_a,
            parse_quote! { true },
        )
        .unwrap_err();

        // Error: variable appears in its own substitution (directly)
        substitution_tester(
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    a = 2*a + 1,
                )
            },
            vars_a,
            parse_quote! { true },
        )
        .unwrap_err();

        // Error: variable appears in its own substitution (indirectly)
        substitution_tester(
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    a = 2*b + 1,
                    b = a + 4,
                )
            },
            vars_a,
            parse_quote! { true },
        )
        .unwrap_err();

        // Successful substitutions

        let vars_nob = (["a", "pub c"].as_slice(), ["A", "B", "C"].as_slice());
        substitution_tester(
            vars_a,
            parse_quote! {
                AND (
                    A = b*B + c*C,
                    b = c,
                )
            },
            vars_nob,
            parse_quote! { A = c*B + c*C },
        )
        .unwrap();

        let vars_cd = (
            [
                "c", "d", "r", "s", "c0", "d0", "r0", "s0", "c1", "d1", "r1", "s1",
            ]
            .as_slice(),
            ["A", "B", "C", "D"].as_slice(),
        );
        let vars_cd_noc01 = (
            ["c", "d", "r", "s", "d0", "r0", "s0", "d1", "r1", "s1"].as_slice(),
            ["A", "B", "C", "D"].as_slice(),
        );
        substitution_tester(
            vars_cd,
            parse_quote! {
                AND (
                    C = c*B + r*A,
                    D = d*B + s*A,
                    OR (
                        AND (
                            C = c0*B + r0*A,
                            D = d0*B + s0*A,
                            c0 = d0,
                        ),
                        AND (
                            C = c1*B + r1*A,
                            D = d1*B + s1*A,
                            c1 = d1 + 1,
                        ),
                     )
                )
            },
            vars_cd_noc01,
            parse_quote! {
                AND (
                    C = c*B + r*A,
                    D = d*B + s*A,
                    OR (
                        AND (
                            C = d0*B + r0*A,
                            D = d0*B + s0*A,
                        ),
                        AND (
                            C = (d1+1)*B + r1*A,
                            D = d1*B + s1*A,
                        ),
                     )
                )
            },
        )
        .unwrap();
    }
}

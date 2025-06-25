//! A module for operations that transform a [`StatementTree`].
//! Every transformation must maintain the [disjunction invariant].
//!
//! [disjunction invariant]: StatementTree::check_disjunction_invariant

use super::codegen::CodeGen;
use super::sigma::combiners::*;
use super::sigma::types::expr_type_tokens;
use super::syntax::taggedvardict_to_vardict;
use super::{TaggedIdent, TaggedScalar, TaggedVarDict};
use quote::quote;
use std::collections::{HashSet, VecDeque};
use syn::visit::Visit;
use syn::visit_mut::{self, VisitMut};
use syn::{parse_quote, Error, Expr, Ident, Result};

/// Produce a [`StatementTree`] that represents the constant `true`
fn leaf_true() -> StatementTree {
    StatementTree::Leaf(parse_quote! { true })
}

/// Test if the given [`StatementTree`] represents the constant `true`
fn is_leaf_true(st: &StatementTree) -> bool {
    if let StatementTree::Leaf(Expr::Lit(exprlit)) = st {
        if let syn::Lit::Bool(syn::LitBool { value: true, .. }) = exprlit.lit {
            return true;
        }
    }
    false
}

/// Simplify a [`StatementTree`] by pruning leaves that are the constant
/// `true`, and simplifying `And`, `Or`, and `Thresh` combiners that
/// have fewer than two children.
pub fn prune_statement_tree(st: &mut StatementTree) {
    match st {
        // If the StatementTree is just a Leaf, just keep it unmodified,
        // even if it is leaf_true.
        StatementTree::Leaf(_) => {}

        // For the And combiner, recursively simplify each child, and then
        // prune the child if it is leaf_true.  If we end up with 1
        // child replace ourselves with that child.  If we end up with 0
        // children, replace ourselves with leaf_true.
        StatementTree::And(v) => {
            let mut i: usize = 0;
            // Note that v.len _can change_ during this loop
            while i < v.len() {
                prune_statement_tree(&mut v[i]);
                if is_leaf_true(&v[i]) {
                    // Remove this child, and _do not_ increment i
                    v.remove(i);
                } else {
                    i += 1;
                }
            }
            if v.is_empty() {
                *st = leaf_true();
            } else if v.len() == 1 {
                let child = v.remove(0);
                *st = child;
            }
        }

        // For the Or combiner, recursively simplify each child, and if
        // it ends up leaf_true, replace ourselves with leaf_true.
        // If we end up with 1 child, we must have started wth 1 child.
        // Replace ourselves with that child anyway.
        StatementTree::Or(v) => {
            let mut i: usize = 0;
            // Note that v.len _can change_ during this loop
            while i < v.len() {
                prune_statement_tree(&mut v[i]);
                if is_leaf_true(&v[i]) {
                    *st = leaf_true();
                    return;
                } else {
                    i += 1;
                }
            }
            if v.len() == 1 {
                let child = v.remove(0);
                *st = child;
            }
        }

        // For the Thresh combiner, recursively simplify each child, and
        // if it ends up leaf_true, prune it, and subtract 1 from the
        // thresh.  If the thresh hits 0, replace ourselves with
        // leaf_true.  If we end up with 1 child and thresh is 1,
        // replace ourselves with that child.
        StatementTree::Thresh(thresh, v) => {
            let mut i: usize = 0;
            // Note that v.len _can change_ during this loop
            while i < v.len() {
                prune_statement_tree(&mut v[i]);
                if is_leaf_true(&v[i]) {
                    // Remove this child, and _do not_ increment i
                    v.remove(i);
                    // But decrement thresh
                    *thresh -= 1;
                    if *thresh == 0 {
                        *st = leaf_true();
                        return;
                    }
                } else {
                    i += 1;
                }
            }
            if v.len() == 1 {
                // If thresh == 0, we would have exited above
                assert!(*thresh == 1);
                let child = v.remove(0);
                *st = child;
            }
        }
    }
}

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

/// Add parentheses around an [`Expr`] (which represents an [arithmetic
/// expression]) if needed.
///
/// The parentheses are needed if the [`Expr`] would parse as multiple
/// tokens.  For example, `a+b` turns into `(a+b)`, but `c`
/// remains `c` and `(a+b)` remains `(a+b)`.
///
/// [arithmetic expression]: super::sigma::types::expr_type
pub fn paren_if_needed(expr: Expr) -> Expr {
    match expr {
        Expr::Unary(_) | Expr::Binary(_) => parse_quote! { (#expr) },
        _ => expr,
    }
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

/// Look for, and apply, all of the _substitutions_ specified in leaves
/// of the [`StatementTree`].
///
/// A _substitution_ is a statement of the form `a = b` or `b = 2*(c + 1)`.
/// That is, it is a single variable name (which must be a private
/// `Scalar`, as specified in the provided [`TaggedVarDict`]), an equal
/// sign, and an [arithmetic expression] involving other `Scalar`
/// variables, constants, parens, and the operators `+`, `-`, and `*`.
///
/// Applying a substitution means replacing the variable to the left of
/// the `=` with the expression on the right of the `=` everywhere it
/// appears in the [`StatementTree`].  Any given variable may only be
/// substituted once in a [`StatementTree`].
///
/// The expression on the right must not contain the variable on the
/// left, either directly or after other substitutions.  For example,
/// `a = a + b` is not allowed, nor is the combination of substitutions
/// `a = b + 1, b = c + 2, c = 2*a`.
///
/// After a substitution is applied, the substituted variable will no
/// longer appear anywhere in the [`StatementTree`], and will be removed
/// from the [`TaggedVarDict`].  The leaves of the [`StatementTree`]
/// containing the substitution statements themselves will be turned
/// into the constant `true` and then pruned using
/// [`prune_statement_tree`].  The [`CodeGen`] will be used to generate
/// tests in the generated `prove` function that the `Params` and
/// `Witness` supplied to it do in fact satisfy the statements being
/// substituted.
///
/// It is the case that if the [disjunction invariant] is satisfied
/// before this function is called (and the caller must ensure that it
/// is), then it will be satisfied after the substitutions are applied,
/// and then also after the [`StatementTree`] is pruned.
///
/// [arithmetic expression]: super::sigma::types::expr_type
/// [disjunction invariant]: StatementTree::check_disjunction_invariant
pub fn apply_substitutions(
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

    // For each leaf expression, see if it looks like a substitution of
    // a private Scalar
    let mut subs: VecDeque<(Ident, Expr, HashSet<String>)> = VecDeque::new();
    let mut subs_vars: HashSet<String> = HashSet::new();
    for leafexpr in leaves.iter_mut() {
        let mut is_subs = None;
        if let Expr::Assign(syn::ExprAssign { left, .. }) = *leafexpr {
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
            let mut expr: Expr = parse_quote! { true };
            std::mem::swap(&mut expr, *leafexpr);
            // This "if let" is guaranteed to succeed
            if let Expr::Assign(syn::ExprAssign { right, .. }) = expr {
                if let Ok((_, right_tokens)) = expr_type_tokens(&vardict, &right) {
                    let used_priv_scalars = priv_scalar_set(&right, vars);
                    if !subs_vars.insert(id.to_string()) {
                        return Err(Error::new(id.span(), "variable substituted multiple times"));
                    }
                    codegen.prove_append(quote! {
                        // It's OK to have a test that observably fails
                        // for illegal inputs (but is constant time for
                        // valid inputs)
                        if #id != #right_tokens {
                            return Err(SigmaError::VerificationFailure);
                        }
                    });
                    let right = paren_if_needed(*right);
                    subs.push_back((id, right, used_priv_scalars));
                }
            }
        }
    }

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
        for leafexpr in leaves.iter_mut() {
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

    #[test]
    fn leaf_true_test() {
        assert!(is_leaf_true(&leaf_true()));
        assert!(!is_leaf_true(&StatementTree::Leaf(parse_quote! { false })));
        assert!(!is_leaf_true(&StatementTree::Leaf(parse_quote! { 1 })));
        assert!(!is_leaf_true(
            &StatementTree::parse(&parse_quote! {
                OR(1=1, a=b)
            })
            .unwrap()
        ));
    }

    fn prune_tester(e: Expr, pruned_e: Expr) {
        let mut st = StatementTree::parse(&e).unwrap();
        prune_statement_tree(&mut st);
        assert_eq!(st, StatementTree::parse(&pruned_e).unwrap());
    }

    #[test]
    fn prune_statement_tree_test() {
        prune_tester(
            parse_quote! {
                AND (
                    true,
                    e = f,
                )
            },
            parse_quote! {
                e = f
            },
        );
        prune_tester(
            parse_quote! {
                AND (
                    e = f,
                    true,
                )
            },
            parse_quote! {
                e = f
            },
        );
        prune_tester(
            parse_quote! {
                AND (
                    e = f,
                    true,
                    b = c,
                )
            },
            parse_quote! {
                AND (
                    e = f,
                    b = c,
                )
            },
        );
        prune_tester(
            parse_quote! {
                OR (
                    true,
                    e = f,
                )
            },
            parse_quote! {
                true
            },
        );
        prune_tester(
            parse_quote! {
                AND (
                    a = b,
                    true,
                    OR (
                        c = d,
                        true,
                        e = f
                    )
                )
            },
            parse_quote! {
                a = b
            },
        );
        prune_tester(
            parse_quote! {
                THRESH (3,
                    a = b,
                    true,
                    THRESH (1,
                        c = d,
                        true,
                        e = f
                    )
                )
            },
            parse_quote! {
                a = b
            },
        );
        prune_tester(
            parse_quote! {
                THRESH (3,
                    a = b,
                    true,
                    THRESH (2,
                        c = d,
                        true,
                        e = f
                    )
                )
            },
            parse_quote! {
                THRESH (2,
                    a = b,
                    THRESH (1,
                        c = d,
                        e = f
                    )
                )
            },
        );
    }

    fn substitution_tester(
        vars: (&[&str], &[&str]),
        e: Expr,
        subbed_vars: (&[&str], &[&str]),
        subbed_e: Expr,
    ) -> Result<()> {
        let mut taggedvardict = taggedvardict_from_strs(vars);
        let mut st = StatementTree::parse(&e).unwrap();
        let mut codegen = CodeGen::new_empty();
        apply_substitutions(&mut codegen, &mut st, &mut taggedvardict)?;
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

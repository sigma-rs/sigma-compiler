//! A module for operations that transform a [`StatementTree`].
//! Every transformation must maintain the [disjunction invariant].
//!
//! [disjunction invariant]: StatementTree::check_disjunction_invariant

use super::sigma::combiners::*;
use syn::{parse_quote, Expr};

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

#[cfg(test)]
mod tests {
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
}

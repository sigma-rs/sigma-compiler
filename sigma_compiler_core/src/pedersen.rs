//! A module for finding and manipulating Pedersen commitments in a
//! [`StatementTree`].
//!
//! A Pedersen commitment to a private `Scalar` `x` looks like
//!
//! `C = (a*x+b)*A + r*B`
//!
//! Where `a` is a constant non-zero `Scalar` (often
//! [`Scalar::ONE`](https://docs.rs/ff/0.13.1/ff/trait.Field.html#associatedconstant.ONE)),
//! `b` is a public `Scalar` or constant (or combinations of those),
//! `r` is a random private `Scalar` that appears nowhere else in the
//! [`StatementTree`], `C` is a public `Point`, and `A` and `B` are
//! computationally independent public `Point`s.

use super::sigma::combiners::*;
use super::sigma::types::*;
use super::syntax::*;
use std::collections::{HashMap, HashSet};
use syn::visit::Visit;

/// Find all random private `Scalar`s (according to the
/// [`TaggedVarDict`]) that appear exactly once in the
/// [`StatementTree`].
pub fn unique_random_scalars(vars: &TaggedVarDict, st: &StatementTree) -> HashSet<String> {
    // Filter the TaggedVarDict so that it only contains the private
    // _random_ Scalars
    let random_private_scalars: VarDict = vars
        .iter()
        .filter(|(_, v)| {
            matches!(
                v,
                TaggedIdent::Scalar(TaggedScalar {
                    is_pub: false,
                    is_rand: true,
                    ..
                })
            )
        })
        .map(|(k, v)| (k.clone(), AExprType::from(v)))
        .collect();

    let mut seen_randoms: HashMap<String, usize> = HashMap::new();

    // Create a PrivScalarMap that will call the given closure for each
    // private Scalar (listed in the VarDict) in a supplied expression
    let mut var_map = PrivScalarMap {
        vars: &random_private_scalars,
        // The closure counts how many times each private random Scalar
        // in the VarDict appears in total
        closure: &mut |ident| {
            let id_str = ident.to_string();
            let val = seen_randoms.get(&id_str);
            let newval = match val {
                Some(n) => n + 1,
                None => 1,
            };
            seen_randoms.insert(id_str, newval);
            Ok(())
        },
        result: Ok(()),
    };
    // Call the PrivScalarMap for each leaf expression in the
    // StatementTree
    for e in st.leaves() {
        var_map.visit_expr(e);
    }
    // Return a HashSet of the ones that we saw exactly once
    seen_randoms
        .into_iter()
        .filter_map(|(k, v)| if v == 1 { Some(k) } else { None })
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use syn::{parse_quote, Expr};

    fn unique_random_scalars_tester(vars: (&[&str], &[&str]), e: Expr, expected: &[&str]) {
        let taggedvardict = taggedvardict_from_strs(vars);
        let st = StatementTree::parse(&e).unwrap();
        let expected_out = expected.iter().map(|s| s.to_string()).collect();
        let output = unique_random_scalars(&taggedvardict, &st);
        assert_eq!(output, expected_out);
    }

    #[test]
    fn unique_random_scalars_test() {
        let vars = (
            ["x", "y", "z", "rand r", "rand s", "rand t"].as_slice(),
            ["C", "cind A", "cind B"].as_slice(),
        );

        unique_random_scalars_tester(
            vars,
            parse_quote! {
                C = x*A + r*B
            },
            ["r"].as_slice(),
        );

        unique_random_scalars_tester(
            vars,
            parse_quote! {
                AND (
                    C = x*A + r*B,
                    D = y*A + s*B,
                )
            },
            ["r", "s"].as_slice(),
        );

        unique_random_scalars_tester(
            vars,
            parse_quote! {
                AND (
                    C = x*A + r*B,
                    OR (
                        D = y*A + s*B,
                        E = y*A + t*B,
                    ),
                    E = z*A + r*B,
                )
            },
            ["s", "t"].as_slice(),
        );
    }
}

//! A module to transform not-equals statements about a private
//! `Scalar` into statements about linear combinations of `Point`s.
//!
//! A non-equals statement looks like `x - 8 != a`, where `x` is a
//! private `Scalar`, `x-8` optionally offsets that private `Scalar` by
//! a public `Scalar` or constant, and `a` is a public `Scalar` or
//! constant (or an [arithmetic expression] that evaluates to a public
//! `Scalar`).
//!
//! [arithmetic expression]: super::sigma::types::expr_type

use super::codegen::CodeGen;
use super::pedersen::{
    convert_commitment, convert_randomness, random_scalars, recognize_linscalar,
    recognize_pedersen_assignment, recognize_pubscalar, LinScalar, PedersenAssignment,
};
use super::sigma::combiners::*;
use super::sigma::types::{expr_type_tokens, VarDict};
use super::syntax::{collect_cind_points, taggedvardict_to_vardict};
use super::transform::paren_if_needed;
use super::TaggedVarDict;
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::{parse_quote, Error, Expr, Ident, Result};

/// Subtract the Expr `subexpr` (with constant value `subval`, if
/// present) from the `LinScalar` `linscalar`.  Return the resulting
/// `LinScalar`.
fn subtract_expr(linscalar: LinScalar, subexpr: &Expr, subval: Option<i128>) -> LinScalar {
    if subval != Some(0) {
        let paren_sub = paren_if_needed(subexpr.clone());
        if let Some(expr) = linscalar.pub_scalar_expr {
            return LinScalar {
                pub_scalar_expr: Some(parse_quote! {
                    #expr - #paren_sub
                }),
                ..linscalar
            };
        } else {
            return LinScalar {
                pub_scalar_expr: Some(parse_quote! {
                    -#paren_sub
                }),
                ..linscalar
            };
        }
    }
    linscalar
}

/// Try to parse the given `Expr` as a not-equals statement.  The
/// resulting `LinScalar` is the left side minus the right side.
fn parse(vars: &TaggedVarDict, vardict: &VarDict, expr: &Expr) -> Option<LinScalar> {
    let Expr::Binary(syn::ExprBinary {
        left,
        op: syn::BinOp::Ne(_),
        right,
        ..
    }) = expr
    else {
        return None;
    };
    let linscalar = recognize_linscalar(vars, vardict, left)?;
    let (subexpr_is_vec, subval) = recognize_pubscalar(vars, vardict, right)?;
    // We don't support vector variables
    if linscalar.is_vec || subexpr_is_vec {
        return None;
    }
    Some(subtract_expr(linscalar, right, subval))
}

/// Look for, and transform, not-equals statements specified in the
/// [`StatementTree`] into basic statements about linear combinations of
/// `Point`s.
#[allow(non_snake_case)] // so that Points can be capital letters
pub fn transform(
    codegen: &mut CodeGen,
    st: &mut StatementTree,
    vars: &mut TaggedVarDict,
) -> Result<()> {
    // Make the VarDict version of the variable dictionary
    let mut vardict = taggedvardict_to_vardict(vars);

    // A HashSet of the random Scalars in the macro input
    let mut randoms = random_scalars(vars, st);

    // Gather mutable references to all of the leaves of the
    // StatementTree.  Note that this ignores the combiner structure in
    // the StatementTree, but that's fine.
    let mut leaves = st.leaves_st_mut();

    // A list of the computationally independent (non-vector) Points in
    // the macro input.  There must be at least two of them in order to
    // handle not-equals statements, so that we can make Pedersen
    // commitments.
    let cind_points = collect_cind_points(vars);

    // Find any statements that look like Pedersen commitments in the
    // StatementTree, and make a HashMap mapping the committed private
    // variable to the parsed commitment.
    let pedersens: HashMap<Ident, PedersenAssignment> = leaves
        .iter()
        .filter_map(|leaf| {
            // See if we recognize this leaf expression as a
            // PedersenAssignment, and if so, make a pair mapping its
            // variable to the PedersenAssignment.  (The "collect()"
            // will turn the list of pairs into a HashMap.)
            if let StatementTree::Leaf(leafexpr) = leaf {
                recognize_pedersen_assignment(vars, &randoms, &vardict, leafexpr)
                    .map(|ped_assign| (ped_assign.var(), ped_assign))
            } else {
                None
            }
        })
        .collect();

    // Count how many not-equals statements we've seen
    let mut neq_stmt_index = 0usize;

    // The generated variable name for the rng
    let rng_var = codegen.gen_ident(&format_ident!("rng"));

    for leaf in leaves.iter_mut() {
        // For each leaf expression, see if it looks like a not-equals statement
        let StatementTree::Leaf(leafexpr) = leaf else {
            continue;
        };
        let Some(neq_linscalar) = parse(vars, &vardict, leafexpr) else {
            continue;
        };
        neq_stmt_index += 1;

        // The variable in the not-equals statement must not be tagged "rand"
        if let Some(super::TaggedIdent::Scalar(super::TaggedScalar {
            is_pub: false,
            is_rand: true,
            ..
        })) = vars.get(&neq_linscalar.id.to_string())
        {
            return Err(Error::new(
                leafexpr.span(),
                "target of not-equals expression cannot be rand",
            ));
        }

        // We will transform the not-equals statement into a list of
        // basic linear combination statements that will be ANDed
        // together to replace the not-equals statement in the
        // StatementTree.  This vector holds the list of basic
        // statements.
        let mut basic_statements: Vec<Expr> = Vec::new();

        // We'll need a Pedersen commitment to the variable in the
        // not-equals statement.  See if there already is one.
        let neq_id = &neq_linscalar.id;
        let ped_assign = if let Some(ped_assign) = pedersens.get(neq_id) {
            ped_assign.clone()
        } else {
            // We'll need to create a new one.  First find two
            // computationally independent Points.
            if cind_points.len() < 2 {
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    "At least two cind Points must be declared to support not-equals statements",
                ));
            }
            let cind_A = &cind_points[0];
            let cind_B = &cind_points[1];

            // Create new variables for the Pedersen commitment and its
            // random Scalar.
            let commitment_var = codegen.gen_point(
                vars,
                &format_ident!("neq{}_{}_genC", neq_stmt_index, neq_id),
                false, // is_vec
                true,  // send_to_verifier
            );
            let rand_var = codegen.gen_scalar(
                vars,
                &format_ident!("neq{}_{}_genr", neq_stmt_index, neq_id),
                true,  // is_rand
                false, // is_vec
            );

            // Update vardict and randoms with the new vars
            vardict = taggedvardict_to_vardict(vars);
            randoms.insert(rand_var.to_string());

            let ped_assign_expr: Expr = parse_quote! {
                #commitment_var = #neq_id * #cind_A + #rand_var * #cind_B
            };
            let ped_assign =
                recognize_pedersen_assignment(vars, &randoms, &vardict, &ped_assign_expr).unwrap();

            codegen.prove_append(quote! {
                let #rand_var = Scalar::random(#rng_var);
                let #ped_assign_expr;
            });

            basic_statements.push(ped_assign_expr);

            ped_assign
        };

        // At this point, we have a Pedersen commitment for some linear
        // function of neq_id (given by
        // ped_assign.pedersen.var_term.coeff), using some linear
        // function of rand_var (given by
        // ped_assign.pedersen.rand_term.coeff) as the randomness.  But
        // what we need is a Pedersen commitment for a possibly
        // different linear function of neq_id (given by
        // neq_linscalar).  So we output runtime code for both the
        // prover and the verifier that converts the commitment, and
        // code for just the prover that converts the randomness.

        // Make a new runtime variable to hold the converted commitment
        let commitment_var = codegen.gen_point(
            vars,
            &format_ident!("neq{}_{}_C", neq_stmt_index, neq_id),
            false, // is_vec
            false, // send_to_verifier
        );
        let rand_var = codegen.gen_ident(&format_ident!("neq{}_{}_r", neq_stmt_index, neq_id));

        // Update vardict and randoms with the new vars
        vardict = taggedvardict_to_vardict(vars);
        randoms.insert(rand_var.to_string());

        codegen.prove_verify_append(convert_commitment(
            &commitment_var,
            &ped_assign,
            &neq_linscalar,
            &vardict,
        )?);
        codegen.prove_append(convert_randomness(
            &rand_var,
            &ped_assign,
            &neq_linscalar,
            &vardict,
        )?);

        // Now commitment_var is a Pedersen commitment to the LinScalar
        // we want to prove is not 0, using the randomness rand_var.
        // That is, commitment_var = L(x)*A + rand_var*B, where L(x) is
        // a linear function of x, and we want to show that L(x) != 0.
        // So we compute j = L(x).invert(), and s = -rand_var*j as new
        // private Scalars, and show that A = j*commitment_var + s*B.
        let Lx_var = codegen.gen_ident(&format_ident!("neq{}_{}_var", neq_stmt_index, neq_id));
        let Lx_code = expr_type_tokens(&vardict, &neq_linscalar.to_expr())?.1;
        let j_var = codegen.gen_scalar(
            vars,
            &format_ident!("neq{}_{}_j", neq_stmt_index, neq_id),
            false, // is_rand
            false, // is_vec
        );
        let s_var = codegen.gen_scalar(
            vars,
            &format_ident!("neq{}_{}_s", neq_stmt_index, neq_id),
            false, // is_rand
            false, // is_vec
        );

        // Update vardict with the new vars
        vardict = taggedvardict_to_vardict(vars);

        // The generators used in the Pedersen commitment
        let commit_generator = &ped_assign.pedersen.var_term.id;
        let rand_generator = &ped_assign.pedersen.rand_term.id;

        // The prover code
        codegen.prove_append(quote! {
            let #Lx_var = #Lx_code;
            let #j_var = <Scalar as Field>::invert(&#Lx_var)
                .into_option()
                .ok_or(SigmaError::VerificationFailure)?;
            let #s_var = -#rand_var * #j_var;
        });

        basic_statements.push(parse_quote! {
            #commit_generator = #j_var * #commitment_var
                + #s_var * #rand_generator
        });

        // Now replace the not-equals statement with an And of the
        // basic_statements
        let neq_st = StatementTree::And(
            basic_statements
                .into_iter()
                .map(StatementTree::Leaf)
                .collect(),
        );

        **leaf = neq_st;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::syntax::taggedvardict_from_strs;
    use super::*;

    fn parse_tester(vars: (&[&str], &[&str]), expr: Expr, expect: Option<LinScalar>) {
        let taggedvardict = taggedvardict_from_strs(vars);
        let vardict = taggedvardict_to_vardict(&taggedvardict);
        let output = parse(&taggedvardict, &vardict, &expr);
        assert_eq!(output, expect);
    }

    #[test]
    fn parse_test() {
        let vars = (
            [
                "x", "y", "z", "pub a", "pub b", "pub c", "rand r", "rand s", "rand t",
            ]
            .as_slice(),
            ["C", "cind A", "cind B"].as_slice(),
        );

        parse_tester(
            vars,
            parse_quote! {
                x != 0
            },
            Some(LinScalar {
                coeff: 1,
                pub_scalar_expr: None,
                id: parse_quote! {x},
                is_vec: false,
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                x != 5
            },
            Some(LinScalar {
                coeff: 1,
                pub_scalar_expr: Some(parse_quote! {-5}),
                id: parse_quote! {x},
                is_vec: false,
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                2*x != 5
            },
            Some(LinScalar {
                coeff: 2,
                pub_scalar_expr: Some(parse_quote! {-5}),
                id: parse_quote! {x},
                is_vec: false,
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                2*x + 12 != 5
            },
            Some(LinScalar {
                coeff: 2,
                pub_scalar_expr: Some(parse_quote! {12i128-5}),
                id: parse_quote! {x},
                is_vec: false,
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                2*x + a*a != 0
            },
            Some(LinScalar {
                coeff: 2,
                pub_scalar_expr: Some(parse_quote! {a*a}),
                id: parse_quote! {x},
                is_vec: false,
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                2*x + a*a != b*c + c
            },
            Some(LinScalar {
                coeff: 2,
                pub_scalar_expr: Some(parse_quote! {a*a-(b*c+c)}),
                id: parse_quote! {x},
                is_vec: false,
            }),
        );
    }
}

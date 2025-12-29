//! A module to transform range statements about `Scalar`s into
//! statements about linear combinations of `Point`s.
//!
//! A range statement looks like `(a..b).contains(x-8)`, where `a` and
//! `b` are expressions involving only _public_ `Scalar`s and constants
//! and `x-8` is a private `Scalar`, possibly offset by a public
//! `Scalar` or constant.  At this time, none of the variables can be
//! vector variables.
//!
//! As usual for Rust notation, the range `a..b` includes `a` but
//! _excludes_ `b`.  You can also write `a..=b` to include both
//! endpoints.  It is allowed for the range to "wrap around" 0, so
//! that `L-50..100` is a valid range, and equivalent to `-50..100`,
//! where `L` is the order of the group you are using.
//!
//! The size of the range (`b-a`) will be known at run time, but not
//! necessarily at compile time.  The size must fit in an [`i128`] and
//! must be strictly greater than 1.  Note that the range (and its size)
//! are public, but the value you are stating is in the range will be
//! private.

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

/// A struct representing a normalized parsed range statement.
///
/// Here, "normalized" means that the range is adjusted so that the
/// lower bound is 0.  This is accomplished by subtracting the stated
/// lower bound from both the upper bound and the expression that is
/// being asserting that it is in the range.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RangeStatement {
    /// The upper bound of the range (exclusive).  This must evaluate to
    /// a public Scalar.
    upper: Expr,
    /// The expression that is being asserted that it is in the range.
    /// This must be a [`LinScalar`]
    linscalar: LinScalar,
}

/// Subtract the Expr `lower` (with constant value `lowerval`, if
/// present) from the Expr `expr` (with constant value `exprval`, if
/// present).  Return the resulting expression, as well as its constant
/// value, if there is one.  Do the subtraction numerically if possible,
/// but otherwise symbolically.
fn subtract_expr(
    expr: Option<&Expr>,
    exprval: Option<i128>,
    lower: &Expr,
    lowerval: Option<i128>,
) -> (Expr, Option<i128>) {
    // Note that if expr is None, then exprval is Some(0)
    if let (Some(ev), Some(lv)) = (exprval, lowerval) {
        if let Some(diffv) = ev.checked_sub(lv) {
            // We can do the subtraction numerically
            return (parse_quote! { #diffv }, Some(diffv));
        }
    }
    let paren_lower = paren_if_needed(lower.clone());
    // Return the difference symbolically
    (
        if let Some(e) = expr {
            parse_quote! { #e - #paren_lower }
        } else {
            parse_quote! { -#paren_lower }
        },
        None,
    )
}

/// Try to parse the given `Expr` as a range statement
fn parse(vars: &TaggedVarDict, vardict: &VarDict, expr: &Expr) -> Option<RangeStatement> {
    // The expression needs to be of the form
    // (lower..upper).contains(expr)
    // The "top level" must be the method call ".contains"
    if let Expr::MethodCall(syn::ExprMethodCall {
        receiver,
        method,
        turbofish: None,
        args,
        ..
    }) = expr
    {
        if &method.to_string() != "contains" {
            // Wasn't ".contains"
            return None;
        }
        // Remove parens around the range, if present
        let mut range_expr = receiver.as_ref();
        if let Expr::Paren(syn::ExprParen {
            expr: parened_expr, ..
        }) = range_expr
        {
            range_expr = parened_expr;
        }
        // Parse the range
        if let Expr::Range(syn::ExprRange {
            start, limits, end, ..
        }) = range_expr
        {
            // The endpoints of the range need to be non-vector public
            // Scalar expressions
            // The first as_ref() turns &Option<Box<Expr>> into
            // Option<&Box<Expr>>.  The ? removes the Option, and the
            // second as_ref() turns &Box<Expr> into &Expr.
            let lower = start.as_ref()?.as_ref().clone();
            let mut upper = end.as_ref()?.as_ref().clone();
            let Some((false, lowerval)) = recognize_pubscalar(vars, vardict, &lower) else {
                return None;
            };
            let Some((false, mut upperval)) = recognize_pubscalar(vars, vardict, &upper) else {
                return None;
            };
            let inclusive_upper = matches!(limits, syn::RangeLimits::Closed(_));
            // There needs to be exactly one argument of .contains()
            if args.len() != 1 {
                return None;
            }
            // The private expression needs to be a LinScalar
            let priv_expr = args.first().unwrap();
            let mut linscalar = recognize_linscalar(vars, vardict, priv_expr)?;
            // It is.  See if the pub_scalar_expr in the LinScalar has a
            // constant value
            let linscalar_pubscalar_val = if let Some(ref pse) = linscalar.pub_scalar_expr {
                let Some((false, pubscalar_val)) = recognize_pubscalar(vars, vardict, pse) else {
                    return None;
                };
                pubscalar_val
            } else {
                Some(0)
            };

            // We have a valid range statement.  Normalize it by forcing
            // the upper bound to be exclusive, and the lower bound to
            // be 0.

            // If the range was inclusive of the upper bound (e.g.,
            // `0..=100`), add 1 to the upper bound to make it exclusive
            // (e.g., `0..101`).
            if inclusive_upper {
                // Add 1 to the upper bound, numerically if possible,
                // but otherwise symbolically
                let mut added_numerically = false;
                if let Some(uv) = upperval {
                    if let Some(new_uv) = uv.checked_add(1) {
                        upper = parse_quote! { #new_uv };
                        upperval = Some(new_uv);
                        added_numerically = true;
                    }
                }
                if !added_numerically {
                    upper = parse_quote! { #upper + 1 };
                    upperval = None;
                }
            }

            // If the lower bound is not 0, subtract it from both the
            // upper bound and the pubscalar_expr in the LinScalar.  Do
            // this numericaly if possibly, but otherwise symbolically.
            if lowerval != Some(0) {
                (upper, _) = subtract_expr(Some(&upper), upperval, &lower, lowerval);
                let pubscalar_expr;
                (pubscalar_expr, _) = subtract_expr(
                    linscalar.pub_scalar_expr.as_ref(),
                    linscalar_pubscalar_val,
                    &lower,
                    lowerval,
                );
                linscalar.pub_scalar_expr = Some(pubscalar_expr);
            }

            return Some(RangeStatement { upper, linscalar });
        }
    }
    None
}

/// Look for, and transform, range statements specified in the
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
    // handle range statements, so that we can make Pedersen
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

    // Count how many range statements we've seen
    let mut range_stmt_index = 0usize;

    // The generated variable name for the rng
    let rng_var = codegen.gen_ident(&format_ident!("rng"));

    for leaf in leaves.iter_mut() {
        // For each leaf expression, see if it looks like a range statement
        let StatementTree::Leaf(leafexpr) = leaf else {
            continue;
        };
        let Some(range_stmt) = parse(vars, &vardict, leafexpr) else {
            continue;
        };
        range_stmt_index += 1;

        // The variable in the range statement must not be tagged "rand"
        if let Some(super::TaggedIdent::Scalar(super::TaggedScalar {
            is_pub: false,
            is_rand: true,
            ..
        })) = vars.get(&range_stmt.linscalar.id.to_string())
        {
            return Err(Error::new(
                leafexpr.span(),
                "target of range expression cannot be rand",
            ));
        }

        // We will transform the range statement into a list of basic
        // linear combination statements that will be ANDed together to
        // replace the range statement in the StatementTree.  This
        // vector holds the list of basic statements.
        let mut basic_statements: Vec<Expr> = Vec::new();

        // We'll need a Pedersen commitment to the variable in the range
        // statement.  See if there already is one.
        let range_id = &range_stmt.linscalar.id;
        let ped_assign = if let Some(ped_assign) = pedersens.get(range_id) {
            ped_assign.clone()
        } else {
            // We'll need to create a new one.  First find two
            // computationally independent Points.
            if cind_points.len() < 2 {
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    "At least two cind Points must be declared to support range statements",
                ));
            }
            let cind_A = &cind_points[0];
            let cind_B = &cind_points[1];

            // Create new variables for the Pedersen commitment and its
            // random Scalar.
            let commitment_var = codegen.gen_point(
                vars,
                &format_ident!("range{}_{}_genC", range_stmt_index, range_id),
                false, // is_vec
                true,  // send_to_verifier
            );
            let rand_var = codegen.gen_scalar(
                vars,
                &format_ident!("range{}_{}_genr", range_stmt_index, range_id),
                true,  // is_rand
                false, // is_vec
            );

            // Update vardict and randoms with the new vars
            vardict = taggedvardict_to_vardict(vars);
            randoms.insert(rand_var.to_string());

            let ped_assign_expr: Expr = parse_quote! {
                #commitment_var = #range_id * #cind_A + #rand_var * #cind_B
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
        // function of range_id (given by
        // ped_assign.pedersen.var_term.coeff), using some linear
        // function of rand_var (given by
        // ped_assign.pedersen.rand_term.coeff) as the randomness.  But
        // what we need is a Pedersen commitment for a possibly
        // different linear function of range_id (given by
        // range_stmt.linscalar).  So we output runtime code for the
        // verifier that converts the commitment, and code for the
        // prover that converts the randomness.

        // Make a new runtime variable to hold the converted commitment
        let commitment_var =
            codegen.gen_ident(&format_ident!("range{}_{}_C", range_stmt_index, range_id));
        let rand_var =
            codegen.gen_ident(&format_ident!("range{}_{}_r", range_stmt_index, range_id));

        // Update vardict and randoms with the new vars
        vardict = taggedvardict_to_vardict(vars);
        randoms.insert(rand_var.to_string());

        codegen.verify_append(convert_commitment(
            &commitment_var,
            &ped_assign,
            &range_stmt.linscalar,
            &vardict,
        )?);
        codegen.prove_append(convert_randomness(
            &rand_var,
            &ped_assign,
            &range_stmt.linscalar,
            &vardict,
        )?);

        // Have both the prover and verifier compute the upper bound of
        // the range, and generate the bitrep_scalar vector based on
        // that upper bound.  The key to the range proof is that this
        // bitrep_scalar vector has the property that you can write a
        // Scalar x as a sum of (different) elements of this vector if
        // and only if 0 <= x < upper.  The prover and verifier both
        // know this vector (it depends only on upper, which is public).
        // Then the prover will generate private bits that indicate
        // which elements of the vector add up to x, and output
        // commitments to those bits, along with proofs that each of
        // those commitments indeed commits to a bit (0 or 1).  The
        // verifier will check that the linear combination of the
        // commitments to those bits with the elements of the
        // bitrep_scalar vector yields the known commitment to x.
        //
        // As a small optimization, the commitment to the first bit
        // (which always has a bitrep_scalar entry of 1) is not actually
        // sent; instead of the verifier checking that the linear
        // combination of the commitments equals the known commitment to
        // x, it _computes_ the missing commitment to the first bit as
        // the difference between the known commitment to x and the
        // linear combination of the remaining commitments.  The prover
        // still needs to prove that the value committed in that
        // computed commitment is a bit, but does not need to send the
        // commitment itself, saving a small bit of communication.

        let upper_var = codegen.gen_ident(&format_ident!(
            "range{}_{}_upper",
            range_stmt_index,
            range_id
        ));
        let upper_code = expr_type_tokens(&vardict, &range_stmt.upper)?.1;
        let bitrep_scalars_var = codegen.gen_ident(&format_ident!(
            "range{}_{}_bitrep_scalars",
            range_stmt_index,
            range_id
        ));
        let nbits_var = codegen.gen_ident(&format_ident!(
            "range{}_{}_nbits",
            range_stmt_index,
            range_id
        ));

        codegen.prove_verify_pre_instance_append(quote! {
            let #upper_var = #upper_code;
            let #bitrep_scalars_var =
                sigma_compiler::rangeutils::bitrep_scalars_vartime(#upper_var)?;
            if #bitrep_scalars_var.is_empty() {
                // The upper bound was either less than 2, or more than
                // i128::MAX
                return Err(SigmaError::VerificationFailure);
            }
            let #nbits_var = #bitrep_scalars_var.len();
        });

        // The prover will compute the bit representation (which
        // elements of the bitrep_scalars vector add up to x).  This
        // should be done (in the prover code at runtime) in constant
        // time.
        let x_var = codegen.gen_ident(&format_ident!("range{}_{}_var", range_stmt_index, range_id));
        let bitrep_var = codegen.gen_ident(&format_ident!(
            "range{}_{}_bitrep",
            range_stmt_index,
            range_id
        ));
        let x_code = expr_type_tokens(&vardict, &range_stmt.linscalar.to_expr())?.1;
        codegen.prove_append(quote! {
            let #x_var = #x_code;
            let #bitrep_var =
                sigma_compiler::rangeutils::compute_bitrep(#x_var, &#bitrep_scalars_var);
        });

        // As mentioned above, we treat the first bit specially.  Make a
        // vector of commitments to the rest of the bits to send to the
        // verifier, and also a vector of the committed bits and a
        // vector of randomnesses for the commitments, both for the
        // witness, again not putting those for the first bit into the
        // vectors.  Do make separate witness elements for the committed
        // first bit and the randomness for it.
        let bitcomm_var = codegen.gen_point(
            vars,
            &format_ident!("range{}_{}_bitC", range_stmt_index, range_id),
            true, // is_vec
            true, // send_to_verifier
        );
        let bits_var = codegen.gen_scalar(
            vars,
            &format_ident!("range{}_{}_bit", range_stmt_index, range_id),
            false, // is_rand
            true,  // is_vec
        );
        let bitrand_var = codegen.gen_scalar(
            vars,
            &format_ident!("range{}_{}_bitrand", range_stmt_index, range_id),
            false, // is_rand is false because this value might get reused in bitrandsq
            true,  // is_vec
        );
        let bitrandsq_var = codegen.gen_scalar(
            vars,
            &format_ident!("range{}_{}_bitrandsq", range_stmt_index, range_id),
            false, // is_rand
            true,  // is_vec
        );
        let firstbitcomm_var = codegen.gen_point(
            vars,
            &format_ident!("range{}_{}_firstbitC", range_stmt_index, range_id),
            false, // is_vec
            false, // send_to_verifier
        );
        let firstbit_var = codegen.gen_scalar(
            vars,
            &format_ident!("range{}_{}_firstbit", range_stmt_index, range_id),
            false, // is_rand
            false, // is_vec
        );
        let firstbitrand_var = codegen.gen_scalar(
            vars,
            &format_ident!("range{}_{}_firstbitrand", range_stmt_index, range_id),
            false, // is_rand
            false, // is_vec
        );
        let firstbitrandsq_var = codegen.gen_scalar(
            vars,
            &format_ident!("range{}_{}_firstbitrandsq", range_stmt_index, range_id),
            false, // is_rand
            false, // is_vec
        );

        // Update vardict and randoms with the new vars
        vardict = taggedvardict_to_vardict(vars);
        randoms.insert(bitrand_var.to_string());
        randoms.insert(firstbitrand_var.to_string());

        // The generators used in the Pedersen commitment
        let commit_generator = &ped_assign.pedersen.var_term.id;
        let rand_generator = &ped_assign.pedersen.rand_term.id;

        codegen.verify_pre_instance_append(quote! {
            let mut #bitcomm_var = Vec::<Point>::new();
            #bitcomm_var.resize(#nbits_var - 1, Point::default());
        });
        // The prover code
        codegen.prove_append(quote! {
            // The main strategy is to prove that each commitment is to
            // a bit (0 or 1), and we do this by showing that the
            // committed value equals its own square.  That is, we show
            // that C = b*A + r*B and also that C = b*C + s*B.  If both
            // of those are true (and A and B are computationally
            // independent), then C = b*(b*A + r*B) + s*B = b^2*A +
            // (r*b+s)*B, so b=b^2 and r=r*b+s.  Therefore either b=0
            // and s=r or b=1 and s=0.

            // Map the bit representation to a vector of Scalar(0) and
            // Scalar(1), but skip the first bit, as described above.
            let #bits_var: Vec<Scalar> =
                #bitrep_var
                    .iter()
                    .skip(1)
                    .map(|b| Scalar::conditional_select(
                        &Scalar::ZERO,
                        &Scalar::ONE,
                        *b,
                    ))
                    .collect();
            // Choose randomizers r for the commitments randomly
            let #bitrand_var: Vec<Scalar> =
                (0..(#nbits_var-1))
                    .map(|_| Scalar::random(#rng_var))
                    .collect();
            // The randomizers s for the commitments to the squares are
            // chosen as above: s=r if b=0 and s=0 if b=1.
            let #bitrandsq_var: Vec<Scalar> =
                (0..(#nbits_var-1))
                    .map(|i| Scalar::conditional_select(
                        &#bitrand_var[i],
                        &Scalar::ZERO,
                        #bitrep_var[i+1],
                    ))
                    .collect();
            // Compute the commitments
            let #bitcomm_var: Vec<Point> =
                (0..(#nbits_var-1))
                    .map(|i| #bits_var[i] * #commit_generator +
                        #bitrand_var[i] * #rand_generator)
                    .collect();
            // The same as above, for for the first bit
            let #firstbit_var =
                Scalar::conditional_select(
                    &Scalar::ZERO,
                    &Scalar::ONE,
                    #bitrep_var[0],
                );
            // Compute the randomness that would be needed in the first
            // bit commitment so that the linear combination of all the
            // bit commitments (with the scalars in bitrep_scalars) adds
            // up to commitment_var.
            let mut #firstbitrand_var = #rand_var;
            for i in 0..(#nbits_var-1) {
                #firstbitrand_var -=
                    #bitrand_var[i] * #bitrep_scalars_var[i+1];
            }
            // And the randomization for the first square is as above
            let #firstbitrandsq_var = Scalar::conditional_select(
                    &#firstbitrand_var,
                    &Scalar::ZERO,
                    #bitrep_var[0],
                );
            // Compute the first bit commitment
            let #firstbitcomm_var =
                #firstbit_var * #commit_generator +
                #firstbitrand_var * #rand_generator;
        });

        // The verifier also needs to compute the first commitment
        codegen.verify_append(quote! {
            let mut #firstbitcomm_var = #commitment_var;
            for i in 0..(#nbits_var-1) {
                #firstbitcomm_var -=
                    #bitcomm_var[i] * #bitrep_scalars_var[i+1];
            }
        });

        basic_statements.push(parse_quote! {
            #bitcomm_var = #bits_var * #commit_generator
                + #bitrand_var * #rand_generator
        });
        basic_statements.push(parse_quote! {
            #bitcomm_var = #bits_var * #bitcomm_var
                + #bitrandsq_var * #rand_generator
        });
        basic_statements.push(parse_quote! {
            #firstbitcomm_var = #firstbit_var * #commit_generator
                + #firstbitrand_var * #rand_generator
        });
        basic_statements.push(parse_quote! {
            #firstbitcomm_var = #firstbit_var * #firstbitcomm_var
                + #firstbitrandsq_var * #rand_generator
        });

        // Now replace the range statement with an And of the
        // basic_statements
        let range_st = StatementTree::And(
            basic_statements
                .into_iter()
                .map(StatementTree::Leaf)
                .collect(),
        );

        **leaf = range_st;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::syntax::taggedvardict_from_strs;
    use super::*;

    fn parse_tester(vars: (&[&str], &[&str]), expr: Expr, expect: Option<RangeStatement>) {
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
                (0..100).contains(x)
            },
            Some(RangeStatement {
                upper: parse_quote! { 100 },
                linscalar: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: None,
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (0..=100).contains(x)
            },
            Some(RangeStatement {
                upper: parse_quote! { 101i128 },
                linscalar: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: None,
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (-12..100).contains(x)
            },
            Some(RangeStatement {
                upper: parse_quote! { 112i128 },
                linscalar: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: Some(parse_quote! { 12i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (-12..(1<<20)).contains(x)
            },
            Some(RangeStatement {
                upper: parse_quote! { 1048588i128 },
                linscalar: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: Some(parse_quote! { 12i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (12..(1<<20)).contains(x+7)
            },
            Some(RangeStatement {
                upper: parse_quote! { 1048564i128 },
                linscalar: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: Some(parse_quote! { -5i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (12..(1<<20)).contains(2*x+7)
            },
            Some(RangeStatement {
                upper: parse_quote! { 1048564i128 },
                linscalar: LinScalar {
                    coeff: 2,
                    pub_scalar_expr: Some(parse_quote! { -5i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (-1..(((1<<126)-1)*2)).contains(x)
            },
            Some(RangeStatement {
                upper: parse_quote! { 170141183460469231731687303715884105727i128 },
                linscalar: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: Some(parse_quote! { 1i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (-2..(((1<<126)-1)*2)).contains(x)
            },
            Some(RangeStatement {
                upper: parse_quote! { (((1<<126)-1)*2)-(-2) },
                linscalar: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: Some(parse_quote! { 2i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );

        parse_tester(
            vars,
            parse_quote! {
                (a*b..b+c*c+7).contains(3*x+c*(a+b+2))
            },
            Some(RangeStatement {
                upper: parse_quote! { b+c*c+7-(a*b) },
                linscalar: LinScalar {
                    coeff: 3,
                    pub_scalar_expr: Some(parse_quote! { c*(a+b+2i128)-(a*b) }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );
    }
}

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
//! necessarily at compile time.  The size must fit in an [`i128`].
//! Note that the range (and its size) are public, but the value you
//! are stating is in the range will be private.

use super::codegen::CodeGen;
use super::pedersen::{recognize_linscalar, recognize_pubscalar, LinScalar};
use super::sigma::combiners::*;
use super::sigma::types::VarDict;
use super::syntax::taggedvardict_to_vardict;
use super::transform::paren_if_needed;
use super::TaggedVarDict;
use syn::{parse_quote, Expr, Result};

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
    expr: LinScalar,
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

            return Some(RangeStatement {
                upper,
                expr: linscalar,
            });
        }
    }
    None
}

/// Look for, and transform, range statements specified in the
/// [`StatementTree`] into basic statements about linear combinations of
/// `Point`s.
pub fn transform(
    codegen: &mut CodeGen,
    st: &mut StatementTree,
    vars: &mut TaggedVarDict,
) -> Result<()> {
    // Make the VarDict version of the variable dictionary
    let vardict = taggedvardict_to_vardict(vars);

    // Gather mutable references to all Exprs in the leaves of the
    // StatementTree.  Note that this ignores the combiner structure in
    // the StatementTree, but that's fine.
    let mut leaves = st.leaves_mut();

    // For each leaf expression, see if it looks like a range statement
    for leafexpr in leaves.iter_mut() {
        let is_range = parse(vars, &vardict, leafexpr);
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
                expr: LinScalar {
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
                expr: LinScalar {
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
                expr: LinScalar {
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
                expr: LinScalar {
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
                expr: LinScalar {
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
                expr: LinScalar {
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
                expr: LinScalar {
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
                expr: LinScalar {
                    coeff: 1,
                    pub_scalar_expr: Some(parse_quote! { 2i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
            }),
        );
    }
}

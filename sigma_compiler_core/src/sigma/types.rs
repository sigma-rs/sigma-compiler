//! At the `sigma` level, each variable can be a private `Scalar`, a
//! public `Scalar`, or a public `Point`, and each variable can be
//! either a vector or not.  Arithmetic expressions of those variables
//! can be of any of those types, and also private `Point`s (vector or
//! not).  This module defines an enum [`AExprType`] for
//! the possible types, as well as a dictionary type that maps
//! [`String`]s (the name of the variable) to [`AExprType`], and a
//! function for determining the type of arithmetic expressions
//! involving such variables.

use proc_macro2::TokenStream;
use quote::quote;
use std::collections::HashMap;
use syn::parse::Result;
use syn::spanned::Spanned;
use syn::{Error, Expr};

/// The possible types of an arithmetic expression over `Scalar`s and
/// `Point`s.  Each expression has type either
/// [`Scalar`](AExprType::Scalar) or [`Point`](AExprType::Point), and
/// can be public (`is_pub == true`) or private (`is_pub == false`), and
/// be either a vector (`is_vec == true`) or not (`is_vec == false`).
/// Note that while an individual variable cannot be a private `Point`,
/// it is common to construct an arithmetic expression of that type, for
/// example by multiplying a private `Scalar` by a public `Point`.
/// In addition, an [`AExprType`] that represents a constant `Scalar`
/// value (that fits in an [`i128`]) will have that constant value in
/// `val`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AExprType {
    Scalar {
        is_pub: bool,
        is_vec: bool,
        val: Option<i128>,
    },
    Point {
        is_pub: bool,
        is_vec: bool,
    },
}

impl From<&str> for AExprType {
    /// A convenience function for creating an [`AExprType`] from a
    /// [`&str`].  Pass one of (or their short forms):
    ///   - `"Scalar"` (`"S"`)
    ///   - `"pub Scalar"` (`"pS"`)
    ///   - `"vec Scalar"` (`"vS"`)
    ///   - `"pub vec Scalar"` (`"pvS"`)
    ///   - `"Point"` (`"P"`)
    ///   - `"pub Point"` (`"pP"`)
    ///   - `"vec Point"` (`"vP"`)
    ///   - `"pub vec Point"` (`"pvP"`)
    fn from(s: &str) -> Self {
        match s {
            "Scalar" | "S" => Self::Scalar {
                is_pub: false,
                is_vec: false,
                val: None,
            },
            "pub Scalar" | "pS" => Self::Scalar {
                is_pub: true,
                is_vec: false,
                val: None,
            },
            "vec Scalar" | "vS" => Self::Scalar {
                is_pub: false,
                is_vec: true,
                val: None,
            },
            "pub vec Scalar" | "pvS" => Self::Scalar {
                is_pub: true,
                is_vec: true,
                val: None,
            },
            "Point" | "P" => Self::Point {
                is_pub: false,
                is_vec: false,
            },
            "vec Point" | "vP" => Self::Point {
                is_pub: false,
                is_vec: true,
            },
            "pub Point" | "pP" => Self::Point {
                is_pub: true,
                is_vec: false,
            },
            "pub vec Point" | "pvP" => Self::Point {
                is_pub: true,
                is_vec: true,
            },
            _ => {
                panic!("Illegal string passed to AExprType::from");
            }
        }
    }
}

/// A dictionary of known variables (given by [`String`]s), mapping each
/// to their [`AExprType`]
pub type VarDict = HashMap<String, AExprType>;

/// Create a [`VarDict`] from a slice of pairs of strings.
///
/// The first element of each pair is the variable name; the second
/// represents the [`AExprType`], as listed in the [`AExprType::from`]
/// function
pub fn vardict_from_strs(strs: &[(&str, &str)]) -> VarDict {
    let c = strs
        .iter()
        .map(|(k, v)| (String::from(*k), AExprType::from(*v)));
    VarDict::from_iter(c)
}

/// Given an [`i128`] value, output a [`TokenStream`] representing a
/// valid Rust expression that evaluates to a `Scalar` having that
/// value.
fn const_i128_tokens(val: i128) -> TokenStream {
    let uval = val.unsigned_abs();
    if val >= 0 {
        quote! { Scalar::from_u128(#uval) }
    } else {
        quote! { Scalar::from_u128(#uval).neg() }
    }
}

/// Given a [`VarDict`] and an [`Expr`] representing an arithmetic
/// expression using the variables in the [`VarDict`], compute the
/// [`AExprType`] of the expression.
///
/// An arithmetic expression can consist of:
///   - variables that are in the [`VarDict`]
///   - integer constants
///   - the operations `*`, `+`, `-` (binary or unary)
///   - the operation `<<` where both operands are expressions with no
///     variables
///   - parens
pub fn expr_type(vars: &VarDict, expr: &Expr) -> Result<AExprType> {
    Ok(expr_type_tokens(vars, expr)?.0)
}

/// Given a [`VarDict`] and an [`Expr`] representing an arithmetic
/// expression using the variables in the [`VarDict`], compute the
/// [`AExprType`] of the expression and also a valid Rust
/// [`TokenStream`] that evaluates the expression.
///
/// An arithmetic expression can consist of:
///   - variables that are in the [`VarDict`]
///   - integer constants
///   - the operations `*`, `+`, `-` (binary or unary)
///   - the operation `<<` where both operands are expressions with no
///     variables
///   - parens
pub fn expr_type_tokens(vars: &VarDict, expr: &Expr) -> Result<(AExprType, TokenStream)> {
    match expr {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(litint),
            ..
        }) => {
            let val = litint.base10_parse::<i128>().ok();
            if let Some(val_i128) = val {
                Ok((
                    AExprType::Scalar {
                        is_pub: true,
                        is_vec: false,
                        val,
                    },
                    const_i128_tokens(val_i128),
                ))
            } else {
                Err(Error::new(expr.span(), "int literal does not fit in i128"))
            }
        }
        Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => match expr_type_tokens(vars, expr.as_ref()) {
            Ok((
                AExprType::Scalar {
                    is_pub: true,
                    is_vec: false,
                    val: Some(v),
                },
                le,
            )) => {
                // If v happens to be i128::MIN, then -v isn't an i128.
                if let Some(negv) = v.checked_neg() {
                    Ok((
                        AExprType::Scalar {
                            is_pub: true,
                            is_vec: false,
                            val: Some(negv),
                        },
                        const_i128_tokens(negv),
                    ))
                } else {
                    Ok((
                        AExprType::Scalar {
                            is_pub: true,
                            is_vec: false,
                            val: None,
                        },
                        quote! { -#le },
                    ))
                }
            }
            Ok((other, le)) => Ok((other, quote! { -#le })),
            Err(err) => Err(err),
        },
        Expr::Paren(syn::ExprParen { expr, .. }) => match expr_type_tokens(vars, expr.as_ref()) {
            Ok((aetype, ex)) => Ok((aetype, quote! { (#ex) })),
            Err(err) => Err(err),
        },
        Expr::Path(syn::ExprPath { path, .. }) => {
            if let Some(id) = path.get_ident() {
                if let Some(&vt) = vars.get(&id.to_string()) {
                    return Ok((vt, quote! { #id }));
                }
            }
            Err(Error::new(expr.span(), "not a known variable"))
        }
        Expr::Binary(syn::ExprBinary {
            left, op, right, ..
        }) => {
            match op {
                syn::BinOp::Add(_) | syn::BinOp::Sub(_) => {
                    let (lt, le) = expr_type_tokens(vars, left.as_ref())?;
                    let (rt, re) = expr_type_tokens(vars, right.as_ref())?;
                    let default_tokens = match op {
                        syn::BinOp::Add(_) => quote! { #le + #re },
                        syn::BinOp::Sub(_) => quote! { #le - #re },
                        // The default match can't happen
                        // because we're already inside a match
                        // on op, but the compiler requires it
                        // anyway
                        _ => quote! {0},
                    };
                    // You can add or subtract two Scalars or two
                    // Points, but not a Scalar and a Point.  The result
                    // is public if both arguments are public.  The
                    // result is a vector if either argument is a
                    // vector.
                    match (lt, rt) {
                        (
                            AExprType::Scalar {
                                is_pub: lpub,
                                is_vec: lvec,
                                val: lval,
                            },
                            AExprType::Scalar {
                                is_pub: rpub,
                                is_vec: rvec,
                                val: rval,
                            },
                        ) => {
                            let val = if let (Some(lv), Some(rv)) = (lval, rval) {
                                match op {
                                    syn::BinOp::Add(_) => lv.checked_add(rv),
                                    syn::BinOp::Sub(_) => lv.checked_sub(rv),
                                    // The default match can't
                                    // happen because we're already
                                    // inside a match on op, but the
                                    // compiler requires it anyway
                                    _ => None,
                                }
                            } else {
                                None
                            };
                            return Ok((
                                AExprType::Scalar {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                    val,
                                },
                                if let Some(v) = val {
                                    const_i128_tokens(v)
                                } else {
                                    default_tokens
                                },
                            ));
                        }
                        (
                            AExprType::Point {
                                is_pub: lpub,
                                is_vec: lvec,
                            },
                            AExprType::Point {
                                is_pub: rpub,
                                is_vec: rvec,
                            },
                        ) => {
                            return Ok((
                                AExprType::Point {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                },
                                default_tokens,
                            ));
                        }
                        _ => {}
                    }
                    return Err(Error::new(
                        expr.span(),
                        "cannot add/subtract a Scalar and a Point",
                    ));
                }
                syn::BinOp::Mul(_) => {
                    let (lt, le) = expr_type_tokens(vars, left.as_ref())?;
                    let (rt, re) = expr_type_tokens(vars, right.as_ref())?;
                    let default_tokens = quote! { #le * #re };
                    // You can multiply two Scalars or a Scalar and a
                    // Point, but not two Points.  You can also not
                    // multiply two private expressions.  The result is
                    // public if both arguments are public.  The result
                    // is a vector if either argument is a vector.
                    match (lt, rt) {
                        (
                            AExprType::Scalar {
                                is_pub: lpub,
                                is_vec: lvec,
                                val: lval,
                            },
                            AExprType::Scalar {
                                is_pub: rpub,
                                is_vec: rvec,
                                val: rval,
                            },
                        ) => {
                            if !lpub && !rpub {
                                return Err(Error::new(
                                    expr.span(),
                                    "cannot multiply two private expressions",
                                ));
                            }
                            let val = if let (Some(lv), Some(rv)) = (lval, rval) {
                                lv.checked_mul(rv)
                            } else {
                                None
                            };
                            return Ok((
                                AExprType::Scalar {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                    val,
                                },
                                if let Some(v) = val {
                                    const_i128_tokens(v)
                                } else {
                                    default_tokens
                                },
                            ));
                        }
                        (
                            AExprType::Scalar {
                                is_pub: lpub,
                                is_vec: lvec,
                                ..
                            },
                            AExprType::Point {
                                is_pub: rpub,
                                is_vec: rvec,
                            },
                        )
                        | (
                            AExprType::Point {
                                is_pub: lpub,
                                is_vec: lvec,
                            },
                            AExprType::Scalar {
                                is_pub: rpub,
                                is_vec: rvec,
                                ..
                            },
                        ) => {
                            if !lpub && !rpub {
                                return Err(Error::new(
                                    expr.span(),
                                    "cannot multiply two private expressions",
                                ));
                            }
                            return Ok((
                                AExprType::Point {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                },
                                default_tokens,
                            ));
                        }
                        _ => {}
                    }
                    return Err(Error::new(
                        expr.span(),
                        "cannot multiply a Point and a Point",
                    ));
                }
                syn::BinOp::Shl(_) => {
                    let lt = expr_type(vars, left.as_ref())?;
                    let rt = expr_type(vars, right.as_ref())?;
                    // You can << only when both operands are constant
                    // Scalar expressions
                    if let (
                        AExprType::Scalar {
                            is_pub: true,
                            is_vec: false,
                            val: Some(lv),
                        },
                        AExprType::Scalar {
                            is_pub: true,
                            is_vec: false,
                            val: Some(rv),
                        },
                    ) = (lt, rt)
                    {
                        let rvu32: Option<u32> = rv.try_into().ok();
                        if let Some(shift_amt) = rvu32 {
                            if let Some(v) = lv.checked_shl(shift_amt) {
                                return Ok((
                                    AExprType::Scalar {
                                        is_pub: true,
                                        is_vec: false,
                                        val: Some(v),
                                    },
                                    const_i128_tokens(v),
                                ));
                            }
                        }
                    }
                    return Err(Error::new(
                        expr.span(),
                        "can shift left only on constant i128 expressions",
                    ));
                }
                _ => {}
            }
            Err(Error::new(
                op.span(),
                "invalid operation for arithmetic expression",
            ))
        }
        _ => Err(Error::new(expr.span(), "not a valid arithmetic expression")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn check(vars: &VarDict, expr: Expr, expect: &str) {
        assert_eq!(expr_type(vars, &expr).unwrap(), AExprType::from(expect));
    }

    fn check_const(vars: &VarDict, expr: Expr, expect: i128) {
        assert_eq!(
            expr_type(vars, &expr).unwrap(),
            AExprType::Scalar {
                is_pub: true,
                is_vec: false,
                val: Some(expect),
            }
        );
    }

    fn check_tokens(vars: &VarDict, expr: Expr, expect: TokenStream) {
        assert_eq!(
            expr_type_tokens(vars, &expr).unwrap().1.to_string(),
            expect.to_string()
        );
    }

    fn check_fail(vars: &VarDict, expr: Expr) {
        expr_type(vars, &expr).unwrap_err();
    }

    #[test]
    fn expr_type_test() {
        let vars: VarDict = vardict_from_strs(&[("a", "S"), ("A", "pP"), ("v", "vS")]);
        check_const(&vars, parse_quote! {2}, 2);
        check_const(&vars, parse_quote! {-4}, -4);
        check_const(&vars, parse_quote! {(2)}, 2);
        check_const(&vars, parse_quote! {1<<20}, 1048576);
        check_const(&vars, parse_quote! {(3-2)<<(4*5)}, 1048576);
        check(&vars, parse_quote! {A}, "pP");
        check(&vars, parse_quote! {a*A}, "P");
        check(&vars, parse_quote! {A*3}, "pP");
        check(&vars, parse_quote! {(a-1)*(A+A)}, "P");
        check(&vars, parse_quote! {(v-1)*(A+A)}, "vP");
        check_tokens(
            &vars,
            parse_quote! { 0 },
            quote! { Scalar::from_u128(0u128) },
        );
        check_tokens(
            &vars,
            parse_quote! { 5 },
            quote! { Scalar::from_u128(5u128) },
        );
        check_tokens(
            &vars,
            parse_quote! { -77 },
            quote! { Scalar::from_u128(77u128).neg() },
        );
        check_tokens(
            &vars,
            parse_quote! { 1<<20 },
            quote! {
            Scalar::from_u128(1048576u128) },
        );
        check_tokens(
            &vars,
            parse_quote! { (3-2)<<(4*5) },
            quote! {
            Scalar::from_u128(1048576u128) },
        );
        check_tokens(
            &vars,
            parse_quote! { 127<<120 },
            quote! {
            Scalar::from_u128(168811955464684315858783496655603761152u128) },
        );
        check_tokens(
            &vars,
            parse_quote! { -(-170141183460469231731687303715884105727) },
            quote! {
            Scalar::from_u128(170141183460469231731687303715884105727u128) },
        );
        // -2^127 fits in an i128, but the negative of that does not
        check_tokens(
            &vars,
            parse_quote! { -(-170141183460469231731687303715884105727-1) },
            quote! {
            -(Scalar::from_u128(170141183460469231731687303715884105728u128).neg()) },
        );
        check_tokens(
            &vars,
            parse_quote! {(a-(2-3))*(A+(3*4)*A)},
            quote! {
            (a-(Scalar::from_u128(1u128).neg()))*(A+(Scalar::from_u128(12u128))*A) },
        );

        // Tests that should fail

        // unknown variable
        check_fail(&vars, parse_quote! {B});
        // adding a Scalar to a Point
        check_fail(&vars, parse_quote! {a+A});
        // multiplying two Points
        check_fail(&vars, parse_quote! {A*A});
        // invalid operation
        check_fail(&vars, parse_quote! {A/A});
        // invalid expression
        check_fail(&vars, parse_quote! {A.size});
        // multiplying two private expressions (two ways)
        check_fail(&vars, parse_quote! {a*a});
        check_fail(&vars, parse_quote! {a*(a*A)});
        // Shifting non-constant expressions
        check_fail(&vars, parse_quote! {a<<2});
        check_fail(&vars, parse_quote! {1<<a});
    }
}

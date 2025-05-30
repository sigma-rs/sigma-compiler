//! At the `sigma` level, each variable can be a private `Scalar`, a
//! public `Scalar`, or a public `Point`, and each variable can be
//! either a vector or not.  Arithmetic expressions of those variables
//! can be of any of those types, and also private `Point`s (vector or
//! not).  This module defines an enum [`AExprType`] for
//! the possible types, as well as a dictionary type that maps
//! [`String`]s (the name of the variable) to [`AExprType`], and a
//! function for determining the type of arithmetic expressions
//! involving such variables.

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
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AExprType {
    Scalar { is_pub: bool, is_vec: bool },
    Point { is_pub: bool, is_vec: bool },
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
            },
            "pub Scalar" | "pS" => Self::Scalar {
                is_pub: true,
                is_vec: false,
            },
            "vec Scalar" | "vS" => Self::Scalar {
                is_pub: false,
                is_vec: true,
            },
            "pub vec Scalar" | "pvS" => Self::Scalar {
                is_pub: true,
                is_vec: true,
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

/// Given a [`VarDict`] and an [`Expr`] representing an arithmetic
/// expression using the variables in the [`VarDict`], compute the
/// [`AExprType`] of the expression.
///
/// An arithmetic expression can consist of:
///   - variables that are in the [`VarDict`]
///   - integer constants
///   - the operations `*`, `+`, `-` (binary or unary)
///   - parens
pub fn expr_type(vars: &VarDict, expr: &Expr) -> Result<AExprType> {
    match expr {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(_),
            ..
        }) => Ok(AExprType::Scalar {
            is_pub: true,
            is_vec: false,
        }),
        Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => expr_type(vars, expr.as_ref()),
        Expr::Paren(syn::ExprParen { expr, .. }) => expr_type(vars, expr.as_ref()),
        Expr::Path(syn::ExprPath { path, .. }) => {
            if let Some(id) = path.get_ident() {
                if let Some(&vt) = vars.get(&id.to_string()) {
                    return Ok(vt);
                }
            }
            Err(Error::new(expr.span(), "not a known variable"))
        }
        Expr::Binary(syn::ExprBinary {
            left, op, right, ..
        }) => {
            match op {
                syn::BinOp::Add(_) | syn::BinOp::Sub(_) => {
                    let lt = expr_type(vars, left.as_ref())?;
                    let rt = expr_type(vars, right.as_ref())?;
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
                            },
                            AExprType::Scalar {
                                is_pub: rpub,
                                is_vec: rvec,
                            },
                        ) => {
                            return Ok(AExprType::Scalar {
                                is_pub: lpub && rpub,
                                is_vec: lvec || rvec,
                            });
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
                            return Ok(AExprType::Point {
                                is_pub: lpub && rpub,
                                is_vec: lvec || rvec,
                            });
                        }
                        _ => {}
                    }
                    return Err(Error::new(
                        expr.span(),
                        "cannot add/subtract a Scalar and a Point",
                    ));
                }
                syn::BinOp::Mul(_) => {
                    let lt = expr_type(vars, left.as_ref())?;
                    let rt = expr_type(vars, right.as_ref())?;
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
                            },
                            AExprType::Scalar {
                                is_pub: rpub,
                                is_vec: rvec,
                            },
                        ) => {
                            if !lpub && !rpub {
                                return Err(Error::new(
                                    expr.span(),
                                    "cannot multiply two private expressions",
                                ));
                            }
                            return Ok(AExprType::Scalar {
                                is_pub: lpub && rpub,
                                is_vec: lvec || rvec,
                            });
                        }
                        (
                            AExprType::Scalar {
                                is_pub: lpub,
                                is_vec: lvec,
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
                            },
                        ) => {
                            if !lpub && !rpub {
                                return Err(Error::new(
                                    expr.span(),
                                    "cannot multiply two private expressions",
                                ));
                            }
                            return Ok(AExprType::Point {
                                is_pub: lpub && rpub,
                                is_vec: lvec || rvec,
                            });
                        }
                        _ => {}
                    }
                    return Err(Error::new(
                        expr.span(),
                        "cannot multiply a Point and a Point",
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

    fn check_fail(vars: &VarDict, expr: Expr) {
        expr_type(vars, &expr).unwrap_err();
    }

    #[test]
    fn test_expr_type() {
        let vars: VarDict = vardict_from_strs(&[("a", "S"), ("A", "pP"), ("v", "vS")]);
        check(&vars, parse_quote! {2}, "pS");
        check(&vars, parse_quote! {-4}, "pS");
        check(&vars, parse_quote! {(2)}, "pS");
        check(&vars, parse_quote! {A}, "pP");
        check(&vars, parse_quote! {a*A}, "P");
        check(&vars, parse_quote! {A*3}, "pP");
        check(&vars, parse_quote! {(a-1)*(A+A)}, "P");
        check(&vars, parse_quote! {(v-1)*(A+A)}, "vP");

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
    }
}

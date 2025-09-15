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
use syn::{parse_quote, Error, Expr, Ident};

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
pub fn const_i128_tokens(val: i128) -> TokenStream {
    let uval = val.unsigned_abs();
    if val >= 0 {
        quote! { Scalar::from_u128(#uval) }
    } else {
        quote! { Scalar::from_u128(#uval).neg() }
    }
}

/// A trait for fold-style evaluations over arithmetic expressions.
///
/// The parameter `T` is the type you want to return.
/// All functions take `(AExprType, T)` for each of the components of
/// the arithmetic expression node, as well as the [`AExprType`] for the
/// resulting value.
///
/// An arithmetic expression can consist of:
///   - variables that are in the [`VarDict`]
///   - integer constants
///   - the operations `*`, `+`, `-` (binary or unary)
///   - the operation `<<` where both operands are expressions with no
///     variables
///   - the function `sum` that takes a single vector argument and
///     returns the sum of its elements
///   - parens
pub trait AExprFold<T> {
    /// Called when an identifier found in the [`VarDict`] is
    /// encountered in the [`Expr`]
    fn ident(&mut self, id: &Ident, restype: AExprType) -> Result<T>;

    /// Called when the arithmetic expression evaluates to a constant
    /// [`i128`] value.
    fn const_i128(&mut self, restype: AExprType) -> Result<T>;

    /// Called for unary negation
    fn neg(&mut self, arg: (AExprType, T), restype: AExprType) -> Result<T>;

    /// Called for a parenthesized expression
    fn paren(&mut self, arg: (AExprType, T), restype: AExprType) -> Result<T>;

    /// Called when adding two `Scalar`s
    fn add_scalars(
        &mut self,
        larg: (AExprType, T),
        rarg: (AExprType, T),
        restype: AExprType,
    ) -> Result<T>;

    /// Called when adding two `Point`s
    fn add_points(
        &mut self,
        larg: (AExprType, T),
        rarg: (AExprType, T),
        restype: AExprType,
    ) -> Result<T>;

    /// Called when summing a vector of `Scalar`s
    fn sum_scalars(&mut self, arg: (AExprType, T), restype: AExprType) -> Result<T>;

    /// Called when summing a vector of `Point`s
    fn sum_points(&mut self, arg: (AExprType, T), restype: AExprType) -> Result<T>;

    /// Called when subtracting two `Scalar`s
    fn sub_scalars(
        &mut self,
        larg: (AExprType, T),
        rarg: (AExprType, T),
        restype: AExprType,
    ) -> Result<T>;

    /// Called when subtracting two `Point`s
    fn sub_points(
        &mut self,
        larg: (AExprType, T),
        rarg: (AExprType, T),
        restype: AExprType,
    ) -> Result<T>;

    /// Called when multiplying two `Scalar`s
    fn mul_scalars(
        &mut self,
        larg: (AExprType, T),
        rarg: (AExprType, T),
        restype: AExprType,
    ) -> Result<T>;

    /// Called when multiplying a `Scalar` and a `Point` (the `Scalar`
    /// will always be passed as the first argument)
    fn mul_scalar_point(
        &mut self,
        sarg: (AExprType, T),
        parg: (AExprType, T),
        restype: AExprType,
    ) -> Result<T>;

    /// Recursively process an arithmetic expression given by the
    /// [`Expr`]
    fn fold(&mut self, vars: &VarDict, expr: &Expr) -> Result<(AExprType, T)> {
        match expr {
            Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(litint),
                ..
            }) => {
                let val = litint.base10_parse::<i128>().ok();
                if val.is_some() {
                    let restype = AExprType::Scalar {
                        is_pub: true,
                        is_vec: false,
                        val,
                    };
                    let res = self.const_i128(restype)?;
                    Ok((restype, res))
                } else {
                    Err(Error::new(expr.span(), "int literal does not fit in i128"))
                }
            }
            Expr::Unary(syn::ExprUnary {
                op: syn::UnOp::Neg(_),
                expr,
                ..
            }) => match self.fold(vars, expr.as_ref()) {
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
                        let restype = AExprType::Scalar {
                            is_pub: true,
                            is_vec: false,
                            val: Some(negv),
                        };
                        let res = self.const_i128(restype)?;
                        Ok((restype, res))
                    } else {
                        let restype = AExprType::Scalar {
                            is_pub: true,
                            is_vec: false,
                            val: None,
                        };
                        let res = self.neg(
                            (
                                AExprType::Scalar {
                                    is_pub: true,
                                    is_vec: false,
                                    val: Some(v),
                                },
                                le,
                            ),
                            restype,
                        )?;
                        Ok((restype, res))
                    }
                }
                Ok((other, le)) => {
                    let res = self.neg((other, le), other)?;
                    Ok((other, res))
                }
                Err(err) => Err(err),
            },
            Expr::Paren(syn::ExprParen { expr, .. }) => match self.fold(vars, expr.as_ref()) {
                Ok((aetype, ex)) => {
                    let res = self.paren((aetype, ex), aetype)?;
                    Ok((aetype, res))
                }
                Err(err) => Err(err),
            },
            Expr::Path(syn::ExprPath { path, .. }) => {
                if let Some(id) = path.get_ident() {
                    if let Some(&vt) = vars.get(&id.to_string()) {
                        let res = self.ident(id, vt)?;
                        return Ok((vt, res));
                    }
                }
                Err(Error::new(expr.span(), "not a known variable"))
            }
            Expr::Binary(syn::ExprBinary {
                left, op, right, ..
            }) => {
                match op {
                    syn::BinOp::Add(_) | syn::BinOp::Sub(_) => {
                        let (lt, le) = self.fold(vars, left.as_ref())?;
                        let (rt, re) = self.fold(vars, right.as_ref())?;
                        let is_add = matches!(op, syn::BinOp::Add(_));
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
                                let restype = AExprType::Scalar {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                    val,
                                };
                                let res = if val.is_some() {
                                    self.const_i128(restype)?
                                } else if is_add {
                                    self.add_scalars((lt, le), (rt, re), restype)?
                                } else {
                                    self.sub_scalars((lt, le), (rt, re), restype)?
                                };
                                return Ok((restype, res));
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
                                let restype = AExprType::Point {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                };
                                let res = if is_add {
                                    self.add_points((lt, le), (rt, re), restype)?
                                } else {
                                    self.sub_points((lt, le), (rt, re), restype)?
                                };
                                return Ok((restype, res));
                            }
                            _ => {}
                        }
                        return Err(Error::new(
                            expr.span(),
                            "cannot add/subtract a Scalar and a Point",
                        ));
                    }
                    syn::BinOp::Mul(_) => {
                        let (lt, le) = self.fold(vars, left.as_ref())?;
                        let (rt, re) = self.fold(vars, right.as_ref())?;
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
                                let restype = AExprType::Scalar {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                    val,
                                };
                                let res = if val.is_some() {
                                    self.const_i128(restype)?
                                } else {
                                    self.mul_scalars((lt, le), (rt, re), restype)?
                                };
                                return Ok((restype, res));
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
                                // Whichever order we were passed the
                                // Scalar and the Point, we pass to
                                // mul_scalar_point the Scalar first and
                                // the Point second.
                                let (ste, pte) = if matches!(lt, AExprType::Scalar { .. }) {
                                    ((lt, le), (rt, re))
                                } else {
                                    ((rt, re), (lt, le))
                                };
                                let restype = AExprType::Point {
                                    is_pub: lpub && rpub,
                                    is_vec: lvec || rvec,
                                };
                                let res = self.mul_scalar_point(ste, pte, restype)?;
                                return Ok((restype, res));
                            }
                            _ => {}
                        }
                        return Err(Error::new(
                            expr.span(),
                            "cannot multiply a Point and a Point",
                        ));
                    }
                    syn::BinOp::Shl(_) => {
                        let (lt, _) = self.fold(vars, left.as_ref())?;
                        let (rt, _) = self.fold(vars, right.as_ref())?;
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
                                    let restype = AExprType::Scalar {
                                        is_pub: true,
                                        is_vec: false,
                                        val: Some(v),
                                    };
                                    let res = self.const_i128(restype)?;
                                    return Ok((restype, res));
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
            Expr::Call(syn::ExprCall { func, args, .. }) => {
                let funcname = match func.as_ref() {
                    Expr::Path(syn::ExprPath { path, .. }) => {
                        path.get_ident().map(|id| id.to_string())
                    }
                    _ => None,
                };
                match funcname {
                    Some(ref s) if s == "sum" => {
                        if args.len() != 1 {
                            return Err(Error::new(
                                func.span(),
                                "sum must have exactly one argument",
                            ));
                        }
                        let (at, ae) = self.fold(vars, args.first().unwrap())?;
                        match at {
                            AExprType::Scalar {
                                is_vec: true,
                                is_pub,
                                ..
                            } => {
                                let restype = AExprType::Scalar {
                                    is_vec: false,
                                    is_pub,
                                    val: None,
                                };
                                let res = self.sum_scalars((at, ae), restype)?;
                                Ok((restype, res))
                            }
                            AExprType::Point {
                                is_vec: true,
                                is_pub,
                            } => {
                                let restype = AExprType::Point {
                                    is_vec: false,
                                    is_pub,
                                };
                                let res = self.sum_points((at, ae), restype)?;
                                Ok((restype, res))
                            }
                            _ => Err(Error::new(args.span(), "argument to sum must be a vector")),
                        }
                    }
                    _ => Err(Error::new(func.span(), "unknown function")),
                }
            }
            _ => Err(Error::new(expr.span(), "not a valid arithmetic expression")),
        }
    }
}

struct FoldNoOp;

impl AExprFold<()> for FoldNoOp {
    /// Called when an identifier found in the [`VarDict`] is
    /// encountered in the [`Expr`]
    fn ident(&mut self, _id: &Ident, _restype: AExprType) -> Result<()> {
        Ok(())
    }

    /// Called when the arithmetic expression evaluates to a constant
    /// [`i128`] value.
    fn const_i128(&mut self, _restype: AExprType) -> Result<()> {
        Ok(())
    }

    /// Called for unary negation
    fn neg(&mut self, _arg: (AExprType, ()), _restype: AExprType) -> Result<()> {
        Ok(())
    }

    /// Called for a parenthesized expression
    fn paren(&mut self, _arg: (AExprType, ()), _restype: AExprType) -> Result<()> {
        Ok(())
    }

    /// Called when adding two `Scalar`s
    fn add_scalars(
        &mut self,
        _larg: (AExprType, ()),
        _rarg: (AExprType, ()),
        _restype: AExprType,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when adding two `Point`s
    fn add_points(
        &mut self,
        _larg: (AExprType, ()),
        _rarg: (AExprType, ()),
        _restype: AExprType,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when summing a vector of `Scalar`s
    fn sum_scalars(&mut self, _arg: (AExprType, ()), _restype: AExprType) -> Result<()> {
        Ok(())
    }

    /// Called when summing a vector of `Point`s
    fn sum_points(&mut self, _arg: (AExprType, ()), _restype: AExprType) -> Result<()> {
        Ok(())
    }

    /// Called when subtracting two `Scalar`s
    fn sub_scalars(
        &mut self,
        _larg: (AExprType, ()),
        _rarg: (AExprType, ()),
        _restype: AExprType,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when subtracting two `Point`s
    fn sub_points(
        &mut self,
        _larg: (AExprType, ()),
        _rarg: (AExprType, ()),
        _restype: AExprType,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when multiplying two `Scalar`s
    fn mul_scalars(
        &mut self,
        _larg: (AExprType, ()),
        _rarg: (AExprType, ()),
        _restype: AExprType,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when multiplying a `Scalar` and a `Point` (the `Scalar`
    /// will always be passed as the first argument)
    fn mul_scalar_point(
        &mut self,
        _sarg: (AExprType, ()),
        _parg: (AExprType, ()),
        _restype: AExprType,
    ) -> Result<()> {
        Ok(())
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
///   - the function `sum` that takes a single vector argument and
///     returns the sum of its elements
///   - parens
pub fn expr_type(vars: &VarDict, expr: &Expr) -> Result<AExprType> {
    let mut fold = FoldNoOp {};
    Ok(fold.fold(vars, expr)?.0)
}

/// Add parentheses around a [`TokenStream`] (which represents an
/// [arithmetic expression]) if needed.
///
/// The parentheses are needed if the [`TokenStream`] would parse as
/// multiple tokens.  For example, `a+b` turns into `(a+b)`, but `c`
/// remains `c` and `(a+b)` remains `(a+b)`.
///
/// [arithmetic expression]: expr_type
pub fn tokens_paren_if_needed(tok: TokenStream) -> TokenStream {
    let expr: Expr = parse_quote! { #tok };
    match expr {
        Expr::Unary(_) | Expr::Binary(_) => quote! { (#tok) },
        _ => tok,
    }
}

/// Given [`TokenStream`]s of two arguments, and [`bool`]s saying whether
/// those arguments are vectors or not, construct a [`TokenStream`]
/// representing their sum.
pub fn tokens_add_maybe_vec(
    left: TokenStream,
    left_is_vec: bool,
    right: TokenStream,
    right_is_vec: bool,
) -> TokenStream {
    let lp = tokens_paren_if_needed(left);
    let rp = tokens_paren_if_needed(right);
    match (left_is_vec, right_is_vec) {
        (true, true) => quote! { add_vecs(&#lp, &#rp) },
        (false, true) => quote! { add_nv_vec(&#lp, &#rp) },
        (true, false) => quote! { add_vec_nv(&#lp, &#rp) },
        (false, false) => quote! { #lp + #rp },
    }
}

/// Given [`TokenStream`]s of two arguments, and [`bool`]s saying whether
/// those arguments are vectors or not, construct a [`TokenStream`]
/// representing their difference.
pub fn tokens_sub_maybe_vec(
    left: TokenStream,
    left_is_vec: bool,
    right: TokenStream,
    right_is_vec: bool,
) -> TokenStream {
    let lp = tokens_paren_if_needed(left);
    let rp = tokens_paren_if_needed(right);
    match (left_is_vec, right_is_vec) {
        (true, true) => quote! { sub_vecs(&#lp, &#rp) },
        (false, true) => quote! { sub_nv_vec(&#lp, &#rp) },
        (true, false) => quote! { sub_vec_nv(&#lp, &#rp) },
        (false, false) => quote! { #lp - #rp },
    }
}

/// Given [`TokenStream`]s of two arguments, and [`bool`]s saying whether
/// those arguments are vectors or not, construct a [`TokenStream`]
/// representing their product.
pub fn tokens_mul_maybe_vec(
    left: TokenStream,
    left_is_vec: bool,
    right: TokenStream,
    right_is_vec: bool,
) -> TokenStream {
    let lp = tokens_paren_if_needed(left);
    let rp = tokens_paren_if_needed(right);
    match (left_is_vec, right_is_vec) {
        (true, true) => quote! { mul_vecs(&#lp, &#rp) },
        (false, true) => quote! { mul_nv_vec(&#lp, &#rp) },
        (true, false) => quote! { mul_vec_nv(&#lp, &#rp) },
        (false, false) => quote! { #lp * #rp },
    }
}

pub struct AExprTokenFold<'a> {
    ident_closure: &'a mut dyn FnMut(&Ident, AExprType) -> Result<TokenStream>,
}

impl<'a> AExprFold<TokenStream> for AExprTokenFold<'a> {
    /// Called when an identifier found in the [`VarDict`] is
    /// encountered in the [`Expr`]
    fn ident(&mut self, id: &Ident, restype: AExprType) -> Result<TokenStream> {
        (self.ident_closure)(id, restype)
    }

    /// Called when the arithmetic expression evaluates to a constant
    /// [`i128`] value.
    fn const_i128(&mut self, restype: AExprType) -> Result<TokenStream> {
        let AExprType::Scalar { val: Some(v), .. } = restype else {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "BUG: it should not happen that const_i128 is called without a value",
            ));
        };
        Ok(const_i128_tokens(v))
    }

    /// Called for unary negation
    fn neg(&mut self, arg: (AExprType, TokenStream), _restype: AExprType) -> Result<TokenStream> {
        let ae = arg.1;
        Ok(quote! { -#ae })
    }

    /// Called for a parenthesized expression
    fn paren(&mut self, arg: (AExprType, TokenStream), _restype: AExprType) -> Result<TokenStream> {
        let ae = arg.1;
        Ok(quote! { (#ae) })
    }

    /// Called when adding two `Scalar`s
    fn add_scalars(
        &mut self,
        larg: (AExprType, TokenStream),
        rarg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let le = larg.1;
        let re = rarg.1;
        let AExprType::Scalar {
            is_vec: l_is_vec, ..
        } = larg.0
        else {
            panic!("Should not happen: non-Scalar passed to add_scalars");
        };
        let AExprType::Scalar {
            is_vec: r_is_vec, ..
        } = rarg.0
        else {
            panic!("Should not happen: non-Scalar passed to add_scalars");
        };
        Ok(tokens_add_maybe_vec(le, l_is_vec, re, r_is_vec))
    }

    /// Called when adding two `Point`s
    fn add_points(
        &mut self,
        larg: (AExprType, TokenStream),
        rarg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let le = larg.1;
        let re = rarg.1;
        let AExprType::Point {
            is_vec: l_is_vec, ..
        } = larg.0
        else {
            panic!("Should not happen: non-Point passed to add_points");
        };
        let AExprType::Point {
            is_vec: r_is_vec, ..
        } = rarg.0
        else {
            panic!("Should not happen: non-Point passed to add_points");
        };
        Ok(tokens_add_maybe_vec(le, l_is_vec, re, r_is_vec))
    }

    /// Called when summing a vector of `Scalar`s
    fn sum_scalars(
        &mut self,
        arg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let arge = arg.1;
        Ok(quote! { sigma_compiler::vecutils::sum_vec(&(#arge)) })
    }

    /// Called when summing a vector of `Point`s
    fn sum_points(
        &mut self,
        arg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let arge = arg.1;
        Ok(quote! { sigma_compiler::vecutils::sum_vec(&(#arge)) })
    }

    /// Called when subtracting two `Scalar`s
    fn sub_scalars(
        &mut self,
        larg: (AExprType, TokenStream),
        rarg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let le = larg.1;
        let re = rarg.1;
        let AExprType::Scalar {
            is_vec: l_is_vec, ..
        } = larg.0
        else {
            panic!("Should not happen: non-Scalar passed to sub_scalars");
        };
        let AExprType::Scalar {
            is_vec: r_is_vec, ..
        } = rarg.0
        else {
            panic!("Should not happen: non-Scalar passed to sub_scalars");
        };
        Ok(tokens_sub_maybe_vec(le, l_is_vec, re, r_is_vec))
    }

    /// Called when subtracting two `Point`s
    fn sub_points(
        &mut self,
        larg: (AExprType, TokenStream),
        rarg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let le = larg.1;
        let re = rarg.1;
        let AExprType::Point {
            is_vec: l_is_vec, ..
        } = larg.0
        else {
            panic!("Should not happen: non-Point passed to sub_points");
        };
        let AExprType::Point {
            is_vec: r_is_vec, ..
        } = rarg.0
        else {
            panic!("Should not happen: non-Point passed to sub_points");
        };
        Ok(tokens_sub_maybe_vec(le, l_is_vec, re, r_is_vec))
    }

    /// Called when multiplying two `Scalar`s
    fn mul_scalars(
        &mut self,
        larg: (AExprType, TokenStream),
        rarg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let AExprType::Scalar {
            is_pub: l_is_pub,
            is_vec: l_is_vec,
            ..
        } = larg.0
        else {
            panic!("Should not happen: non-Scalar passed to mul_scalars");
        };
        let AExprType::Scalar {
            is_pub: r_is_pub,
            is_vec: r_is_vec,
            ..
        } = rarg.0
        else {
            panic!("Should not happen: non-Scalar passed to mul_scalars");
        };
        // If one is public and one is private, put the private one on
        // the left
        let (lv, le, rv, re) = if l_is_pub && !r_is_pub {
            (r_is_vec, rarg.1, l_is_vec, larg.1)
        } else {
            (l_is_vec, larg.1, r_is_vec, rarg.1)
        };
        Ok(tokens_mul_maybe_vec(le, lv, re, rv))
    }

    /// Called when multiplying a `Scalar` and a `Point` (the `Scalar`
    /// will always be passed as the first argument)
    fn mul_scalar_point(
        &mut self,
        sarg: (AExprType, TokenStream),
        parg: (AExprType, TokenStream),
        _restype: AExprType,
    ) -> Result<TokenStream> {
        let AExprType::Scalar {
            is_pub: s_is_pub,
            is_vec: s_is_vec,
            ..
        } = sarg.0
        else {
            panic!("Should not happen: non-Scalar passed to mul_scalar_point");
        };
        let AExprType::Point {
            is_vec: p_is_vec, ..
        } = parg.0
        else {
            panic!("Should not happen: non-Point passed to mul_scalar_point");
        };
        // If the Scalar is public, put it on the right
        let (lv, le, rv, re) = if s_is_pub {
            (p_is_vec, parg.1, s_is_vec, sarg.1)
        } else {
            (s_is_vec, sarg.1, p_is_vec, parg.1)
        };
        Ok(tokens_mul_maybe_vec(le, lv, re, rv))
    }
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
///   - the function `sum` that takes a single vector argument and
///     returns the sum of its elements
///   - parens
pub fn expr_type_tokens(vars: &VarDict, expr: &Expr) -> Result<(AExprType, TokenStream)> {
    let mut fold = AExprTokenFold {
        ident_closure: &mut |id, _var_type| Ok(quote! { #id }),
    };
    fold.fold(vars, expr)
}

/// Like [`expr_type_tokens`], but call a custom closure on encountering
/// each [`struct@Ident`].
pub fn expr_type_tokens_id_closure(
    vars: &VarDict,
    expr: &Expr,
    ident_closure: &mut dyn FnMut(&Ident, AExprType) -> Result<TokenStream>,
) -> Result<(AExprType, TokenStream)> {
    let mut fold = AExprTokenFold { ident_closure };
    fold.fold(vars, expr)
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
            (a-(Scalar::from_u128(1u128).neg()))*(A+(A*(Scalar::from_u128(12u128)))) },
        );
        check_tokens(
            &vars,
            parse_quote! {(a-(2-3))*(A+A*(3*4))},
            quote! {
            (a-(Scalar::from_u128(1u128).neg()))*(A+(A*(Scalar::from_u128(12u128)))) },
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

//! A module for finding and manipulating Pedersen commitments in a
//! [`StatementTree`].
//!
//! A Pedersen commitment to a private `Scalar` `x` looks like
//!
//! `C = (a*x+b)*A + (c*r+d)*B`
//!
//! Where `a` and `c` are a constant non-zero `Scalar`s (defaults to
//! [`Scalar::ONE`](https://docs.rs/ff/0.13.1/ff/trait.Field.html#associatedconstant.ONE)),
//! `b`, and `d` are public `Scalar`s or constants (or combinations of
//! those), `r` is a random private `Scalar` that appears nowhere else
//! in the [`StatementTree`], `C` is a public `Point`, and `A` and `B`
//! are computationally independent public `Point`s.

use super::sigma::combiners::*;
use super::sigma::types::*;
use super::syntax::*;
use super::transform::paren_if_needed;
use std::collections::{HashMap, HashSet};
use syn::parse::Result;
use syn::visit::Visit;
use syn::{parse_quote, Error, Expr, Ident};

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

/// A representation of `a*x + b` where `a` is a constant `Scalar`, `b`
/// is a public `Scalar` [arithmetic expression], and `x` is a private
/// `Scalar` variable.  `a` is optional, and defaults to `1`. `b` is
/// optional, and defaults to `0`
///
/// [arithmetic expression]: expr_type
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinScalar {
    /// The coefficient `a`
    pub coeff: i128,
    /// The public `Scalar` expression `b`, if present
    pub pub_scalar_expr: Option<Expr>,
    /// The private `Scalar` `x`
    pub id: Ident,
    /// Whether `x` is a vector variable
    pub is_vec: bool,
}

impl LinScalar {
    /// Negate a [`LinScalar`]
    pub fn neg(self) -> Result<Self> {
        Ok(Self {
            coeff: self.coeff.checked_neg().ok_or(Error::new(
                proc_macro2::Span::call_site(),
                "i128 neg overflow",
            ))?,
            pub_scalar_expr: if let Some(expr) = self.pub_scalar_expr {
                let pexpr = paren_if_needed(expr);
                Some(parse_quote! { -#pexpr })
            } else {
                None
            },
            ..self
        })
    }

    /// Add a public `Scalar` expression to a [`LinScalar`]
    pub fn add_opt_pub_scalar_expr(self, opsexpr: Option<Expr>) -> Result<Self> {
        if let Some(psexpr) = opsexpr {
            Ok(Self {
                pub_scalar_expr: if let Some(expr) = self.pub_scalar_expr {
                    let ppsexpr = paren_if_needed(psexpr);
                    Some(parse_quote! { #expr + #ppsexpr })
                } else {
                    Some(psexpr)
                },
                ..self
            })
        } else {
            Ok(self)
        }
    }

    /// Add a [`LinScalar`] to a [`LinScalar`].
    ///
    /// The private variables must match.
    pub fn add_linscalar(self, arg: Self) -> Result<Self> {
        if self.id != arg.id {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "private variables in added LinScalars do not match",
            ));
        }
        Self {
            coeff: self.coeff.checked_add(arg.coeff).ok_or(Error::new(
                proc_macro2::Span::call_site(),
                "i128 add overflow",
            ))?,
            ..self
        }
        .add_opt_pub_scalar_expr(arg.pub_scalar_expr)
    }

    /// Multiply a [`LinScalar`] by a constant
    pub fn mul_const(self, arg: i128) -> Result<Self> {
        Ok(Self {
            coeff: self.coeff.checked_mul(arg).ok_or(Error::new(
                proc_macro2::Span::call_site(),
                "i128 mul overflow",
            ))?,
            pub_scalar_expr: if let Some(expr) = self.pub_scalar_expr {
                if arg == 1 {
                    Some(expr)
                } else {
                    let pexpr = paren_if_needed(expr);
                    Some(parse_quote! { #pexpr * #arg })
                }
            } else {
                None
            },
            ..self
        })
    }
}

/// A representation of `b*A` where `b` is a public `Scalar` [arithmetic
/// expression] (default to `1`) and `A` is a computationally
/// independent `Point`.
///
/// [arithmetic expression]: expr_type
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CIndPoint {
    /// The public `Scalar` expression `b`
    pub coeff: Option<Expr>,
    /// The value of the coeff, if it's a constant
    pub coeff_val: Option<i128>,
    /// The public `Point` `A`
    pub id: Ident,
}

impl CIndPoint {
    /// Negate a [`CIndPoint`]
    pub fn neg(self) -> Result<Self> {
        Ok(Self {
            coeff: Some(if let Some(expr) = self.coeff {
                let pexpr = paren_if_needed(expr);
                parse_quote! { -#pexpr }
            } else {
                parse_quote! { -1 }
            }),
            coeff_val: if let Some(val) = self.coeff_val {
                val.checked_neg()
            } else {
                None
            },
            ..self
        })
    }

    /// Add a [`CIndPoint`] to a [`CIndPoint`]
    pub fn add_cind(self, arg: CIndPoint) -> Result<Self> {
        if self.id != arg.id {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "public points in added CIndPoints do not match",
            ));
        }
        let lexpr = if let Some(expr) = self.coeff {
            expr
        } else {
            parse_quote! { 1 }
        };
        let rexpr = if let Some(expr) = arg.coeff {
            paren_if_needed(expr)
        } else {
            parse_quote! { 1 }
        };
        let coeff_val = if let (Some(lval), Some(rval)) = (self.coeff_val, arg.coeff_val) {
            lval.checked_add(rval)
        } else {
            None
        };
        Ok(Self {
            coeff: Some(parse_quote! { #lexpr + #rexpr }),
            coeff_val,
            ..self
        })
    }

    /// Multiply a public Scalar [`Expr`] by a [`CIndPoint`].  val is
    /// Some(v) if the Expr is the constant v.
    pub fn mul_pub_scalar_expr(self, expr: Expr, val: Option<i128>) -> Result<Self> {
        let coeff = match self.coeff {
            None => Some(expr),
            Some(selfexpr) => {
                let pleft = paren_if_needed(selfexpr);
                let pright = paren_if_needed(expr);
                Some(parse_quote! { #pleft * #pright })
            }
        };
        let coeff_val = match (self.coeff_val, val) {
            (Some(leftval), Some(rightval)) => leftval.checked_mul(rightval),
            _ => None,
        };
        Ok(Self {
            coeff,
            coeff_val,
            ..self
        })
    }
}

/// A representation of `(a*x + b)*A` where `a` is a constant `Scalar`
/// (default to `1`), `b` is a public `Scalar` [arithmetic expression]
/// (default to `0`), `x` is a private `Scalar` variable, and `A` is a
/// computationally independent `Point`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    /// The `Scalar` expression `a*x + b`
    pub coeff: LinScalar,
    /// The public `Point` `A`
    pub id: Ident,
}

impl Term {
    /// Negate a [`Term`]
    pub fn neg(self) -> Result<Self> {
        Ok(Self {
            coeff: self.coeff.neg()?,
            ..self
        })
    }

    /// Add a [`CIndPoint`] to a [`Term`]
    pub fn add_cind(self, arg: CIndPoint) -> Result<Self> {
        if self.id != arg.id {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "public points in added CIndPoint and Term do not match",
            ));
        }
        Ok(Self {
            coeff: self.coeff.add_opt_pub_scalar_expr(arg.coeff)?,
            ..self
        })
    }

    /// Add a [`Term`] to a [`Term`]
    pub fn add_term(self, arg: Term) -> Result<Self> {
        if self.id != arg.id {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "public points in added Terms do not match",
            ));
        }
        Ok(Self {
            coeff: self.coeff.add_linscalar(arg.coeff)?,
            ..self
        })
    }

    /// Multiply a [`Term`] by a constant
    pub fn mul_const(self, arg: i128) -> Result<Self> {
        Ok(Self {
            coeff: self.coeff.mul_const(arg)?,
            ..self
        })
    }
}

/// A representation of `(a*x+b)*A + (c*r+d)*B` where `a` and `c` are a
/// constant non-zero `Scalar`s (default to `1`), `b`, and `d` are
/// public `Scalar`s or constants or combinations of those (default to
/// `0`), `x` is a private `Scalar`, `r` is a random private `Scalar`
/// that appears nowhere else in the [`StatementTree`], and `A` and `B`
/// are computationally independent public `Point`s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pedersen {
    /// The term containing the variable being committed to (`x` above)
    pub var_term: Term,
    /// The term containing the random variable (`r` above)
    pub rand_term: Term,
}

impl Pedersen {
    /// Get the `Ident` for the committed private `Scalar` in a [`Pedersen`]
    pub fn var(&self) -> Option<Ident> {
        Some(self.var_term.coeff.id.clone())
    }

    /// Negate a [`Pedersen`]
    pub fn neg(self) -> Result<Self> {
        Ok(Self {
            var_term: self.var_term.neg()?,
            rand_term: self.rand_term.neg()?,
        })
    }

    /// Add a [`CIndPoint`] to a [`Pedersen`]
    pub fn add_cind(self, arg: CIndPoint) -> Result<Self> {
        if self.var_term.id == arg.id {
            Ok(Self {
                var_term: self.var_term.add_cind(arg)?,
                ..self
            })
        } else if self.rand_term.id == arg.id {
            // This branch actually can't happen, since the private
            // random Scalar variable can only appear once in the
            // StatementTree.
            Ok(Self {
                rand_term: self.rand_term.add_cind(arg)?,
                ..self
            })
        } else {
            Err(Error::new(
                proc_macro2::Span::call_site(),
                "public points in added Pedersen and CIndPoint do not match",
            ))
        }
    }

    /// Add a [`Term`] to a [`Pedersen`]
    pub fn add_term(self, arg: Term) -> Result<Self> {
        if self.var_term.id == arg.id {
            Ok(Self {
                var_term: self.var_term.add_term(arg)?,
                ..self
            })
        } else if self.rand_term.id == arg.id {
            // This branch actually can't happen, since the private
            // random Scalar variable can only appear once in the
            // StatementTree.
            Ok(Self {
                rand_term: self.rand_term.add_term(arg)?,
                ..self
            })
        } else {
            Err(Error::new(
                proc_macro2::Span::call_site(),
                "public points in added Pedersen and CIndPoint do not match",
            ))
        }
    }

    /// Multiply a [`Pedersen`] by a constant
    pub fn mul_const(self, arg: i128) -> Result<Self> {
        Ok(Self {
            var_term: self.var_term.mul_const(arg)?,
            rand_term: self.rand_term.mul_const(arg)?,
        })
    }
}

/// Components of a Pedersen commitment
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PedersenExpr {
    PubScalarExpr(Expr),
    LinScalar(LinScalar),
    CIndPoint(CIndPoint),
    Term(Term),
    Pedersen(Pedersen),
}

/// A struct that implements [`AExprFold`] in service of [`recognize`]
struct RecognizeFold<'a> {
    /// The [`TaggedVarDict`] that maps variable names to their types
    vars: &'a TaggedVarDict,

    /// The HashSet of random variables that appear exactly once in the
    /// parent [`StatementTree`]
    randoms: &'a HashSet<String>,
}

impl<'a> AExprFold<PedersenExpr> for RecognizeFold<'a> {
    /// Called when an identifier found in the [`VarDict`] is
    /// encountered in the [`Expr`]
    fn ident(&mut self, id: &Ident, _restype: AExprType) -> Result<PedersenExpr> {
        let Some(vartype) = self.vars.get(&id.to_string()) else {
            return Err(Error::new(id.span(), "unknown identifier"));
        };
        match vartype {
            TaggedIdent::Scalar(TaggedScalar { is_pub: true, .. }) => {
                // A bare public Scalar is a simple PubScalarExpr
                Ok(PedersenExpr::PubScalarExpr(parse_quote! { #id }))
            }
            TaggedIdent::Scalar(TaggedScalar {
                is_pub: false,
                is_vec,
                ..
            }) => {
                // A bare private Scalar is a simple LinScalar
                Ok(PedersenExpr::LinScalar(LinScalar {
                    coeff: 1i128,
                    pub_scalar_expr: None,
                    id: id.clone(),
                    is_vec: *is_vec,
                }))
            }
            TaggedIdent::Point(TaggedPoint { is_cind: true, .. }) => {
                // A bare cind Point is a CIndPoint
                Ok(PedersenExpr::CIndPoint(CIndPoint {
                    coeff: None,
                    coeff_val: Some(1),
                    id: id.clone(),
                }))
            }
            TaggedIdent::Point(TaggedPoint { is_cind: false, .. }) => {
                // Not a part of a valid Pedersen expression
                Err(Error::new(id.span(), "non-cind Point"))
            }
        }
    }

    /// Called when the arithmetic expression evaluates to a constant
    /// [`i128`] value.
    fn const_i128(&mut self, restype: AExprType) -> Result<PedersenExpr> {
        let AExprType::Scalar { val: Some(val), .. } = restype else {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "BUG: it should not happen that const_i128 is called without a value",
            ));
        };
        Ok(PedersenExpr::PubScalarExpr(parse_quote! { #val }))
    }

    /// Called for unary negation
    fn neg(&mut self, arg: (AExprType, PedersenExpr), _restype: AExprType) -> Result<PedersenExpr> {
        match arg.1 {
            PedersenExpr::PubScalarExpr(expr) => {
                Ok(PedersenExpr::PubScalarExpr(parse_quote! { -#expr }))
            }
            PedersenExpr::LinScalar(linscalar) => Ok(PedersenExpr::LinScalar(linscalar.neg()?)),
            PedersenExpr::CIndPoint(cind) => Ok(PedersenExpr::CIndPoint(cind.neg()?)),
            PedersenExpr::Term(term) => Ok(PedersenExpr::Term(term.neg()?)),
            PedersenExpr::Pedersen(pedersen) => Ok(PedersenExpr::Pedersen(pedersen.neg()?)),
        }
    }

    /// Called for a parenthesized expression
    fn paren(
        &mut self,
        arg: (AExprType, PedersenExpr),
        _restype: AExprType,
    ) -> Result<PedersenExpr> {
        match arg.1 {
            PedersenExpr::PubScalarExpr(expr) => {
                Ok(PedersenExpr::PubScalarExpr(parse_quote! { (#expr) }))
            }
            _ => Ok(arg.1),
        }
    }

    /// Called when adding two `Scalar`s
    fn add_scalars(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        _restype: AExprType,
    ) -> Result<PedersenExpr> {
        match (larg.1, rarg.1) {
            // Adding two PubScalarExprs yields a PubScalarExpr
            (PedersenExpr::PubScalarExpr(lexpr), PedersenExpr::PubScalarExpr(rexpr)) => Ok(
                PedersenExpr::PubScalarExpr(parse_quote! { #lexpr + #rexpr }),
            ),
            // Adding a PubScalarExpr and a LinScalar yields a LinScalar
            (PedersenExpr::PubScalarExpr(psexpr), PedersenExpr::LinScalar(linscalar))
            | (PedersenExpr::LinScalar(linscalar), PedersenExpr::PubScalarExpr(psexpr)) => Ok(
                PedersenExpr::LinScalar(linscalar.add_opt_pub_scalar_expr(Some(psexpr))?),
            ),
            // Adding two LinScalars yields a LinScalar if they're for
            // the same private Scalar
            (PedersenExpr::LinScalar(llinscalar), PedersenExpr::LinScalar(rlinscalar)) => Ok(
                PedersenExpr::LinScalar(llinscalar.add_linscalar(rlinscalar)?),
            ),
            // Nothing else is valid
            _ => Err(Error::new(
                proc_macro2::Span::call_site(),
                "not a component of a Pedersen commitment",
            )),
        }
    }

    /// Called when adding two `Point`s
    fn add_points(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        _restype: AExprType,
    ) -> Result<PedersenExpr> {
        match (larg.1, rarg.1) {
            // Adding two CIndPoints yields a CIndPoint if they're for
            // the same public Point
            (PedersenExpr::CIndPoint(lcind), PedersenExpr::CIndPoint(rcind)) => {
                Ok(PedersenExpr::CIndPoint(lcind.add_cind(rcind)?))
            }
            // Adding a CIndPoint to a Term yields a Term if they're for
            // the same public Point
            (PedersenExpr::CIndPoint(cind), PedersenExpr::Term(term))
            | (PedersenExpr::Term(term), PedersenExpr::CIndPoint(cind)) => {
                Ok(PedersenExpr::Term(term.add_cind(cind)?))
            }
            // Adding a Term to a Term yields a Term if they're for the
            // same public Point and private Scalar, or a Pedersen if
            // they're different, but one is for a private random Scalar
            // that only appears once in the [`StatementTree`]
            (PedersenExpr::Term(lterm), PedersenExpr::Term(rterm)) => {
                if lterm.id == rterm.id {
                    Ok(PedersenExpr::Term(lterm.add_term(rterm)?))
                } else if self.randoms.contains(&rterm.coeff.id.to_string()) {
                    Ok(PedersenExpr::Pedersen(Pedersen {
                        var_term: lterm,
                        rand_term: rterm,
                    }))
                } else if self.randoms.contains(&lterm.coeff.id.to_string()) {
                    Ok(PedersenExpr::Pedersen(Pedersen {
                        var_term: rterm,
                        rand_term: lterm,
                    }))
                } else {
                    Err(Error::new(
                        proc_macro2::Span::call_site(),
                        "public points in added Terms do not form a Pedersen commitment",
                    ))
                }
            }
            // Adding a CIndPoint to a Pedersen yields a Pedersen if one
            // of the two public Points in the Pedersen matches the one
            // in the CIndPoint
            (PedersenExpr::CIndPoint(cind), PedersenExpr::Pedersen(pedersen))
            | (PedersenExpr::Pedersen(pedersen), PedersenExpr::CIndPoint(cind)) => {
                Ok(PedersenExpr::Pedersen(pedersen.add_cind(cind)?))
            }
            // Adding a Term to a Pedersen yields a Pedersen if one of
            // the two public Points in the Pedersen matches the one
            // in the Term
            (PedersenExpr::Term(term), PedersenExpr::Pedersen(pedersen))
            | (PedersenExpr::Pedersen(pedersen), PedersenExpr::Term(term)) => {
                Ok(PedersenExpr::Pedersen(pedersen.add_term(term)?))
            }

            // Note that it's impossible to add a Pedersen to a
            // Pedersen, since the private random Scalar only appears
            // once in the StatementTree.

            // Nothing else is valid
            _ => Err(Error::new(
                proc_macro2::Span::call_site(),
                "not a component of a Pedersen commitment",
            )),
        }
    }

    /// Called when subtracting two `Scalar`s
    fn sub_scalars(
        &mut self,
        (largtype, larg): (AExprType, PedersenExpr),
        (rargtype, rarg): (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        if let PedersenExpr::PubScalarExpr(ref lexpr) = larg {
            if let PedersenExpr::PubScalarExpr(ref rexpr) = rarg {
                // Subtracting two PubScalarExprs yields a PubScalarExpr
                return Ok(PedersenExpr::PubScalarExpr(
                    parse_quote! { #lexpr - #rexpr },
                ));
            }
        }
        // Anything else gets the default treatment
        let negrarg = self.neg((rargtype, rarg), rargtype)?;
        self.add_scalars((largtype, larg), (rargtype, negrarg), restype)
    }

    /// Called when subtracting two `Point`s
    fn sub_points(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        let rargtype = rarg.0;
        let negrarg = self.neg(rarg, rargtype)?;
        self.add_points(larg, (rargtype, negrarg), restype)
    }

    /// Called when multiplying two `Scalar`s
    fn mul_scalars(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        _restype: AExprType,
    ) -> Result<PedersenExpr> {
        match (larg, rarg) {
            // Multiplying two PubScalarExprs yields a PubScalarExpr
            ((_, PedersenExpr::PubScalarExpr(lexpr)), (_, PedersenExpr::PubScalarExpr(rexpr))) => {
                Ok(PedersenExpr::PubScalarExpr(
                    parse_quote! { #lexpr * #rexpr },
                ))
            }
            // Multiplying a PubScalarExpr by a LinScalar yields a
            // LinScalar if the PubScalarExpr is actually a constant
            // (indicated by the AExprType's val field containing
            // Some(val))
            (
                (
                    AExprType::Scalar {
                        val: Some(psval), ..
                    },
                    PedersenExpr::PubScalarExpr(_),
                ),
                (_, PedersenExpr::LinScalar(linscalar)),
            )
            | (
                (_, PedersenExpr::LinScalar(linscalar)),
                (
                    AExprType::Scalar {
                        val: Some(psval), ..
                    },
                    PedersenExpr::PubScalarExpr(_),
                ),
            ) => Ok(PedersenExpr::LinScalar(linscalar.mul_const(psval)?)),
            // Nothing else is valid
            _ => Err(Error::new(
                proc_macro2::Span::call_site(),
                "not a component of a Pedersen commitment",
            )),
        }
    }

    /// Called when multiplying a `Scalar` and a `Point` (the `Scalar`
    /// will always be passed as the first argument)
    fn mul_scalar_point(
        &mut self,
        sarg: (AExprType, PedersenExpr),
        parg: (AExprType, PedersenExpr),
        _restype: AExprType,
    ) -> Result<PedersenExpr> {
        match (sarg.0, sarg.1, parg.1) {
            // Multiplying a PubScalarExpr by a CIndPoint yields a
            // CIndPoint
            (
                AExprType::Scalar { val, .. },
                PedersenExpr::PubScalarExpr(pub_expr),
                PedersenExpr::CIndPoint(cind),
            ) => Ok(PedersenExpr::CIndPoint(
                cind.mul_pub_scalar_expr(pub_expr, val)?,
            )),
            // Multiplying a LinScalar by a CIndPoint yields a Term
            // if the coefficient in the CIndPoint is a constant
            (
                _,
                PedersenExpr::LinScalar(linscalar),
                PedersenExpr::CIndPoint(CIndPoint {
                    coeff_val: Some(cval),
                    id,
                    ..
                }),
            ) => Ok(PedersenExpr::Term(Term {
                coeff: linscalar.mul_const(cval)?,
                id,
            })),
            // Multiplying a PubScalarExpr by a Term yields a Term if
            // the PubScalarExpr is a constant
            (
                AExprType::Scalar {
                    val: Some(const_val),
                    ..
                },
                PedersenExpr::PubScalarExpr(_),
                PedersenExpr::Term(term),
            ) => Ok(PedersenExpr::Term(term.mul_const(const_val)?)),
            // Multiplying a PubScalarExpr by a Pedersen yields a
            // Pedersen if the PubScalarExpr is a constant
            (
                AExprType::Scalar {
                    val: Some(const_val),
                    ..
                },
                PedersenExpr::PubScalarExpr(_),
                PedersenExpr::Pedersen(pedersen),
            ) => Ok(PedersenExpr::Pedersen(pedersen.mul_const(const_val)?)),
            // Nothing else is valid
            _ => Err(Error::new(
                proc_macro2::Span::call_site(),
                "not a component of a Pedersen commitment",
            )),
        }
    }
}

/// Parse the right-hand side of the = in an [`Expr`] to see if we
/// recognize it as a Pedersen commitment
pub fn recognize(
    vars: &TaggedVarDict,
    randoms: &HashSet<String>,
    vardict: &VarDict,
    expr: &Expr,
) -> Option<Pedersen> {
    let mut fold = RecognizeFold { vars, randoms };
    let Ok((aetype, PedersenExpr::Pedersen(pedersen))) = fold.fold(vardict, expr) else {
        return None;
    };
    // It's not allowed for the overall expression to be a vector type,
    // but the randomizer variable be a non-vector
    if let Some(TaggedIdent::Scalar(TaggedScalar { is_vec: false, .. })) =
        vars.get(&pedersen.rand_term.id.to_string())
    {
        if matches!(aetype, AExprType::Point { is_vec: true, .. }) {
            return None;
        }
    }
    // It's not allowed for either the committed variable or the random
    // variable to have a 0 coefficient
    if pedersen.var_term.coeff.coeff == 0 || pedersen.rand_term.coeff.coeff == 0 {
        return None;
    }
    Some(pedersen)
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

    fn fold_tester(
        vars: (&[&str], &[&str]),
        randoms: &[&str],
        e: Expr,
        expected_out: Option<PedersenExpr>,
    ) {
        let taggedvardict = taggedvardict_from_strs(vars);
        let vardict = taggedvardict_to_vardict(&taggedvardict);
        let mut randoms_hash = HashSet::new();
        for r in randoms {
            randoms_hash.insert(r.to_string());
        }
        let mut fold = RecognizeFold {
            vars: &taggedvardict,
            randoms: &randoms_hash,
        };
        let output = if let Ok((_, pe)) = fold.fold(&vardict, &e) {
            Some(pe)
        } else {
            None
        };
        assert_eq!(output, expected_out);
    }

    #[test]
    fn fold_test() {
        let vars = (
            [
                "x", "y", "z", "pub a", "pub b", "pub c", "rand r", "rand s", "rand t",
            ]
            .as_slice(),
            ["C", "cind A", "cind B"].as_slice(),
        );
        let randoms = ["r", "s", "t"].as_slice();

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                1
            },
            Some(PedersenExpr::PubScalarExpr(parse_quote! { 1i128 })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                1 + 2
            },
            Some(PedersenExpr::PubScalarExpr(parse_quote! { 3i128 })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                1 - 2
            },
            Some(PedersenExpr::PubScalarExpr(parse_quote! { -1i128 })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a
            },
            Some(PedersenExpr::PubScalarExpr(parse_quote! { a })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a + 1
            },
            Some(PedersenExpr::PubScalarExpr(parse_quote! { a + 1i128 })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a + b + 1
            },
            Some(PedersenExpr::PubScalarExpr(parse_quote! { a + b + 1i128 })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a + 2*b + 1
            },
            Some(PedersenExpr::PubScalarExpr(
                parse_quote! { a + 2i128 * b + 1i128 },
            )),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a - 2*b + 1
            },
            Some(PedersenExpr::PubScalarExpr(
                parse_quote! { a - 2i128 * b + 1i128 },
            )),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a - (2*b + 1)
            },
            Some(PedersenExpr::PubScalarExpr(
                parse_quote! { a - (2i128 * b + 1i128) },
            )),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                x
            },
            Some(PedersenExpr::LinScalar(LinScalar {
                coeff: 1,
                pub_scalar_expr: None,
                id: parse_quote! {x},
                is_vec: false,
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                x + 2
            },
            Some(PedersenExpr::LinScalar(LinScalar {
                coeff: 1,
                pub_scalar_expr: Some(parse_quote! { 2i128 }),
                id: parse_quote! {x},
                is_vec: false,
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*x + 2
            },
            Some(PedersenExpr::LinScalar(LinScalar {
                coeff: 3,
                pub_scalar_expr: Some(parse_quote! { 2i128 }),
                id: parse_quote! {x},
                is_vec: false,
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + 2)
            },
            Some(PedersenExpr::LinScalar(LinScalar {
                coeff: 3,
                pub_scalar_expr: Some(parse_quote! { 2i128 * 3i128 }),
                id: parse_quote! {x},
                is_vec: false,
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x - 2)
            },
            Some(PedersenExpr::LinScalar(LinScalar {
                coeff: 3,
                pub_scalar_expr: Some(parse_quote! { (-2i128) * 3i128 }),
                id: parse_quote! {x},
                is_vec: false,
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + a)
            },
            Some(PedersenExpr::LinScalar(LinScalar {
                coeff: 3,
                pub_scalar_expr: Some(parse_quote! { a * 3i128 }),
                id: parse_quote! {x},
                is_vec: false,
            })),
        );

        // Adding two private Scalars
        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + y)
            },
            None,
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a * A
            },
            Some(PedersenExpr::CIndPoint(CIndPoint {
                coeff: Some(parse_quote! { a }),
                coeff_val: None,
                id: parse_quote! {A},
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                a * A * b
            },
            Some(PedersenExpr::CIndPoint(CIndPoint {
                coeff: Some(parse_quote! { a * b }),
                coeff_val: None,
                id: parse_quote! {A},
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                A * b * (c+1)
            },
            Some(PedersenExpr::CIndPoint(CIndPoint {
                coeff: Some(parse_quote! { b * (c + 1i128) }),
                coeff_val: None,
                id: parse_quote! {A},
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + a) * A
            },
            Some(PedersenExpr::Term(Term {
                coeff: LinScalar {
                    coeff: 3,
                    pub_scalar_expr: Some(parse_quote! { a * 3i128 }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
                id: parse_quote! {A},
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + a) * A + A * b
            },
            Some(PedersenExpr::Term(Term {
                coeff: LinScalar {
                    coeff: 3,
                    pub_scalar_expr: Some(parse_quote! { a * 3i128 + b }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
                id: parse_quote! {A},
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + a) * A + A * b * (c + 1)
            },
            Some(PedersenExpr::Term(Term {
                coeff: LinScalar {
                    coeff: 3,
                    pub_scalar_expr: Some(parse_quote! { a * 3i128 + (b*(c+1i128)) }),
                    id: parse_quote! {x},
                    is_vec: false,
                },
                id: parse_quote! {A},
            })),
        );

        fold_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + a) * A + A * b * (c + 1) + r * B
            },
            Some(PedersenExpr::Pedersen(Pedersen {
                var_term: Term {
                    coeff: LinScalar {
                        coeff: 3,
                        pub_scalar_expr: Some(parse_quote! { a * 3i128 + (b * (c + 1i128)) }),
                        id: parse_quote! {x},
                        is_vec: false,
                    },
                    id: parse_quote! {A},
                },
                rand_term: Term {
                    coeff: LinScalar {
                        coeff: 1,
                        pub_scalar_expr: None,
                        id: parse_quote! {r},
                        is_vec: false,
                    },
                    id: parse_quote! {B},
                },
            })),
        );
    }

    fn recognize_tester(
        vars: (&[&str], &[&str]),
        randoms: &[&str],
        e: Expr,
        expected_out: Option<Pedersen>,
    ) {
        let taggedvardict = taggedvardict_from_strs(vars);
        let vardict = taggedvardict_to_vardict(&taggedvardict);
        let mut randoms_hash = HashSet::new();
        for r in randoms {
            randoms_hash.insert(r.to_string());
        }
        let output = recognize(&taggedvardict, &randoms_hash, &vardict, &e);
        assert_eq!(output, expected_out);
    }

    #[test]
    fn recognize_test() {
        let vars = (
            [
                "x", "y", "z", "pub a", "pub b", "pub c", "rand r", "rand s", "rand t",
            ]
            .as_slice(),
            ["C", "cind A", "cind B"].as_slice(),
        );
        let randoms = ["r", "s", "t"].as_slice();

        recognize_tester(
            vars,
            randoms,
            parse_quote! {
                3*(x + a) * A + A * b * (c + 1) + r * B
            },
            Some(Pedersen {
                var_term: Term {
                    coeff: LinScalar {
                        coeff: 3,
                        pub_scalar_expr: Some(parse_quote! { a * 3i128 + (b * (c + 1i128)) }),
                        id: parse_quote! {x},
                        is_vec: false,
                    },
                    id: parse_quote! {A},
                },
                rand_term: Term {
                    coeff: LinScalar {
                        coeff: 1,
                        pub_scalar_expr: None,
                        id: parse_quote! {r},
                        is_vec: false,
                    },
                    id: parse_quote! {B},
                },
            }),
        );

        recognize_tester(
            vars,
            randoms,
            parse_quote! {
                3 * (2*x + a*b) * A + B * (3 * r - 7)
            },
            Some(Pedersen {
                var_term: Term {
                    coeff: LinScalar {
                        coeff: 6,
                        pub_scalar_expr: Some(parse_quote! { (a * b) * 3i128 }),
                        id: parse_quote! {x},
                        is_vec: false,
                    },
                    id: parse_quote! {A},
                },
                rand_term: Term {
                    coeff: LinScalar {
                        coeff: 3,
                        pub_scalar_expr: Some(parse_quote! { -7i128 }),
                        id: parse_quote! {r},
                        is_vec: false,
                    },
                    id: parse_quote! {B},
                },
            }),
        );

        recognize_tester(
            vars,
            randoms,
            parse_quote! {
                (3 * (2*x + a*b) * A + B * (3 * r - 7))* (7-2)
            },
            Some(Pedersen {
                var_term: Term {
                    coeff: LinScalar {
                        coeff: 30,
                        pub_scalar_expr: Some(parse_quote! { ((a * b) * 3i128) * 5i128 }),
                        id: parse_quote! {x},
                        is_vec: false,
                    },
                    id: parse_quote! {A},
                },
                rand_term: Term {
                    coeff: LinScalar {
                        coeff: 15,
                        pub_scalar_expr: Some(parse_quote! { (-7i128) * 5i128 }),
                        id: parse_quote! {r},
                        is_vec: false,
                    },
                    id: parse_quote! {B},
                },
            }),
        );
    }
}

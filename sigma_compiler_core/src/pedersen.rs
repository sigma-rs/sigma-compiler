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
/// `Scalar` variable
///
/// [arithmetic expression]: expr_type
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

/// A representation of `(a*x + b)*A` where `a` is a constant `Scalar`,
/// `b` is a public `Scalar` [arithmetic expression], `x` is a private
/// `Scalar` variable, and `A` is a computationally independent `Point`
pub struct Term {
    /// The `Scalar` expression `a*x + b`
    pub coeff: LinScalar,
    /// The public `Point` `A`
    pub id: Ident,
}

/// A representation of `(a*x+b)*A + (c*r+d)*B` where `a` and `c` are a
/// constant non-zero `Scalar`s, `b`, and `d` are public `Scalar`s or
/// constants (or combinations of those), `r` is a random private
/// `Scalar` that appears nowhere else in the [`StatementTree`], and `A`
/// and `B` are computationally independent public `Point`s.
pub struct Pedersen {
    /// The term containing the variable being committed to (`x` above)
    pub var_term: Term,
    /// The term containing the random variable (`r` above)
    pub rand_term: Term,
}

/// Get the `Ident` for the committed private `Scalar` in a [`Pedersen`]
impl Pedersen {
    pub fn var(&self) -> Option<Ident> {
        Some(self.var_term.coeff.id.clone())
    }
}

/// Components of a Pedersen commitment
pub enum PedersenExpr {
    PubScalarExpr(Expr),
    LinScalar(LinScalar),
    CIndPoint(Ident),
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
                Ok(PedersenExpr::CIndPoint(id.clone()))
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
    fn neg(&mut self, arg: (AExprType, PedersenExpr), restype: AExprType) -> Result<PedersenExpr> {
        Ok(arg.1)
    }

    /// Called for a parenthesized expression
    fn paren(
        &mut self,
        arg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        Ok(arg.1)
    }

    /// Called when adding two `Scalar`s
    fn add_scalars(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        Ok(larg.1)
    }

    /// Called when adding two `Point`s
    fn add_points(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        Ok(larg.1)
    }

    /// Called when subtracting two `Scalar`s
    fn sub_scalars(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        Ok(larg.1)
    }

    /// Called when subtracting two `Point`s
    fn sub_points(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        Ok(larg.1)
    }

    /// Called when multiplying two `Scalar`s
    fn mul_scalars(
        &mut self,
        larg: (AExprType, PedersenExpr),
        rarg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        Ok(larg.1)
    }

    /// Called when multiplying a `Scalar` and a `Point` (the `Scalar`
    /// will always be passed as the first argument)
    fn mul_scalar_point(
        &mut self,
        sarg: (AExprType, PedersenExpr),
        parg: (AExprType, PedersenExpr),
        restype: AExprType,
    ) -> Result<PedersenExpr> {
        Ok(sarg.1)
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
}

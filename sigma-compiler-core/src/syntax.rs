//! A module for parsing the syntax of the macro

use super::sigma::combiners::StatementTree;
use super::sigma::types::*;
use quote::format_ident;
use std::collections::HashMap;
use std::fmt;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{parenthesized, Error, Expr, Ident, Token};

/// A [`TaggedScalar`] is an [`struct@Ident`] representing a `Scalar`,
/// preceded by zero or more of the following tags: `pub`, `rand`, `vec`
///
/// The following combinations are valid:
///  - (nothing)
///  - `pub`
///  - `rand`
///  - `vec`
///  - `pub vec`
///  - `rand vec`

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaggedScalar {
    pub id: Ident,
    pub is_pub: bool,
    pub is_rand: bool,
    pub is_vec: bool,
}

impl Parse for TaggedScalar {
    fn parse(input: ParseStream) -> Result<Self> {
        let (mut is_pub, mut is_rand, mut is_vec) = (false, false, false);
        loop {
            let id = input.call(Ident::parse_any)?;
            match id.to_string().as_str() {
                // pub and rand are mutually exclusive
                "pub" if !is_rand => {
                    is_pub = true;
                }
                "rand" if !is_pub => {
                    is_rand = true;
                }
                // any other use of the tagging keywords is not allowed
                "pub" | "rand" | "cind" | "const" => {
                    return Err(Error::new(id.span(), "tag not allowed in this position"));
                }
                // vec is allowed with any other tag
                "vec" => {
                    is_vec = true;
                }
                _ => {
                    return Ok(TaggedScalar {
                        id,
                        is_pub,
                        is_rand,
                        is_vec,
                    });
                }
            }
        }
    }
}

impl fmt::Display for TaggedScalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = String::new();
        if self.is_pub {
            res += "pub ";
        }
        if self.is_rand {
            res += "rand ";
        }
        if self.is_vec {
            res += "vec ";
        }
        res += &self.id.to_string();

        write!(f, "{res}")
    }
}

/// A [`TaggedPoint`] is an [`struct@Ident`] representing a `Point`,
/// preceded by zero or more of the following tags: `cind`, `const`,
/// `vec`
///
/// All combinations are valid:
///  - (nothing)
///  - `cind`
///  - `const`
///  - `cind const`
///  - `vec`
///  - `cind vec`
///  - `const vec`
///  - `cind const vec`

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaggedPoint {
    pub id: Ident,
    pub is_cind: bool,
    pub is_const: bool,
    pub is_vec: bool,
}

impl Parse for TaggedPoint {
    fn parse(input: ParseStream) -> Result<Self> {
        // Points are always pub
        let (mut is_cind, mut is_const, mut is_vec) = (false, false, false);
        loop {
            let id = input.call(Ident::parse_any)?;
            match id.to_string().as_str() {
                "cind" => {
                    is_cind = true;
                }
                "const" => {
                    is_const = true;
                }
                // any other use of the tagging keywords is not allowed
                "pub" | "rand" => {
                    return Err(Error::new(id.span(), "tag not allowed in this position"));
                }
                "vec" => {
                    is_vec = true;
                }
                _ => {
                    return Ok(TaggedPoint {
                        id,
                        is_cind,
                        is_const,
                        is_vec,
                    });
                }
            }
        }
    }
}

impl fmt::Display for TaggedPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut res = String::new();
        if self.is_vec {
            res += "vec ";
        }
        if self.is_const {
            res += "const ";
        }
        if self.is_cind {
            res += "cind ";
        }
        res += &self.id.to_string();

        write!(f, "{res}")
    }
}

/// A [`TaggedIdent`] can be either a [`TaggedScalar`] or a
/// [`TaggedPoint`]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaggedIdent {
    Scalar(TaggedScalar),
    Point(TaggedPoint),
}

/// Convert a [`TaggedIdent`] to its underlying [`AExprType`]
impl From<&TaggedIdent> for AExprType {
    fn from(ti: &TaggedIdent) -> Self {
        match ti {
            TaggedIdent::Scalar(ts) => Self::Scalar {
                is_pub: ts.is_pub,
                is_vec: ts.is_vec,
                val: None,
            },
            TaggedIdent::Point(tp) => Self::Point {
                is_pub: true,
                is_vec: tp.is_vec,
            },
        }
    }
}

/// A [`TaggedVarDict`] is a dictionary of the available variables,
/// mapping the string version of [`struct@Ident`]s to [`TaggedIdent`],
/// which includes their type ([`Scalar`](TaggedIdent::Scalar) or
/// [`Point`](TaggedIdent::Point))
pub type TaggedVarDict = HashMap<String, TaggedIdent>;

/// Convert a [`TaggedVarDict`] (a map from [`String`] to
/// [`TaggedIdent`]) into the equivalent [`VarDict`] (a map from
/// [`String`] to [`AExprType`])
pub fn taggedvardict_to_vardict(vd: &TaggedVarDict) -> VarDict {
    vd.iter()
        .map(|(k, v)| (k.clone(), AExprType::from(v)))
        .collect()
}

/// Collect the list of [`Point`](TaggedIdent::Point)s tagged `cind`
/// from the given [`TaggedVarDict`]
pub fn collect_cind_points(vars: &TaggedVarDict) -> Vec<Ident> {
    let mut cind_points: Vec<Ident> = vars
        .values()
        .filter_map(|ti| {
            if let TaggedIdent::Point(TaggedPoint {
                is_cind: true,
                is_vec: false,
                id,
                ..
            }) = ti
            {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();
    cind_points.sort();
    cind_points
}

#[cfg(test)]
/// Convert a list of strings describing `Scalar`s and a list of strings
/// describing `Point`s into a [`TaggedVarDict`]
pub fn taggedvardict_from_strs((scalar_strs, point_strs): (&[&str], &[&str])) -> TaggedVarDict {
    let mut vars = HashMap::new();

    for scalar in scalar_strs {
        let ts: TaggedScalar = syn::parse_str(scalar).unwrap();
        vars.insert(ts.id.to_string(), TaggedIdent::Scalar(ts));
    }
    for point in point_strs {
        let tp: TaggedPoint = syn::parse_str(point).unwrap();
        vars.insert(tp.id.to_string(), TaggedIdent::Point(tp));
    }
    vars
}

/// The [`SigmaCompSpec`] struct is the result of parsing the macro
/// input.
#[derive(Debug)]
pub struct SigmaCompSpec {
    /// An identifier for the name of the zero-knowledge protocol being
    /// defined
    pub proto_name: Ident,

    /// An identifier for the mathematical
    /// [`PrimeGroup`](https://docs.rs/group/0.13.0/group/prime/trait.PrimeGroup.html)
    /// being used (if none is specified, it is assumed there is a
    /// default type called `G` in scope that implements the
    /// [`PrimeGroup`](https://docs.rs/group/0.13.0/group/prime/trait.PrimeGroup.html)
    /// trait)
    pub group_name: Ident,

    /// A [`TaggedVarDict`] mapping variable names to their types
    /// (`Scalar` or `Point`) and tags (e.g., `rand`, `pub`, `vec`,
    /// `cind`, `const`)
    pub vars: TaggedVarDict,

    /// A [`StatementTree`] representing the statements provided in the
    /// macro invocation that are to be proved true in zero knowledge
    pub statements: StatementTree,
}

// T is TaggedScalar or TaggedPoint
fn paren_taggedidents<T: Parse>(input: ParseStream) -> Result<Vec<T>> {
    let content;
    parenthesized!(content in input);
    let punc: Punctuated<T, Token![,]> = content.parse_terminated(T::parse, Token![,])?;
    Ok(punc.into_iter().collect())
}

impl Parse for SigmaCompSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let proto_name: Ident = input.parse()?;
        // See if a group was specified
        let group_name = if input.peek(Token![<]) {
            input.parse::<Token![<]>()?;
            let gr: Ident = input.parse()?;
            input.parse::<Token![>]>()?;
            gr
        } else {
            format_ident!("G")
        };
        input.parse::<Token![,]>()?;

        let mut vars: TaggedVarDict = HashMap::new();

        let scalars = paren_taggedidents::<TaggedScalar>(input)?;
        vars.extend(
            scalars
                .into_iter()
                .map(|ts| (ts.id.to_string(), TaggedIdent::Scalar(ts))),
        );
        input.parse::<Token![,]>()?;

        let points = paren_taggedidents::<TaggedPoint>(input)?;
        vars.extend(
            points
                .into_iter()
                .map(|tp| (tp.id.to_string(), TaggedIdent::Point(tp))),
        );
        input.parse::<Token![,]>()?;

        let statementpunc: Punctuated<Expr, Token![,]> =
            input.parse_terminated(Expr::parse, Token![,])?;
        let statementlist: Vec<Expr> = statementpunc.into_iter().collect();
        let statements = StatementTree::parse_andlist(&statementlist)?;

        Ok(SigmaCompSpec {
            proto_name,
            group_name,
            vars,
            statements,
        })
    }
}

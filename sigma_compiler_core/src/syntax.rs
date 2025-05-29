use super::sigma::combiners::StatementTree;
use quote::format_ident;
use std::collections::HashMap;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{parenthesized, Error, Expr, Ident, Token};

/// A `TaggedScalar` is an `Ident` representing a `Scalar`, preceded by
/// zero or more of the following tags: `pub`, `rand`, `vec`
///
/// The following combinations are valid:
///  - (nothing)
///  - `pub`
///  - `rand`
///  - `vec`
///  - `pub vec`
///  - `rand vec`

#[derive(Debug)]
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

/// A `TaggedPoint` is an `Ident` representing a `Point`, preceded by
/// zero or more of the following tags: `cind`, `const`, `vec`
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

#[derive(Debug)]
pub struct TaggedPoint {
    pub id: Ident,
    pub is_cind: bool,
    pub is_const: bool,
    pub is_vec: bool,
}

/// A `TaggedIdent` can be either a `TaggedScalar` or a `TaggedPoint`
#[derive(Debug)]
pub enum TaggedIdent {
    Scalar(TaggedScalar),
    Point(TaggedPoint),
}

/// A `VarDict` is a dictionary of the available variables, mapping
/// the string version of `Ident`s to `TaggedIdent`, which includes
/// their type (`Scalar` or `Point`)
pub type VarDict = HashMap<String, TaggedIdent>;

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

#[derive(Debug)]
pub struct SigmaCompSpec {
    pub proto_name: Ident,
    pub group_name: Ident,
    pub vars: VarDict,
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

        let mut vars: VarDict = HashMap::new();

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

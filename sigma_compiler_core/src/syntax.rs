use quote::format_ident;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{parenthesized, Expr, Ident, Token};

/// A `TaggedIdent` is an `Ident`, preceded by zero or more of the
/// following tags: `pub`, `rand`, `cind`, `const`, `vec`
///
/// A `TaggedIndent` representing a `Scalar` can be preceded by:
///  - (nothing)
///  - `pub`
///  - `rand`
///  - `vec`
///  - `pub vec`
///  - `rand vec`
///
/// A `TaggedIndent` representing a `Point` can be preceded by:
///  - (nothing)
///  - `cind`
///  - `const`
///  - `cind const`
///  - `vec`
///  - `cind vec`
///  - `const vec`
///  - `cind const vec`

#[derive(Debug)]
pub struct TaggedIdent {
    pub id: Ident,
    pub is_pub: bool,
    pub is_rand: bool,
    pub is_cind: bool,
    pub is_const: bool,
    pub is_vec: bool,
}

impl TaggedIdent {
    // parse for a `Scalar` if point is false; parse for a `Point` if point
    // is true
    pub fn parse(input: ParseStream, point: bool) -> Result<Self> {
        // Points are always pub
        let (mut is_pub, mut is_rand, mut is_cind, mut is_const, mut is_vec) =
            (point, false, false, false, false);
        loop {
            let id = input.call(Ident::parse_any)?;
            match id.to_string().as_str() {
                // pub and rand are only allowed for Scalars, and are
                // mutually exclusive
                "pub" if !point && !is_rand => {
                    is_pub = true;
                }
                "rand" if !point && !is_pub => {
                    is_rand = true;
                }
                // cind and const are only allowed for Points, but can
                // be used together
                "cind" if point => {
                    is_cind = true;
                }
                "const" if point => {
                    is_const = true;
                }
                // vec is allowed with either Scalars or Points, and
                // with any other tag
                "vec" => {
                    is_vec = true;
                }
                _ => {
                    return Ok(TaggedIdent {
                        id,
                        is_pub,
                        is_rand,
                        is_cind,
                        is_const,
                        is_vec,
                    });
                }
            }
        }
    }

    // Parse a `TaggedIndent` using the tags allowed for a `Scalar`
    pub fn parse_scalar(input: ParseStream) -> Result<Self> {
        Self::parse(input, false)
    }

    // Parse a `TaggedIndent` using the tags allowed for a `Point`
    pub fn parse_point(input: ParseStream) -> Result<Self> {
        Self::parse(input, true)
    }
}

#[derive(Debug)]
pub struct SigmaCompSpec {
    pub proto_name: Ident,
    pub group_name: Ident,
    pub scalars: Vec<TaggedIdent>,
    pub points: Vec<TaggedIdent>,
    pub statements: Vec<Expr>,
}

// parse for a `Scalar` if point is false; parse for a `Point` if point
// is true
fn paren_taggedidents(input: ParseStream, point: bool) -> Result<Vec<TaggedIdent>> {
    let content;
    parenthesized!(content in input);
    let punc: Punctuated<TaggedIdent, Token![,]> = content.parse_terminated(
        if point {
            TaggedIdent::parse_point
        } else {
            TaggedIdent::parse_scalar
        },
        Token![,],
    )?;
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

        let scalars = paren_taggedidents(input, false)?;
        input.parse::<Token![,]>()?;

        let points = paren_taggedidents(input, true)?;
        input.parse::<Token![,]>()?;

        let statementpunc: Punctuated<Expr, Token![,]> =
            input.parse_terminated(Expr::parse, Token![,])?;
        let statements: Vec<Expr> = statementpunc.into_iter().collect();

        Ok(SigmaCompSpec {
            proto_name,
            group_name,
            scalars,
            points,
            statements,
        })
    }
}

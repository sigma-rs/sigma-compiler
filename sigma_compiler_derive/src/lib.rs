use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{parenthesized, parse_macro_input, Expr, Ident, Token};

// Either an Ident or "vec(Ident)"
#[derive(Debug)]
enum VecIdent {
    Ident(Ident),
    VecIdent(Ident),
}

impl Parse for VecIdent {
    fn parse(input: ParseStream) -> Result<Self> {
        let id: Ident = input.parse()?;
        if id.to_string() == "vec" {
            let content;
            parenthesized!(content in input);
            let vid: Ident = content.parse()?;
            Ok(Self::VecIdent(vid))
        } else {
            Ok(Self::Ident(id))
        }
    }
}

#[derive(Debug)]
struct SigmaCompSpec {
    proto_name: Ident,
    group_name: Ident,
    rand_scalars: Vec<VecIdent>,
    priv_scalars: Vec<VecIdent>,
    pub_scalars: Vec<VecIdent>,
    cind_points: Vec<VecIdent>,
    pub_points: Vec<VecIdent>,
    const_points: Vec<VecIdent>,
    statements: Vec<Expr>,
}

fn paren_vecidents(input: ParseStream) -> Result<Vec<VecIdent>> {
    let content;
    parenthesized!(content in input);
    let punc: Punctuated<VecIdent, Token![,]> =
        content.parse_terminated(VecIdent::parse, Token![,])?;
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

        let rand_scalars = paren_vecidents(input)?;
        input.parse::<Token![,]>()?;

        let priv_scalars = paren_vecidents(input)?;
        input.parse::<Token![,]>()?;

        let pub_scalars = paren_vecidents(input)?;
        input.parse::<Token![,]>()?;

        let cind_points = paren_vecidents(input)?;
        input.parse::<Token![,]>()?;

        let pub_points = paren_vecidents(input)?;
        input.parse::<Token![,]>()?;

        let const_points = paren_vecidents(input)?;
        input.parse::<Token![,]>()?;

        let statementpunc: Punctuated<Expr, Token![,]> =
            input.parse_terminated(Expr::parse, Token![,])?;
        let statements: Vec<Expr> = statementpunc.into_iter().collect();

        Ok(SigmaCompSpec {
            proto_name,
            group_name,
            rand_scalars,
            priv_scalars,
            pub_scalars,
            cind_points,
            pub_points,
            const_points,
            statements,
        })
    }
}

// Names and types of fields that might end up in a generated struct
enum StructField {
    Scalar(Ident),
    VecScalar(Ident),
    Point(Ident),
    VecPoint(Ident),
}

// A list of StructField items
#[derive(Default)]
struct StructFieldList {
    fields: Vec<StructField>,
}

impl StructFieldList {
    pub fn push_scalar(&mut self, s: &Ident) {
        self.fields.push(StructField::Scalar(s.clone()));
    }
    pub fn push_vecscalar(&mut self, s: &Ident) {
        self.fields.push(StructField::VecScalar(s.clone()));
    }
    pub fn push_point(&mut self, s: &Ident) {
        self.fields.push(StructField::Point(s.clone()));
    }
    pub fn push_vecpoint(&mut self, s: &Ident) {
        self.fields.push(StructField::VecPoint(s.clone()));
    }
    pub fn push_scalars(&mut self, sl: &[VecIdent]) {
        for vi in sl.iter() {
            match vi {
                VecIdent::Ident(id) => self.push_scalar(id),
                VecIdent::VecIdent(id) => self.push_vecscalar(id),
            }
        }
    }
    pub fn push_points(&mut self, sl: &[VecIdent]) {
        for vi in sl.iter() {
            match vi {
                VecIdent::Ident(id) => self.push_point(id),
                VecIdent::VecIdent(id) => self.push_vecpoint(id),
            }
        }
    }
    /// Output a ToTokens of the fields as they would appear in a struct
    /// definition
    pub fn field_decls(&self) -> impl ToTokens {
        let decls = self.fields.iter().map(|f| match f {
            StructField::Scalar(id) => quote! {
                pub #id: Scalar,
            },
            StructField::VecScalar(id) => quote! {
                pub #id: Vec<Scalar>,
            },
            StructField::Point(id) => quote! {
                pub #id: Point,
            },
            StructField::VecPoint(id) => quote! {
                pub #id: Vec<Point>,
            },
        });
        quote! { #(#decls)* }
    }
}

fn sigma_compiler_impl(
    spec: &SigmaCompSpec,
    emit_prover: bool,
    emit_verifier: bool,
) -> TokenStream {
    let proto_name = &spec.proto_name;
    let group_name = &spec.group_name;

    let group_types = quote! {
        pub type Scalar = <super::#group_name as Group>::Scalar;
        pub type Point = super::#group_name;
    };

    // Generate the public params struct definition
    let params_def = {
        let mut pub_params_fields = StructFieldList::default();
        pub_params_fields.push_points(&spec.const_points);
        pub_params_fields.push_points(&spec.cind_points);
        pub_params_fields.push_points(&spec.pub_points);
        pub_params_fields.push_scalars(&spec.pub_scalars);

        let decls = pub_params_fields.field_decls();
        quote! {
            pub struct Params {
                #decls
            }
        }
    };

    // Generate the witness struct definition
    let witness_def = if emit_prover {
        let mut witness_fields = StructFieldList::default();
        witness_fields.push_scalars(&spec.rand_scalars);
        witness_fields.push_scalars(&spec.priv_scalars);

        let decls = witness_fields.field_decls();
        quote! {
            pub struct Witness {
                #decls
            }
        }
    } else {
        quote! {}
    };

    // Generate the (currently dummy) prove function
    let prove_func = if emit_prover {
        quote! {
            pub fn prove(params: &Params, witness: &Witness) -> Result<Vec<u8>,()> {
                Ok(Vec::<u8>::default())
            }
        }
    } else {
        quote! {}
    };

    // Generate the (currently dummy) verify function
    let verify_func = if emit_verifier {
        quote! {
            pub fn verify(params: &Params, proof: &[u8]) -> Result<(),()> {
                Ok(())
            }
        }
    } else {
        quote! {}
    };

    // Output the generated module for this protocol
    quote! {
        #[allow(non_snake_case)]
        pub mod #proto_name {
            use super::*;

            #group_types
            #params_def
            #witness_def

            #prove_func
            #verify_func
        }
    }
    .into()
}

#[proc_macro]
pub fn sigma_compiler(input: TokenStream) -> TokenStream {
    let spec: SigmaCompSpec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_impl(&spec, true, true)
}

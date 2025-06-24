//! A module for generating the code that uses the `sigma-rs` crate API.
//!
//! If that crate gets its own macro interface, it can use this module
//! directly.

use quote::{quote, ToTokens};
use syn::Ident;

// Names and types of fields that might end up in a generated struct
enum StructField {
    Scalar(Ident),
    VecScalar(Ident),
    Point(Ident),
    VecPoint(Ident),
}

// A list of StructField items
#[derive(Default)]
pub struct StructFieldList {
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
    #[cfg(feature = "dump")]
    /// Output a ToTokens of the contents of the fields
    pub fn dump(&self) -> impl ToTokens {
        let dump_chunks = self.fields.iter().map(|f| match f {
            StructField::Scalar(id) => quote! {
                print!("  {}: ", stringify!(#id));
                Params::dump_scalar(&self.#id);
                println!("");
            },
            StructField::VecScalar(id) => quote! {
                print!("  {}: [", stringify!(#id));
                for s in self.#id.iter() {
                    print!("    ");
                    Params::dump_scalar(s);
                    println!(",");
                }
                println!("  ]");
            },
            StructField::Point(id) => quote! {
                print!("  {}: ", stringify!(#id));
                Params::dump_point(&self.#id);
                println!("");
            },
            StructField::VecPoint(id) => quote! {
                print!("  {}: [", stringify!(#id));
                for p in self.#id.iter() {
                    print!("    ");
                    Params::dump_point(p);
                    println!(",");
                }
                println!("  ]");
            },
        });
        quote! { #(#dump_chunks)* }
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
    /// Output a ToTokens of the list of fields
    pub fn field_list(&self) -> impl ToTokens {
        let field_ids = self.fields.iter().map(|f| match f {
            StructField::Scalar(id) => quote! {
                #id,
            },
            StructField::VecScalar(id) => quote! {
                #id,
            },
            StructField::Point(id) => quote! {
                #id,
            },
            StructField::VecPoint(id) => quote! {
                #id,
            },
        });
        quote! { #(#field_ids)* }
    }
}

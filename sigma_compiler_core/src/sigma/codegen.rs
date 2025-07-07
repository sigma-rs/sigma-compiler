//! A module for generating the code that uses the `sigma-rs` crate API.
//!
//! If that crate gets its own macro interface, it can use this module
//! directly.

use super::combiners::StatementTree;
use super::types::{AExprType, VarDict};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::Ident;

/// Names and types of fields that might end up in a generated struct
pub enum StructField {
    Scalar(Ident),
    VecScalar(Ident),
    Point(Ident),
    VecPoint(Ident),
}

/// A list of StructField items
#[derive(Default)]
pub struct StructFieldList {
    pub fields: Vec<StructField>,
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
    pub fn push_vars(&mut self, vars: &VarDict, for_params: bool) {
        for (id, ti) in vars.iter() {
            match ti {
                AExprType::Scalar { is_pub, is_vec, .. } => {
                    if *is_pub == for_params {
                        if *is_vec {
                            self.push_vecscalar(&format_ident!("{}", id))
                        } else {
                            self.push_scalar(&format_ident!("{}", id))
                        }
                    }
                }
                AExprType::Point { is_vec, .. } => {
                    if for_params {
                        if *is_vec {
                            self.push_vecpoint(&format_ident!("{}", id))
                        } else {
                            self.push_point(&format_ident!("{}", id))
                        }
                    }
                }
            }
        }
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

/// The main struct to handle code generation using the `sigma-rs` API.
pub struct CodeGen<'a> {
    proto_name: Ident,
    group_name: Ident,
    vars: &'a VarDict,
    statements: &'a StatementTree,
}

impl<'a> CodeGen<'a> {
    pub fn new(
        proto_name: Ident,
        group_name: Ident,
        vars: &'a VarDict,
        statements: &'a StatementTree,
    ) -> Self {
        Self {
            proto_name,
            group_name,
            vars,
            statements,
        }
    }

    /// Generate the code that uses the `sigma-rs` API to prove and
    /// verify the statements in the [`CodeGen`].
    ///
    /// `emit_prover` and `emit_verifier` are as in
    /// [`sigma_compiler_core`](super::super::sigma_compiler_core).
    pub fn generate(&self, emit_prover: bool, emit_verifier: bool) -> TokenStream {
        let proto_name = &self.proto_name;
        let group_name = &self.group_name;

        let group_types = quote! {
            use super::group;
            pub type Scalar = <super::#group_name as group::Group>::Scalar;
            pub type Point = super::#group_name;
        };

        let mut pub_params_fields = StructFieldList::default();
        pub_params_fields.push_vars(self.vars, true);

        // Generate the public params struct definition
        let params_def = {
            let decls = pub_params_fields.field_decls();
            #[cfg(feature = "dump")]
            let dump_impl = {
                let dump_chunks = pub_params_fields.dump();
                quote! {
                    impl Params {
                        fn dump_scalar(s: &Scalar) {
                            let bytes: &[u8] = &s.to_repr();
                            print!("{:02x?}", bytes);
                        }

                        fn dump_point(p: &Point) {
                            let bytes: &[u8] = &p.to_bytes();
                            print!("{:02x?}", bytes);
                        }

                        pub fn dump(&self) {
                            #dump_chunks
                        }
                    }
                }
            };
            #[cfg(not(feature = "dump"))]
            let dump_impl = {
                quote! {}
            };
            quote! {
                #[derive(Clone)]
                pub struct Params {
                    #decls
                }

                #dump_impl
            }
        };

        let mut witness_fields = StructFieldList::default();
        witness_fields.push_vars(self.vars, false);

        // Generate the witness struct definition
        let witness_def = if emit_prover {
            let decls = witness_fields.field_decls();
            quote! {
                #[derive(Clone)]
                pub struct Witness {
                    #decls
                }
            }
        } else {
            quote! {}
        };

        // Generate the (currently dummy) prove function
        let prove_func = if emit_prover {
            let dumper = if cfg!(feature = "dump") {
                quote! {
                    println!("prover params = {{");
                    params.dump();
                    println!("}}");
                }
            } else {
                quote! {}
            };
            let params_ids = pub_params_fields.field_list();
            let witness_ids = witness_fields.field_list();

            quote! {
                pub fn prove(
                    params: &Params,
                    witness: &Witness,
                    session_id: &[u8],
                    rng: &mut (impl CryptoRng + RngCore),
                ) -> Result<Vec<u8>, SigmaError> {
                    #dumper
                    let Params { #params_ids } = params.clone();
                    let Witness { #witness_ids } = witness.clone();
                    Ok(Vec::<u8>::default())
                }
            }
        } else {
            quote! {}
        };

        // Generate the (currently dummy) verify function
        let verify_func = if emit_verifier {
            let dumper = if cfg!(feature = "dump") {
                quote! {
                    println!("verifier params = {{");
                    params.dump();
                    println!("}}");
                }
            } else {
                quote! {}
            };
            let params_ids = pub_params_fields.field_list();
            quote! {
                pub fn verify(
                    params: &Params,
                    proof: &[u8],
                    session_id: &[u8],
                ) -> Result<(), SigmaError> {
                    #dumper
                    let Params { #params_ids } = params.clone();
                    Ok(())
                }
            }
        } else {
            quote! {}
        };

        // Output the generated module for this protocol
        let dump_use = if cfg!(feature = "dump") {
            quote! {
                use group::GroupEncoding;
            }
        } else {
            quote! {}
        };
        quote! {
            #[allow(non_snake_case)]
            pub mod #proto_name {
                use sigma_compiler::rand::{CryptoRng, RngCore};
                use sigma_compiler::group::ff::PrimeField;
                use sigma_compiler::sigma_rs::errors::Error as SigmaError;
                #dump_use

                #group_types
                #params_def
                #witness_def

                #prove_func
                #verify_func
            }
        }
    }
}

//! A module for generating the code produced by this macro.  This code
//! will interact with the underlying `sigma` macro.

use super::syntax::*;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
#[cfg(test)]
use syn::parse_quote;
use syn::visit_mut::{self, VisitMut};
use syn::{Expr, Ident, Token};

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
    pub fn push_vars(&mut self, vardict: &TaggedVarDict, is_pub: bool) {
        for (_, ti) in vardict.iter() {
            match ti {
                TaggedIdent::Scalar(st) => {
                    if st.is_pub == is_pub {
                        if st.is_vec {
                            self.push_vecscalar(&st.id)
                        } else {
                            self.push_scalar(&st.id)
                        }
                    }
                }
                TaggedIdent::Point(pt) => {
                    if is_pub {
                        if pt.is_vec {
                            self.push_vecpoint(&pt.id)
                        } else {
                            self.push_point(&pt.id)
                        }
                    }
                }
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

/// An implementation of the
/// [`VisitMut`](https://docs.rs/syn/latest/syn/visit_mut/trait.VisitMut.html)
/// trait that massages the provided statements.
///
/// This massaging currently consists of:
///   - Changing equality from = to ==
struct StatementFixup {}

impl VisitMut for StatementFixup {
    fn visit_expr_mut(&mut self, node: &mut Expr) {
        if let Expr::Assign(assn) = node {
            *node = Expr::Binary(syn::ExprBinary {
                attrs: assn.attrs.clone(),
                left: assn.left.clone(),
                op: syn::BinOp::Eq(Token![==](assn.eq_token.span)),
                right: assn.right.clone(),
            });
        }
        // Unless we bailed out above, continue with the default
        // traversal
        visit_mut::visit_expr_mut(self, node);
    }
}

/// The main struct to handle code generation for this macro.
///
/// Initialize a [`CodeGen`] with the [`SigmaCompSpec`] you get by
/// parsing the macro input.  Pass it to the various transformations and
/// statement handlers, which will both update the code it will
/// generate, and modify the [`SigmaCompSpec`].  Then at the end, call
/// [`CodeGen::generate`] with the modified [`SigmaCompSpec`] to generate the
/// code output by this macro.
pub struct CodeGen {
    proto_name: Ident,
    group_name: Ident,
    vars: TaggedVarDict,
    prove_code: TokenStream,
}

impl CodeGen {
    /// Create a new [`CodeGen`] given the [`SigmaCompSpec`] you get by
    /// parsing the macro input.
    pub fn new(spec: &SigmaCompSpec) -> Self {
        Self {
            proto_name: spec.proto_name.clone(),
            group_name: spec.group_name.clone(),
            vars: spec.vars.clone(),
            prove_code: quote! {},
        }
    }

    #[cfg(test)]
    /// Create an empty [`CodeGen`].  Primarily useful in testing.
    pub fn new_empty() -> Self {
        Self {
            proto_name: parse_quote! { proto },
            group_name: parse_quote! { G },
            vars: TaggedVarDict::default(),
            prove_code: quote! {},
        }
    }

    /// Append some code to the generated `prove` function
    pub fn prove_append(&mut self, code: TokenStream) {
        let prove_code = &self.prove_code;
        self.prove_code = quote! {
            #prove_code
            #code
        };
    }

    /// Generate the code to be output by this macro.
    ///
    /// `emit_prover` and `emit_verifier` are as in
    /// [`super::sigma_compiler_core`].
    pub fn generate(
        &self,
        spec: &SigmaCompSpec,
        emit_prover: bool,
        emit_verifier: bool,
    ) -> TokenStream {
        let proto_name = &self.proto_name;
        let group_name = &self.group_name;

        let group_types = quote! {
            pub type Scalar = <super::#group_name as super::Group>::Scalar;
            pub type Point = super::#group_name;
        };

        let mut pub_params_fields = StructFieldList::default();
        pub_params_fields.push_vars(&self.vars, true);

        // Generate the public params struct definition
        let params_def = {
            let decls = pub_params_fields.field_decls();
            let dump_impl = if cfg!(feature = "dump") {
                let dump_chunks = pub_params_fields.fields.iter().map(|f| match f {
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
                            #(#dump_chunks)*
                        }
                    }
                }
            } else {
                quote! {}
            };
            quote! {
                pub struct Params {
                    #decls
                }

                #dump_impl
            }
        };

        let mut witness_fields = StructFieldList::default();
        witness_fields.push_vars(&self.vars, false);

        // Generate the witness struct definition
        let witness_def = if emit_prover {
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
            let mut assert_statementtree = spec.statements.clone();
            let mut statement_fixup = StatementFixup {};
            assert_statementtree
                .leaves_mut()
                .into_iter()
                .for_each(|expr| statement_fixup.visit_expr_mut(expr));
            let assert_statements = assert_statementtree.leaves_mut();
            let prove_code = &self.prove_code;

            quote! {
                // The "#[allow(unused_variables)]" is temporary, until we
                // actually call the underlying sigma macro
                #[allow(unused_variables)]
                pub fn prove(params: &Params, witness: &Witness) -> Result<Vec<u8>,()> {
                    #dumper
                    let Params { #params_ids } = *params;
                    let Witness { #witness_ids } = *witness;
                    #prove_code
                    #(assert!(#assert_statements);)*
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
                // The "#[allow(unused_variables)]" is temporary, until we
                // actually call the underlying sigma macro
                #[allow(unused_variables)]
                pub fn verify(params: &Params, proof: &[u8]) -> Result<(),()> {
                    #dumper
                    let Params { #params_ids } = *params;
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
                use group::ff::PrimeField;
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

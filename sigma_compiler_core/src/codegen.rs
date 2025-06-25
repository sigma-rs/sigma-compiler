//! A module for generating the code produced by this macro.  This code
//! will interact with the underlying `sigma` macro.

use super::sigma::codegen::StructFieldList;
use super::syntax::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
#[cfg(test)]
use syn::parse_quote;
use syn::Ident;

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
    // A prefix that does not appear at the beginning of any variable
    // name in vars
    unique_prefix: String,
    prove_code: TokenStream,
}

impl CodeGen {
    /// Find a prefix that does not appear at the beginning of any
    /// variable name in `vars`
    fn unique_prefix(vars: &TaggedVarDict) -> String {
        'outer: for tag in 0usize.. {
            let try_prefix = if tag == 0 {
                "gen__".to_string()
            } else {
                format!("gen{}__", tag)
            };
            for v in vars.keys() {
                if v.starts_with(&try_prefix) {
                    continue 'outer;
                }
            }
            return try_prefix;
        }
        // The compiler complains if this isn't here, but it will only
        // get hit if vars contains at least usize::MAX entries, which
        // isn't going to happen.
        String::new()
    }

    /// Create a new [`CodeGen`] given the [`SigmaCompSpec`] you get by
    /// parsing the macro input.
    pub fn new(spec: &SigmaCompSpec) -> Self {
        Self {
            proto_name: spec.proto_name.clone(),
            group_name: spec.group_name.clone(),
            vars: spec.vars.clone(),
            unique_prefix: Self::unique_prefix(&spec.vars),
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
            unique_prefix: "gen__".into(),
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
    /// [`sigma_compiler_core`](super::sigma_compiler_core).
    pub fn generate(
        &self,
        spec: &SigmaCompSpec,
        emit_prover: bool,
        emit_verifier: bool,
    ) -> TokenStream {
        let proto_name = &self.proto_name;
        let group_name = &self.group_name;

        let group_types = quote! {
            use super::group;
            pub type Scalar = <super::#group_name as group::Group>::Scalar;
            pub type Point = super::#group_name;
        };

        // vardict contains the variables that were defined in the macro
        // call to [`sigma_compiler`]
        let vardict = taggedvardict_to_vardict(&self.vars);
        // sigma_rs_vardict contains the variables that we are passing
        // to sigma_rs.  We may have removed some via substitution, and
        // we may have added some when compiling statements like range
        // assertions into underlying linear combination assertions.
        let sigma_rs_vardict = taggedvardict_to_vardict(&spec.vars);

        // Generate the code that uses the underlying sigma_rs API
        let sigma_rs_codegen = super::sigma::codegen::CodeGen::new(
            format_ident!("sigma"),
            format_ident!("Point"),
            &sigma_rs_vardict,
            &spec.statements,
        );
        let sigma_rs_code = sigma_rs_codegen.generate(emit_prover, emit_verifier);

        let mut pub_params_fields = StructFieldList::default();
        pub_params_fields.push_vars(&vardict, true);
        let mut witness_fields = StructFieldList::default();
        witness_fields.push_vars(&vardict, false);

        let mut sigma_rs_params_fields = StructFieldList::default();
        sigma_rs_params_fields.push_vars(&sigma_rs_vardict, true);
        let mut sigma_rs_witness_fields = StructFieldList::default();
        sigma_rs_witness_fields.push_vars(&sigma_rs_vardict, false);

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
                pub struct Params {
                    #decls
                }

                #dump_impl
            }
        };

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
            let sigma_rs_params_ids = sigma_rs_params_fields.field_list();
            let sigma_rs_witness_ids = sigma_rs_witness_fields.field_list();
            let prove_code = &self.prove_code;
            let codegen_params_var = format_ident!("{}sigma_params", self.unique_prefix);
            let codegen_witness_var = format_ident!("{}sigma_witness", self.unique_prefix);

            quote! {
                pub fn prove(params: &Params, witness: &Witness) -> Result<Vec<u8>, SigmaError> {
                    #dumper
                    let Params { #params_ids } = *params;
                    let Witness { #witness_ids } = *witness;
                    #prove_code
                    let #codegen_params_var = sigma::Params {
                        #sigma_rs_params_ids
                    };
                    let #codegen_witness_var = sigma::Witness {
                        #sigma_rs_witness_ids
                    };
                    sigma::prove(&#codegen_params_var, &#codegen_witness_var)
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
            let sigma_rs_params_ids = sigma_rs_params_fields.field_list();
            let codegen_params_var = format_ident!("{}sigma_params", self.unique_prefix);
            quote! {
                pub fn verify(params: &Params, proof: &[u8]) -> Result<(), SigmaError> {
                    #dumper
                    let Params { #params_ids } = *params;
                    let #codegen_params_var = sigma::Params {
                        #sigma_rs_params_ids
                    };
                    sigma::verify(&#codegen_params_var, proof)
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
                use sigma_compiler::sigma_rs::errors::Error as SigmaError;
                #dump_use

                #group_types

                #sigma_rs_code

                #params_def
                #witness_def
                #prove_func
                #verify_func
            }
        }
    }
}

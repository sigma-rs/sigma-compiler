//! A module for generating the code that uses the `sigma-rs` crate API.
//!
//! If that crate gets its own macro interface, it can use this module
//! directly.

use super::combiners::StatementTree;
use super::types::{expr_type_tokens_id_closure, AExprType, VarDict};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use std::collections::HashSet;
use syn::{Expr, Ident};

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
    pub fn push_vars(&mut self, vars: &VarDict, for_instance: bool) {
        for (id, ti) in vars.iter() {
            match ti {
                AExprType::Scalar { is_pub, is_vec, .. } => {
                    if *is_pub == for_instance {
                        if *is_vec {
                            self.push_vecscalar(&format_ident!("{}", id))
                        } else {
                            self.push_scalar(&format_ident!("{}", id))
                        }
                    }
                }
                AExprType::Point { is_vec, .. } => {
                    if for_instance {
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
                Instance::dump_scalar(&self.#id);
                println!("");
            },
            StructField::VecScalar(id) => quote! {
                print!("  {}: [", stringify!(#id));
                for s in self.#id.iter() {
                    print!("    ");
                    Instance::dump_scalar(s);
                    println!(",");
                }
                println!("  ]");
            },
            StructField::Point(id) => quote! {
                print!("  {}: ", stringify!(#id));
                Instance::dump_point(&self.#id);
                println!("");
            },
            StructField::VecPoint(id) => quote! {
                print!("  {}: [", stringify!(#id));
                for p in self.#id.iter() {
                    print!("    ");
                    Instance::dump_point(p);
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
    unique_prefix: String,
    statements: &'a mut StatementTree,
}

impl<'a> CodeGen<'a> {
    /// Find a prefix that does not appear at the beginning of any
    /// variable name in `vars`
    fn unique_prefix(vars: &VarDict) -> String {
        'outer: for tag in 0usize.. {
            let try_prefix = if tag == 0 {
                "sigma__".to_string()
            } else {
                format!("sigma{}__", tag)
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

    pub fn new(
        proto_name: Ident,
        group_name: Ident,
        vars: &'a VarDict,
        statements: &'a mut StatementTree,
    ) -> Self {
        Self {
            proto_name,
            group_name,
            vars,
            unique_prefix: Self::unique_prefix(vars),
            statements,
        }
    }

    /// Generate the code for the `protocol` and `protocol_witness`
    /// functions that create the `ComposedRelation` and `ComposedWitness`
    /// structs, respectively, given a slice of [`Expr`]s that will be
    /// bundled into a single `LinearRelation`.  The `protocol` code
    /// must evaluate to a `Result<ComposedRelation>` and the `protocol_witness`
    /// code must evaluate to a `Result<ComposedWitness>`.
    fn linear_relation_codegen(&self, exprs: &[&Expr]) -> (TokenStream, TokenStream) {
        let instance_var = format_ident!("{}instance", self.unique_prefix);
        let lr_var = format_ident!("{}lr", self.unique_prefix);
        let mut allocated_vars: HashSet<Ident> = HashSet::new();
        let mut param_vec_code = quote! {};
        let mut witness_vec_code = quote! {};
        let mut witness_code = quote! {};
        let mut scalar_allocs = quote! {};
        let mut element_allocs = quote! {};
        let mut eq_code = quote! {};
        let mut element_assigns = quote! {};

        for (i, expr) in exprs.iter().enumerate() {
            let eq_id = format_ident!("{}eq{}", self.unique_prefix, i + 1);
            let vec_index_var = format_ident!("{}i", self.unique_prefix);
            let vec_len_var = format_ident!("{}veclen{}", self.unique_prefix, i + 1);
            // Ensure the `Expr` is of a type we recognize.  In
            // particular, it must be an assignment (C = something)
            // where the variable on the left is a public Point, and the
            // something on the right is an arithmetic expression that
            // evaluates to a private Point.  It is allowed for neither
            // or both Points to be vector variables.
            let Expr::Assign(syn::ExprAssign { left, right, .. }) = expr else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Unrecognized expression: {expr_str}");
            };
            let Expr::Path(syn::ExprPath { path, .. }) = left.as_ref() else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Left side of = is not a variable: {expr_str}");
            };
            let Some(left_id) = path.get_ident() else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Left side of = is not a variable: {expr_str}");
            };
            let Some(AExprType::Point {
                is_vec: left_is_vec,
                is_pub: true,
            }) = self.vars.get(&left_id.to_string())
            else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Left side of = is not a public point: {expr_str}");
            };
            // Record any vector variables we encountered in this
            // expression
            let mut vec_param_vars: HashSet<Ident> = HashSet::new();
            let mut vec_witness_vars: HashSet<Ident> = HashSet::new();
            if *left_is_vec {
                vec_param_vars.insert(left_id.clone());
            }
            let Ok((right_type, right_tokens)) =
                expr_type_tokens_id_closure(self.vars, right, &mut |id, id_type| match id_type {
                    AExprType::Scalar {
                        is_vec: false,
                        is_pub: false,
                        ..
                    } => {
                        if allocated_vars.insert(id.clone()) {
                            scalar_allocs = quote! {
                                #scalar_allocs
                                let #id = #lr_var.allocate_scalar();
                            };
                            witness_code = quote! {
                                #witness_code
                                witnessvec.push(witness.#id);
                            };
                        }
                        Ok(quote! {#id})
                    }
                    AExprType::Scalar {
                        is_vec: false,
                        is_pub: true,
                        ..
                    } => Ok(quote! {#instance_var.#id}),
                    AExprType::Scalar {
                        is_vec: true,
                        is_pub: false,
                        ..
                    } => {
                        vec_witness_vars.insert(id.clone());
                        if allocated_vars.insert(id.clone()) {
                            scalar_allocs = quote! {
                                #scalar_allocs
                                let #id = (0..#vec_len_var)
                                    .map(|i| #lr_var.allocate_scalar())
                                    .collect::<Vec<_>>();
                            };
                            witness_code = quote! {
                                #witness_code
                                witnessvec.extend(witness.#id.clone());
                            };
                        }
                        Ok(quote! {#id[#vec_index_var]})
                    }
                    AExprType::Scalar {
                        is_vec: true,
                        is_pub: true,
                        ..
                    } => {
                        vec_param_vars.insert(id.clone());
                        Ok(quote! {#instance_var.#id[#vec_index_var]})
                    }
                    AExprType::Point { is_vec: false, .. } => {
                        if allocated_vars.insert(id.clone()) {
                            element_allocs = quote! {
                                #element_allocs
                                let #id = #lr_var.allocate_element();
                            };
                            element_assigns = quote! {
                                #element_assigns
                                #lr_var.set_element(#id, #instance_var.#id);
                            };
                        }
                        Ok(quote! {#id})
                    }
                    AExprType::Point { is_vec: true, .. } => {
                        vec_param_vars.insert(id.clone());
                        if allocated_vars.insert(id.clone()) {
                            element_allocs = quote! {
                                #element_allocs
                                let #id = (0..#vec_len_var)
                                    .map(|#vec_index_var| #lr_var.allocate_element())
                                    .collect::<Vec<_>>();
                            };
                            element_assigns = quote! {
                                #element_assigns
                                for #vec_index_var in 0..#vec_len_var {
                                    #lr_var.set_element(
                                        #id[#vec_index_var],
                                        #instance_var.#id[#vec_index_var],
                                    );
                                }
                            };
                        }
                        Ok(quote! {#id[#vec_index_var]})
                    }
                })
            else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Right side of = is not a valid arithmetic expression: {expr_str}");
            };
            let AExprType::Point {
                is_vec: right_is_vec,
                ..
            } = right_type
            else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Right side of = does not evaluate to a Point: {expr_str}");
            };
            if *left_is_vec != right_is_vec {
                let expr_str = quote! { #expr }.to_string();
                panic!("Only one side of = is a vector expression: {expr_str}");
            }
            let vec_param_varvec = Vec::from_iter(vec_param_vars);
            let vec_witness_varvec = Vec::from_iter(vec_witness_vars);

            if !vec_param_varvec.is_empty() {
                let firstvar = &vec_param_varvec[0];
                param_vec_code = quote! {
                    #param_vec_code
                    let #vec_len_var = #instance_var.#firstvar.len();
                };
                for thisvar in vec_param_varvec.iter().skip(1) {
                    param_vec_code = quote! {
                        #param_vec_code
                        if #vec_len_var != #instance_var.#thisvar.len() {
                            eprintln!(
                                "Instance variables {} and {} must have the same length",
                                stringify!(#firstvar),
                                stringify!(#thisvar),
                            );
                            return Err(SigmaError::VerificationFailure);
                        }
                    };
                }
                if !vec_witness_varvec.is_empty() {
                    witness_vec_code = quote! {
                        #witness_vec_code
                        let #vec_len_var = instance.#firstvar.len();
                    };
                }
                for witvar in vec_witness_varvec {
                    witness_vec_code = quote! {
                        #witness_vec_code
                        if #vec_len_var != witness.#witvar.len() {
                            eprintln!(
                                "Instance variables {} and {} must have the same length",
                                stringify!(#firstvar),
                                stringify!(#witvar),
                            );
                            return Err(SigmaError::VerificationFailure);
                        }
                    }
                }
            };
            if right_is_vec {
                eq_code = quote! {
                    #eq_code
                    let #eq_id = (0..#vec_len_var)
                        .map(|#vec_index_var| #lr_var.allocate_eq(#right_tokens))
                        .collect::<Vec<_>>();
                };
                element_assigns = quote! {
                    #element_assigns
                    for #vec_index_var in 0..#vec_len_var {
                        #lr_var.set_element(
                            #eq_id[#vec_index_var],
                            #instance_var.#left_id[#vec_index_var],
                        );
                    }
                };
            } else {
                eq_code = quote! {
                    #eq_code
                    let #eq_id = #lr_var.allocate_eq(#right_tokens);
                };
                element_assigns = quote! {
                    #element_assigns
                    #lr_var.set_element(#eq_id, #instance_var.#left_id);
                }
            }
        }

        (
            quote! {
                {
                    let mut #lr_var = LinearRelation::<Point>::new();
                    #param_vec_code
                    #scalar_allocs
                    #element_allocs
                    #eq_code
                    #element_assigns

                    Ok(ComposedRelation::from(#lr_var))
                }
            },
            quote! {
                {
                    #witness_vec_code
                    let mut witnessvec = Vec::new();
                    #witness_code
                    Ok(ComposedWitness::Simple(witnessvec))
                }
            },
        )
    }

    /// Generate the code for the `protocol` and `protocol_witness`
    /// functions that create the `Protocol` and `ComposedWitness`
    /// structs, respectively, given a [`StatementTree`] describing the
    /// statements to be proven.  The output components are the code for
    /// the `protocol` and `protocol_witness` functions, respectively.
    /// The `protocol` code must evaluate to a `Result<Protocol>` and
    /// the `protocol_witness` code must evaluate to a
    /// `Result<ComposedWitness>`.
    fn proto_witness_codegen(&self, statement: &StatementTree) -> (TokenStream, TokenStream) {
        match statement {
            // The StatementTree has no statements (it's just the single
            // leaf "true")
            StatementTree::Leaf(_) if statement.is_leaf_true() => (
                quote! {
                    Ok(ComposedRelation::from(LinearRelation::<Point>::new()))
                },
                quote! {
                    Ok(ComposedWitness::Simple(vec![]))
                },
            ),
            // The StatementTree is a single statement.  Generate a
            // single LinearRelation from it.
            StatementTree::Leaf(leafexpr) => {
                self.linear_relation_codegen(std::slice::from_ref(&leafexpr))
            }
            // The StatementTree is an And.  Separate out the leaf
            // statements, and generate a single LinearRelation from
            // them.  Then if there are non-leaf nodes as well, And them
            // together.
            StatementTree::And(stvec) => {
                let mut leaves: Vec<&Expr> = Vec::new();
                let mut others: Vec<&StatementTree> = Vec::new();
                for st in stvec {
                    match st {
                        StatementTree::Leaf(le) => leaves.push(le),
                        _ => others.push(st),
                    }
                }
                let (proto_code, witness_code) = self.linear_relation_codegen(&leaves);
                if others.is_empty() {
                    (proto_code, witness_code)
                } else {
                    let (others_proto, others_witness): (Vec<TokenStream>, Vec<TokenStream>) =
                        others
                            .iter()
                            .map(|st| self.proto_witness_codegen(st))
                            .unzip();
                    (
                        quote! {
                            Ok(ComposedRelation::And(vec![
                                #proto_code.map_err(|e| -> SigmaError { e })?,
                                #(#others_proto.map_err(|e| -> SigmaError { e })?,)*
                            ]))
                        },
                        quote! {
                            Ok(ComposedWitness::And(vec![
                                #witness_code.map_err(|e| -> SigmaError { e })?,
                                #(#others_witness.map_err(|e| -> SigmaError { e })?,)*
                            ]))
                        },
                    )
                }
            }
            StatementTree::Or(stvec) => {
                let (proto, witness): (Vec<TokenStream>, Vec<TokenStream>) = stvec
                    .iter()
                    .map(|st| self.proto_witness_codegen(st))
                    .unzip();
                (
                    quote! {
                        Ok(ComposedRelation::Or(vec![
                            #(#proto.map_err(|e| -> SigmaError { e })?,)*
                        ]))
                    },
                    quote! {
                        Ok(ComposedWitness::Or(vec![
                            #(CtOption::new(
                                #witness.map_err(|e| -> SigmaError { e })?,
                                1u8.into()),)*
                        ]))
                    },
                )
            }
            StatementTree::Thresh(_thresh, _stvec) => {
                todo! {"Thresh not yet implemented"};
            }
        }
    }

    /// Generate the code that uses the `sigma-rs` API to prove and
    /// verify the statements in the [`CodeGen`].
    ///
    /// `emit_prover` and `emit_verifier` are as in
    /// [`sigma_compiler_core`](super::super::sigma_compiler_core).
    pub fn generate(&mut self, emit_prover: bool, emit_verifier: bool) -> TokenStream {
        let proto_name = &self.proto_name;
        let group_name = &self.group_name;

        let group_types = quote! {
            use super::group;
            pub type Scalar = <super::#group_name as group::Group>::Scalar;
            pub type Point = super::#group_name;
        };

        // Flatten nested "And"s into single "And"s
        self.statements.flatten_ands();

        println!("Statements = {{");
        self.statements.dump();
        println!("}}");

        let mut pub_instance_fields = StructFieldList::default();
        pub_instance_fields.push_vars(self.vars, true);

        // Generate the public instance struct definition
        let instance_def = {
            let decls = pub_instance_fields.field_decls();
            #[cfg(feature = "dump")]
            let dump_impl = {
                let dump_chunks = pub_instance_fields.dump();
                quote! {
                    impl Instance {
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
                pub struct Instance {
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

        let (protocol_code, witness_code) = self.proto_witness_codegen(self.statements);

        // Generate the function that creates the sigma-rs Protocol
        let protocol_func = {
            let instance_var = format_ident!("{}instance", self.unique_prefix);

            quote! {
                fn protocol(
                    #instance_var: &Instance,
                ) -> Result<ComposedRelation<Point>, SigmaError> {
                    #protocol_code
                }
            }
        };

        // Generate the function that creates the sigma-rs ComposedWitness
        let witness_func = if emit_prover {
            quote! {
                fn protocol_witness(
                    instance: &Instance,
                    witness: &Witness,
                ) -> Result<ComposedWitness<Point>, SigmaError> {
                    #witness_code
                }
            }
        } else {
            quote! {}
        };

        // Generate the prove function
        let prove_func = if emit_prover {
            let instance_var = format_ident!("{}instance", self.unique_prefix);
            let witness_var = format_ident!("{}witness", self.unique_prefix);
            let session_id_var = format_ident!("{}session_id", self.unique_prefix);
            let rng_var = format_ident!("{}rng", self.unique_prefix);
            let proto_var = format_ident!("{}proto", self.unique_prefix);
            let proto_witness_var = format_ident!("{}proto_witness", self.unique_prefix);
            let nizk_var = format_ident!("{}nizk", self.unique_prefix);

            let dumper = if cfg!(feature = "dump") {
                quote! {
                    println!("prover instance = {{");
                    #instance_var.dump();
                    println!("}}");
                }
            } else {
                quote! {}
            };

            quote! {
                pub fn prove(
                    #instance_var: &Instance,
                    #witness_var: &Witness,
                    #session_id_var: &[u8],
                    #rng_var: &mut (impl CryptoRng + RngCore),
                ) -> Result<Vec<u8>, SigmaError> {
                    #dumper
                    let #proto_var = protocol(#instance_var)?;
                    let #proto_witness_var = protocol_witness(#instance_var, #witness_var)?;
                    let #nizk_var = #proto_var.into_nizk(#session_id_var);

                    #nizk_var.prove_batchable(&#proto_witness_var, #rng_var)
                }
            }
        } else {
            quote! {}
        };

        // Generate the verify function
        let verify_func = if emit_verifier {
            let instance_var = format_ident!("{}instance", self.unique_prefix);
            let proof_var = format_ident!("{}proof", self.unique_prefix);
            let session_id_var = format_ident!("{}session_id", self.unique_prefix);
            let proto_var = format_ident!("{}proto", self.unique_prefix);
            let nizk_var = format_ident!("{}nizk", self.unique_prefix);

            let dumper = if cfg!(feature = "dump") {
                quote! {
                    println!("verifier instance = {{");
                    #instance_var.dump();
                    println!("}}");
                }
            } else {
                quote! {}
            };

            quote! {
                pub fn verify(
                    #instance_var: &Instance,
                    #proof_var: &[u8],
                    #session_id_var: &[u8],
                ) -> Result<(), SigmaError> {
                    #dumper
                    let #proto_var = protocol(#instance_var)?;
                    let #nizk_var = #proto_var.into_nizk(#session_id_var);

                    #nizk_var.verify_batchable(#proof_var)
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
                use sigma_compiler::sigma_rs;
                use sigma_rs::{
                    composition::{ComposedRelation, ComposedWitness},
                    errors::Error as SigmaError,
                    LinearRelation, Nizk,
                };
                use sigma_compiler::rand::{CryptoRng, RngCore};
                use sigma_compiler::group::ff::PrimeField;
                use sigma_compiler::subtle::CtOption;
                use std::ops::Neg;
                #dump_use

                #group_types
                #instance_def
                #witness_def

                #protocol_func
                #witness_func
                #prove_func
                #verify_func
            }
        }
    }
}

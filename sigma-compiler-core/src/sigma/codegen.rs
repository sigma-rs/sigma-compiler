//! A module for generating the code that uses the `sigma-proofs` crate API.
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
#[derive(Clone)]
pub enum StructField {
    Scalar(Ident),
    VecScalar(Ident),
    Point(Ident),
    VecPoint(Ident),
}

impl StructField {
    /// Extract the [`struct@Ident`] from the [`StructField`]
    pub fn ident(&self) -> Ident {
        match self {
            Self::Scalar(id) | Self::VecScalar(id) | Self::Point(id) | Self::VecPoint(id) => {
                id.clone()
            }
        }
    }
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
    /// Output a ToTokens of code to dump the contents of the fields to
    /// the `std::fmt::Formatter` with the given `fmt_id`
    pub fn dump(&self, fmt_id: &Ident) -> impl ToTokens {
        // Sort the field ids
        let mut fields = self.fields.clone();
        fields.sort_by_key(|f| f.ident());

        let dump_chunks = fields.iter().map(|f| match f {
            // It's not a big deal if writes fail here, so we use "ok()"
            // to ignore the `Result`
            StructField::Scalar(id) => quote! {
                write!(#fmt_id, "  {}: ", stringify!(#id)).ok();
                Instance::dump_scalar(&self.#id, #fmt_id);
                write!(#fmt_id, ",\n").ok();
            },
            StructField::VecScalar(id) => quote! {
                write!(#fmt_id, "  {}: [\n", stringify!(#id)).ok();
                for s in self.#id.iter() {
                    write!(#fmt_id, "    ").ok();
                    Instance::dump_scalar(s, #fmt_id);
                    write!(#fmt_id, ",\n").ok();
                }
                write!(#fmt_id, "  ],\n").ok();
            },
            StructField::Point(id) => quote! {
                write!(#fmt_id, "  {}: ", stringify!(#id)).ok();
                Instance::dump_point(&self.#id, #fmt_id);
                write!(#fmt_id, ",\n").ok();
            },
            StructField::VecPoint(id) => quote! {
                write!(#fmt_id, "  {}: [\n", stringify!(#id)).ok();
                for p in self.#id.iter() {
                    write!(#fmt_id, "    ").ok();
                    Instance::dump_point(p, #fmt_id);
                    write!(#fmt_id, ",\n").ok();
                }
                write!(#fmt_id, "  ],\n").ok();
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

/// The main struct to handle code generation using the `sigma-proofs` API.
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

            // Record any vector variables we encountered in this
            // expression
            let mut vec_param_vars: HashSet<Ident> = HashSet::new();
            let mut vec_witness_vars: HashSet<Ident> = HashSet::new();

            // Ensure the `Expr` is of a type we recognize.  In
            // particular, it must be an assignment (left = right) where
            // the expression on the left is an arithmetic expression
            // that evaluates to a public Point, and the expression on
            // the right is an arithmetic expression that evaluates to a
            // Point.  It is allowed for neither or both Points to be
            // vector variables.
            let Expr::Assign(syn::ExprAssign { left, right, .. }) = expr else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Unrecognized expression: {expr_str}");
            };
            let (left_type, left_tokens) =
                expr_type_tokens_id_closure(self.vars, left, &mut |id, id_type| match id_type {
                    AExprType::Scalar { is_pub: false, .. } => {
                        panic!("Left side of = contains a private Scalar");
                    }
                    AExprType::Scalar {
                        is_vec: false,
                        is_pub: true,
                        ..
                    }
                    | AExprType::Point { is_vec: false, .. } => Ok(quote! {#instance_var.#id}),
                    AExprType::Scalar {
                        is_vec: true,
                        is_pub: true,
                        ..
                    }
                    | AExprType::Point { is_vec: true, .. } => {
                        vec_param_vars.insert(id.clone());
                        Ok(quote! {#instance_var.#id})
                    }
                })
                .unwrap();
            let AExprType::Point {
                is_pub: true,
                is_vec: left_is_vec,
            } = left_type
            else {
                let expr_str = quote! { #expr }.to_string();
                panic!("Left side of = does not evaluate to a public point: {expr_str}");
            };
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
                        Ok(quote! { #id })
                    }
                    AExprType::Scalar {
                        is_vec: true,
                        is_pub: true,
                        ..
                    } => {
                        vec_param_vars.insert(id.clone());
                        Ok(quote! {#instance_var.#id})
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
                        Ok(quote! { #id })
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
            if left_is_vec != right_is_vec {
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
                    let #eq_id = (#right_tokens)
                        .iter()
                        .cloned()
                        .map(|lr| #lr_var.allocate_eq(lr))
                        .collect::<Vec<_>>();
                };
                element_assigns = quote! {
                    #element_assigns
                    (#left_tokens)
                        .iter()
                        .zip(#eq_id.iter())
                        .for_each(|(l,eq)| #lr_var.set_element(*eq, *l));
                };
            } else {
                eq_code = quote! {
                    #eq_code
                    let #eq_id = #lr_var.allocate_eq(#right_tokens);
                };
                element_assigns = quote! {
                    #element_assigns
                    #lr_var.set_element(#eq_id, #left_tokens);
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

                    SigmaOk(ComposedRelation::try_from(#lr_var).unwrap())
                }
            },
            quote! {
                {
                    #witness_vec_code
                    let mut witnessvec = Vec::new();
                    #witness_code
                    SigmaOk(ComposedWitness::Simple(witnessvec))
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
                    Ok(ComposedRelation::try_from(LinearRelation::<Point>::new()).unwrap())
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
                            SigmaOk(ComposedRelation::and([
                                #proto_code?,
                                #(#others_proto?,)*
                            ]))
                        },
                        quote! {
                            SigmaOk(ComposedWitness::and([
                                #witness_code?,
                                #(#others_witness?,)*
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
                        SigmaOk(ComposedRelation::or([
                            #(#proto?,)*
                        ]))
                    },
                    quote! {
                        SigmaOk(ComposedWitness::or([
                            #(#witness?,)*
                        ]))
                    },
                )
            }
            StatementTree::Thresh(thresh, stvec) => {
                let (proto, witness): (Vec<TokenStream>, Vec<TokenStream>) = stvec
                    .iter()
                    .map(|st| self.proto_witness_codegen(st))
                    .unzip();
                (
                    quote! {
                        SigmaOk(ComposedRelation::threshold(#thresh, [
                            #(#proto?,)*
                        ]))
                    },
                    quote! {
                        SigmaOk(ComposedWitness::threshold([
                            #(#witness?,)*
                        ]))
                    },
                )
            }
        }
    }

    /// Generate the code that uses the `sigma-proofs` API to prove and
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

        let mut pub_instance_fields = StructFieldList::default();
        pub_instance_fields.push_vars(self.vars, true);

        // Generate the public instance struct definition
        let instance_def = {
            let decls = pub_instance_fields.field_decls();
            #[cfg(feature = "dump")]
            let dump_impl = {
                let dump_chunks = pub_instance_fields.dump(&format_ident!("fmt"));
                quote! {
                    impl Instance {
                        fn dump_scalar(s: &Scalar, fmt: &mut std::fmt::Formatter<'_>) {
                            let bytes: &[u8] = &s.to_repr();
                            for b in bytes.iter().rev() {
                                // It's not a big deal if writes fail
                                // here, so we use "ok()" to ignore the
                                // `Result`
                                write!(fmt, "{:02x}", b).ok();
                            }
                        }

                        fn dump_point(p: &Point, fmt: &mut std::fmt::Formatter<'_>) {
                            let bytes: &[u8] = &p.to_bytes();
                            for b in bytes.iter().rev() {
                                // It's not a big deal if writes fail
                                // here, so we use "ok()" to ignore the
                                // `Result`
                                write!(fmt, "{:02x}", b).ok();
                            }
                        }
                    }

                    impl std::fmt::Debug for Instance {
                        fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            #dump_chunks
                            Ok(())
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

        // Generate the function that creates the sigma-proofs Protocol
        let protocol_func = {
            let instance_var = format_ident!("{}instance", self.unique_prefix);

            quote! {
                fn protocol(
                    #instance_var: &Instance,
                ) -> SigmaResult<ComposedRelation<Point>> {
                    #protocol_code
                }
            }
        };

        // Generate the function that creates the sigma-proofs ComposedWitness
        let witness_func = if emit_prover {
            quote! {
                fn protocol_witness(
                    instance: &Instance,
                    witness: &Witness,
                ) -> SigmaResult<ComposedWitness<Point>> {
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

            quote! {
                pub fn prove(
                    #instance_var: &Instance,
                    #witness_var: &Witness,
                    #session_id_var: &[u8],
                    #rng_var: &mut (impl CryptoRng + RngCore),
                ) -> SigmaResult<Vec<u8>> {
                    let #proto_var = protocol(#instance_var)?;
                    let #proto_witness_var = protocol_witness(#instance_var, #witness_var)?;
                    let #nizk_var = #proto_var.into_nizk(#session_id_var);

                    #nizk_var.prove_compact(&#proto_witness_var, #rng_var)
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

            quote! {
                pub fn verify(
                    #instance_var: &Instance,
                    #proof_var: &[u8],
                    #session_id_var: &[u8],
                ) -> SigmaResult<()> {
                    let #proto_var = protocol(#instance_var)?;
                    let #nizk_var = #proto_var.into_nizk(#session_id_var);

                    #nizk_var.verify_compact(#proof_var)
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
                use super::sigma_compiler;
                use sigma_compiler::sigma_proofs;
                use sigma_compiler::group::ff::PrimeField;
                use sigma_compiler::rand::{CryptoRng, RngCore};
                use sigma_compiler::subtle::CtOption;
                use sigma_compiler::vecutils::*;
                use sigma_proofs::{
                    composition::{ComposedRelation, ComposedWitness},
                    errors::Error as SigmaError,
                    errors::Ok as SigmaOk,
                    errors::Result as SigmaResult,
                    LinearRelation, Nizk,
                };
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

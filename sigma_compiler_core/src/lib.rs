use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use std::collections::HashMap;
use syn::visit_mut::{self, VisitMut};
use syn::{parse_quote, Expr, Ident, Token};

/// The submodules that would be useful to have in the lower-level
/// `sigma` crate are for now included as submodules of a local `sigma`
/// module
pub mod sigma {
    pub mod combiners;
    pub mod types;
}
mod syntax;
mod transform;

pub use syntax::{SigmaCompSpec, TaggedIdent, TaggedPoint, TaggedScalar, TaggedVarDict};

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
}

/// An implementation of the
/// [`VisitMut`](https://docs.rs/syn/latest/syn/visit_mut/trait.VisitMut.html)
/// trait that massages the provided statements.
///
/// This massaging currently consists of:
///   - Changing equality from = to ==
///   - Changing any identifier `id` to either `params.id` or
///     `witness.id` depending on whether it is public or private
struct StatementFixup {
    idmap: HashMap<String, Expr>,
}

impl StatementFixup {
    pub fn new(spec: &SigmaCompSpec) -> Self {
        let mut idmap: HashMap<String, Expr> = HashMap::new();

        // For each public identifier id (Points, or Scalars marked
        // "pub"), add to the map "id" -> params.id.  For each private
        // identifier (Scalars not marked "pub"), add to the map "id" ->
        // witness.id.
        for (id, is_pub) in spec.vars.values().map(|ti| match ti {
            TaggedIdent::Scalar(st) => (&st.id, st.is_pub),
            TaggedIdent::Point(pt) => (&pt.id, true),
        }) {
            let idexpr: Expr = if is_pub {
                parse_quote! { params.#id }
            } else {
                parse_quote! { witness.#id }
            };
            idmap.insert(id.to_string(), idexpr);
        }

        Self { idmap }
    }
}

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
        if let Expr::Path(expath) = node {
            if let Some(id) = expath.path.get_ident() {
                if let Some(expr) = self.idmap.get(&id.to_string()) {
                    *node = expr.clone();
                    // Don't recurse
                    return;
                }
            }
        }
        // Unless we bailed out above, continue with the default
        // traversal
        visit_mut::visit_expr_mut(self, node);
    }
}

/// The main function of this macro.
///
/// Parse the macro input with [`parse`](SigmaCompSpec#method.parse) to
/// produce a [`SigmaCompSpec`], and then pass that to this function to
/// output the data structures and code for the ZKP protocol
/// implementation.
///
/// If `emit_prover` is `true`, output the data structures and code for
/// the prover side.  If `emit_verifier` is `true`, output the data
/// structures and code for the verifier side.  (Typically both will be
/// `true`, but you can set one to `false` if you don't need that side
/// of the protocol.)
pub fn sigma_compiler_core(
    spec: &mut SigmaCompSpec,
    emit_prover: bool,
    emit_verifier: bool,
) -> TokenStream {
    let proto_name = &spec.proto_name;
    let group_name = &spec.group_name;

    // Save a copy of the input TaggedVarDict, because the Params and
    // Witness structs will be built from that TaggedVarDict, not the
    // one modified by apply_substitutions below.

    let input_vars = spec.vars.clone();

    // Apply any substitution transformations
    transform::apply_substitutions(&mut spec.statements, &mut spec.vars).unwrap();

    let group_types = quote! {
        pub type Scalar = <super::#group_name as super::Group>::Scalar;
        pub type Point = super::#group_name;
    };

    // Generate the public params struct definition
    let params_def = {
        let mut pub_params_fields = StructFieldList::default();
        pub_params_fields.push_vars(&input_vars, true);

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

    // Generate the witness struct definition
    let witness_def = if emit_prover {
        let mut witness_fields = StructFieldList::default();
        witness_fields.push_vars(&input_vars, false);

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
        let mut assert_statementtree = spec.statements.clone();
        let mut statement_fixup = StatementFixup::new(spec);
        assert_statementtree
            .leaves_mut()
            .into_iter()
            .for_each(|expr| statement_fixup.visit_expr_mut(expr));
        let assert_statements = assert_statementtree.leaves_mut();

        quote! {
            pub fn prove(params: &Params, witness: &Witness) -> Result<Vec<u8>,()> {
                #dumper
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
        quote! {
            pub fn verify(params: &Params, proof: &[u8]) -> Result<(),()> {
                #dumper
                Ok(())
            }
        }
    } else {
        quote! {}
    };

    // Output the generated module for this protocol
    let dump_use = if cfg!(feature = "dump") {
        quote! {
            use ff::PrimeField;
            use group::GroupEncoding;
        }
    } else {
        quote! {}
    };
    quote! {
        #[allow(non_snake_case)]
        pub mod #proto_name {
            #dump_use

            #group_types
            #params_def
            #witness_def

            #prove_func
            #verify_func
        }
    }
}

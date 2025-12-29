//! A module for operations that transform a [`StatementTree`].
//! Every transformation must maintain the [disjunction invariant].
//!
//! [disjunction invariant]: StatementTree::check_disjunction_invariant

use super::codegen::CodeGen;
use super::pedersen::{
    convert_commitment, convert_randomness, random_scalars, recognize_pedersen_assignment,
    LinScalar, PedersenAssignment,
};
use super::sigma::combiners::*;
use super::syntax::{collect_cind_points, taggedvardict_to_vardict};
use super::{TaggedIdent, TaggedScalar, TaggedVarDict};
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::visit_mut::{self, VisitMut};
use syn::{parse_quote, Error, Expr, Ident, Result};

/// Simplify a [`StatementTree`] by pruning leaves that are the constant
/// `true`, and simplifying `And`, `Or`, and `Thresh` combiners that
/// have fewer than two children.
pub fn prune_statement_tree(st: &mut StatementTree) {
    match st {
        // If the StatementTree is just a Leaf, just keep it unmodified,
        // even if it is leaf_true.
        StatementTree::Leaf(_) => {}

        // For the And combiner, recursively simplify each child, and then
        // prune the child if it is leaf_true.  If we end up with 1
        // child replace ourselves with that child.  If we end up with 0
        // children, replace ourselves with leaf_true.
        StatementTree::And(v) => {
            let mut i: usize = 0;
            // Note that v.len _can change_ during this loop
            while i < v.len() {
                prune_statement_tree(&mut v[i]);
                if v[i].is_leaf_true() {
                    // Remove this child, and _do not_ increment i
                    v.remove(i);
                } else {
                    i += 1;
                }
            }
            if v.is_empty() {
                *st = StatementTree::leaf_true();
            } else if v.len() == 1 {
                let child = v.remove(0);
                *st = child;
            }
        }

        // For the Or combiner, recursively simplify each child, and if
        // it ends up leaf_true, replace ourselves with leaf_true.
        // If we end up with 1 child, we must have started wth 1 child.
        // Replace ourselves with that child anyway.
        StatementTree::Or(v) => {
            let mut i: usize = 0;
            // Note that v.len _can change_ during this loop
            while i < v.len() {
                prune_statement_tree(&mut v[i]);
                if v[i].is_leaf_true() {
                    *st = StatementTree::leaf_true();
                    return;
                } else {
                    i += 1;
                }
            }
            if v.len() == 1 {
                let child = v.remove(0);
                *st = child;
            }
        }

        // For the Thresh combiner, recursively simplify each child, and
        // if it ends up leaf_true, prune it, and subtract 1 from the
        // thresh.  If the thresh hits 0, replace ourselves with
        // leaf_true.  If we end up with 1 child and thresh is 1,
        // replace ourselves with that child.
        StatementTree::Thresh(thresh, v) => {
            let mut i: usize = 0;
            // Note that v.len _can change_ during this loop
            while i < v.len() {
                prune_statement_tree(&mut v[i]);
                if v[i].is_leaf_true() {
                    // Remove this child, and _do not_ increment i
                    v.remove(i);
                    // But decrement thresh
                    *thresh -= 1;
                    if *thresh == 0 {
                        *st = StatementTree::leaf_true();
                        return;
                    }
                } else {
                    i += 1;
                }
            }
            if v.len() == 1 {
                // If thresh == 0, we would have exited above
                assert!(*thresh == 1);
                let child = v.remove(0);
                *st = child;
            }
        }
    }
}

/// Add parentheses around an [`Expr`] (which represents an [arithmetic
/// expression]) if needed.
///
/// The parentheses are needed if the [`Expr`] would parse as multiple
/// tokens.  For example, `a+b` turns into `(a+b)`, but `c`
/// remains `c` and `(a+b)` remains `(a+b)`.
///
/// [arithmetic expression]: super::sigma::types::expr_type
pub fn paren_if_needed(expr: Expr) -> Expr {
    match expr {
        Expr::Unary(_) | Expr::Binary(_) => parse_quote! { (#expr) },
        _ => expr,
    }
}

/// Transform the [`StatementTree`] so that it satisfies the
/// [disjunction invariant].
///
/// [disjunction invariant]: StatementTree::check_disjunction_invariant
#[allow(non_snake_case)] // so that Points can be capital letters
pub fn enforce_disjunction_invariant(
    codegen: &mut CodeGen,
    st: &mut StatementTree,
    vars: &mut TaggedVarDict,
) -> Result<()> {
    // Make the VarDict version of the variable dictionary
    let mut vardict = taggedvardict_to_vardict(vars);

    // A HashSet of the random Scalars in the macro input
    let mut randoms = random_scalars(vars, st);

    // A list of the computationally independent (non-vector) Points in
    // the macro input.  If we need to do any transformations, there
    // must be at least two of them in order to create Pedersen
    // commitments.

    let cind_points = collect_cind_points(vars);

    // Extra statements to be added to the root disjunction branch
    let mut root_extra_statements: Vec<StatementTree> = Vec::new();

    // The generated variable name for the rng
    let rng_var = codegen.gen_ident(&format_ident!("rng"));

    // Find any statements that look like Pedersen commitments in the
    // root disjunction branch of the StatementTree, and make a HashMap
    // mapping the committed private variable to the parsed commitment.
    let mut root_pedersens: HashMap<Ident, PedersenAssignment> = HashMap::new();
    st.for_each_disjunction_branch_leaf(&mut |leaf| {
        // See if we recognize this leaf expression as a
        // PedersenAssignment, and if so, map its variable to the
        // PedersenAssignment.
        if let StatementTree::Leaf(leafexpr) = leaf {
            if let Some(ped_assign) =
                recognize_pedersen_assignment(vars, &randoms, &vardict, leafexpr)
            {
                root_pedersens.insert(ped_assign.var(), ped_assign);
            }
        }
        Ok(())
    })?;

    // Count how many disjunction branches contain each private Scalar
    let mut branch_count: HashMap<Ident, usize> = HashMap::new();
    st.for_each_disjunction_branch(&mut |branch, _path| {
        branch
            .disjunction_branch_priv_scalars(&vardict)
            .drain()
            .for_each(|id| {
                if let Some(n) = branch_count.get(&id) {
                    branch_count.insert(id, n + 1);
                } else {
                    branch_count.insert(id, 1);
                }
            });
        Ok(())
    })?;

    // Make a HashSet of any of those private Scalars whose count is
    // strictly larger than 1.  (Those private Scalars are the ones
    // that are in violation of the disjunction invariant.)
    let mut invariant_violators: HashSet<Ident> = branch_count
        .drain()
        .filter_map(|(id, n)| if n > 1 { Some(id) } else { None })
        .collect();

    // If there are no invariant violators, we're done.
    if invariant_violators.is_empty() {
        return Ok(());
    }

    // Otherwise, ensure there are at least two computationally
    // independent points, since we'll need to construct Pedersen
    // commitments.
    if cind_points.len() < 2 {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            "At least two cind Points must be declared to support Pedersen commitments",
        ));
    }
    let cind_A = &cind_points[0];
    let cind_B = &cind_points[1];

    // For each invariant violator, find (or create) a Pedersen
    // commitment in the root disjunction branch for it.
    let invariant_violator_pedersens: HashMap<Ident, PedersenAssignment> = invariant_violators
        .drain()
        .map(|id| {
            // Check if the private Scalar is a vector variable or
            // not
            let is_vec = if let Some(TaggedIdent::Scalar(TaggedScalar { is_vec, .. })) =
                vars.get(&id.to_string())
            {
                *is_vec
            } else {
                false
            };

            // See if we already have a PedersenAssignment in the
            // root disjunction branch for this private Scalar
            let ped_assign = if let Some(ped_assign) = root_pedersens.get(&id) {
                ped_assign.clone()
            } else {
                // Create new variables for the Pedersen commitment and its
                // random Scalar.
                let commitment_var = codegen.gen_point(
                    vars,
                    &format_ident!("disj_{}_genC", id),
                    is_vec, // is_vec
                    true,   // send_to_verifier
                );
                let rand_var = codegen.gen_scalar(
                    vars,
                    &format_ident!("disj_{}_genr", id),
                    true,   // is_rand
                    is_vec, // is_vec
                );

                // Update vardict and randoms with the new vars
                vardict = taggedvardict_to_vardict(vars);
                randoms.insert(rand_var.to_string());

                let ped_assign_expr: Expr = parse_quote! {
                    #commitment_var = #id * #cind_A + #rand_var * #cind_B
                };
                let ped_assign =
                    recognize_pedersen_assignment(vars, &randoms, &vardict, &ped_assign_expr)
                        .unwrap();

                if is_vec {
                    codegen.prove_append(quote! {
                        let #rand_var: Vec<Scalar> = #id
                            .map(|_| Scalar::random(#rng_var))
                            .collect();
                        let #commitment_var = (0..#id.len())
                            .map(|i| {
                                #id[i] * #cind_A + #rand_var[i] * #cind_B
                            })
                            .collect();
                    });
                } else {
                    codegen.prove_append(quote! {
                        let #rand_var = Scalar::random(#rng_var);
                        let #ped_assign_expr;
                    });
                }

                root_extra_statements.push(StatementTree::Leaf(ped_assign_expr));

                ped_assign
            };

            // At this point, we have a Pedersen commitment for some linear
            // function of id (given by
            // ped_assign.pedersen.var_term.coeff), using some linear
            // function of rand_var (given by
            // ped_assign.pedersen.rand_term.coeff) as the randomness.  But
            // what we need is a Pedersen commitment for id itself.
            // So we output runtime code for both the prover and the
            // verifier that converts the commitment, and code for just
            // the prover that converts the randomness.

            // Make new runtime variables to hold the converted
            // commitment and randomness
            let commitment_var = codegen.gen_point(
                vars,
                &format_ident!("disj_{}_C", id),
                is_vec, // is_vec
                false,  // send_to_verifier
            );
            let rand_var = codegen.gen_ident(&format_ident!("disj_{}_r", id));

            // Update vardict and randoms with the new vars
            vardict = taggedvardict_to_vardict(vars);
            randoms.insert(rand_var.to_string());

            // The identity LinScalar for this id
            let id_linscalar = LinScalar {
                coeff: 1i128,
                pub_scalar_expr: None,
                id: id.clone(),
                is_vec,
            };

            codegen.prove_verify_append(
                convert_commitment(&commitment_var, &ped_assign, &id_linscalar, &vardict).unwrap(),
            );
            codegen.prove_append(
                convert_randomness(&rand_var, &ped_assign, &id_linscalar, &vardict).unwrap(),
            );

            (id, ped_assign)
        })
        .collect();

    // Do another pass over each disjunction branch (other than the
    // root).  In each non-root branch, if there are any instances of an
    // invariant violator, then change all instances of that violating
    // identifier to a fresh identifier, and insert a Pedersen
    // commitment (to the same commitment variable that exists in the
    // root disjunction branch) to bind the new identifier to the
    // original.
    let mut disjunction_branch_num = 0usize;
    st.for_each_disjunction_branch(&mut |branch, path| {
        // Skip the root disjunction branch, which is represented by an
        // empty path
        if path.is_empty() {
            return Ok(());
        }

        disjunction_branch_num += 1;

        // Keep track of the ids in invariant_violator_pedersens
        // that we encounter and rename in this disjunction branch
        let mut ids_renamed: HashSet<Ident> = HashSet::new();

        // Extra statements to be added to this disjunction branch
        let mut branch_extra_statements: Vec<StatementTree> = Vec::new();

        struct Renamer<'a> {
            codegen: &'a CodeGen,
            disjunction_branch_num: usize,
            invariant_violators: &'a HashMap<Ident, PedersenAssignment>,
            ids_renamed: &'a mut HashSet<Ident>,
        }

        impl<'a> VisitMut for Renamer<'a> {
            fn visit_expr_mut(&mut self, node: &mut Expr) {
                if let Expr::Path(expath) = node {
                    if let Some(id) = expath.path.get_ident() {
                        if self.invariant_violators.contains_key(id) {
                            let replacement_ident = self.codegen.gen_ident(&format_ident!(
                                "disj{}_{}",
                                self.disjunction_branch_num,
                                id
                            ));
                            self.ids_renamed.insert(id.clone());
                            *node = parse_quote! { #replacement_ident };
                            return;
                        }
                    }
                }
                // Unless we bailed out above, continue with the default
                // traversal
                visit_mut::visit_expr_mut(self, node);
            }
        }
        let mut renamer = Renamer {
            codegen,
            disjunction_branch_num,
            invariant_violators: &invariant_violator_pedersens,
            ids_renamed: &mut ids_renamed,
        };

        branch.for_each_disjunction_branch_leaf(&mut |leaf| {
            let StatementTree::Leaf(ref mut leafexpr) = leaf else {
                panic!(
                    "Should not happen: leaf {:?} is not a StatementTree::Leaf",
                    leaf
                );
            };
            renamer.visit_expr_mut(leafexpr);
            Ok(())
        })?;

        // For each id we renamed, insert a Pedersen commitment to the
        // new name (using the _same_ commitment value we computed in
        // the root Pedersen commitment) into this disjunction branch.
        // This binds the new name to the old name.
        for id in ids_renamed {
            // Is it a vector variable?
            let is_vec = if let Some(TaggedIdent::Scalar(TaggedScalar { is_vec, .. })) =
                vars.get(&id.to_string())
            {
                *is_vec
            } else {
                false
            };

            // Variables for the renamed private Scalar and the randomness
            let id_var = codegen.gen_scalar(
                vars,
                &format_ident!("disj{}_{}", disjunction_branch_num, id,),
                false,  // is_rand
                is_vec, // is_vec
            );
            let rand_var = codegen.gen_scalar(
                vars,
                &format_ident!("disj{}_{}_r", disjunction_branch_num, id,),
                true,   // is_rand
                is_vec, // is_vec
            );
            let root_commitment_var = codegen.gen_ident(&format_ident!("disj_{}_C", id));
            let root_rand_var = codegen.gen_ident(&format_ident!("disj_{}_r", id));
            if is_vec {
                codegen.prove_append(quote! {
                    let #id_var = #id.clone();
                    let #rand_var = #root_rand_var.clone();
                });
            } else {
                codegen.prove_append(quote! {
                    let #id_var = #id;
                    let #rand_var = #root_rand_var;
                });
            }
            // The generators for the Pedersen commitment for this id
            let ped_assign = invariant_violator_pedersens.get(&id).unwrap();
            let var_generator = &ped_assign.pedersen.var_term.id;
            let rand_generator = &ped_assign.pedersen.rand_term.id;

            branch_extra_statements.push(StatementTree::Leaf(parse_quote! {
                #root_commitment_var = #id_var * #var_generator + #rand_var * #rand_generator
            }));
        }

        // Now add the branch_extra_statements to the top node of this
        // disjunction branch.  If it's already an And node, just add
        // them to the vector.  Otherwise, make a new And node
        // containing the old node and the branch_extra_statements.
        if let StatementTree::And(ref mut stvec) = branch {
            stvec.append(&mut branch_extra_statements);
        } else {
            let old_branch = std::mem::replace(branch, StatementTree::leaf_true());
            branch_extra_statements.push(old_branch);
            *branch = StatementTree::And(branch_extra_statements);
        }

        Ok(())
    })?;

    // Add the root_extra_statements to the root of the StatementTree.
    // If it's already an And node, just add them to the vector.
    // Otherwise, make a new And node containing the old root and the
    // root_extra_statements
    if let StatementTree::And(ref mut stvec) = st {
        stvec.append(&mut root_extra_statements);
    } else {
        let old_st = std::mem::replace(st, StatementTree::leaf_true());
        root_extra_statements.push(old_st);
        *st = StatementTree::And(root_extra_statements);
    }

    // Sanity check
    st.check_disjunction_invariant(&vardict)
}

#[cfg(test)]
mod tests {
    use super::super::syntax::taggedvardict_from_strs;
    use super::*;

    fn prune_tester(e: Expr, pruned_e: Expr) {
        let mut st = StatementTree::parse(&e).unwrap();
        prune_statement_tree(&mut st);
        assert_eq!(st, StatementTree::parse(&pruned_e).unwrap());
    }

    #[test]
    fn prune_statement_tree_test() {
        prune_tester(
            parse_quote! {
                AND (
                    true,
                    e = f,
                )
            },
            parse_quote! {
                e = f
            },
        );
        prune_tester(
            parse_quote! {
                AND (
                    e = f,
                    true,
                )
            },
            parse_quote! {
                e = f
            },
        );
        prune_tester(
            parse_quote! {
                AND (
                    e = f,
                    true,
                    b = c,
                )
            },
            parse_quote! {
                AND (
                    e = f,
                    b = c,
                )
            },
        );
        prune_tester(
            parse_quote! {
                OR (
                    true,
                    e = f,
                )
            },
            parse_quote! {
                true
            },
        );
        prune_tester(
            parse_quote! {
                AND (
                    a = b,
                    true,
                    OR (
                        c = d,
                        true,
                        e = f
                    )
                )
            },
            parse_quote! {
                a = b
            },
        );
        prune_tester(
            parse_quote! {
                THRESH (3,
                    a = b,
                    true,
                    THRESH (1,
                        c = d,
                        true,
                        e = f
                    )
                )
            },
            parse_quote! {
                a = b
            },
        );
        prune_tester(
            parse_quote! {
                THRESH (3,
                    a = b,
                    true,
                    THRESH (2,
                        c = d,
                        true,
                        e = f
                    )
                )
            },
            parse_quote! {
                THRESH (2,
                    a = b,
                    THRESH (1,
                        c = d,
                        e = f
                    )
                )
            },
        );
    }

    fn enforce_disjunction_invariant_tester(vars: (&[&str], &[&str]), e: Expr, expect: Expr) {
        let mut codegen = CodeGen::new_empty();
        let mut st = StatementTree::parse(&e).unwrap();
        let mut vars = taggedvardict_from_strs(vars);
        enforce_disjunction_invariant(&mut codegen, &mut st, &mut vars).unwrap();
        assert_eq!(st, StatementTree::parse(&expect).unwrap());
    }

    #[test]
    fn enforce_disjunction_invariant_test() {
        let vars = (
            [
                "x", "y", "z", "pub a", "pub b", "pub c", "rand r", "rand s", "rand t",
            ]
            .as_slice(),
            ["C", "D", "cind A", "cind B"].as_slice(),
        );

        enforce_disjunction_invariant_tester(
            vars,
            parse_quote! {
                C = x*A
            },
            parse_quote! {
                C = x*A
            },
        );

        enforce_disjunction_invariant_tester(
            vars,
            parse_quote! {
                AND (
                    C = x*A + r*B,
                    OR (
                        y=1,
                        z=2,
                    )
                )
            },
            parse_quote! {
                AND (
                    C = x*A + r*B,
                    OR (
                        y=1,
                        z=2,
                    )
                )
            },
        );

        enforce_disjunction_invariant_tester(
            vars,
            parse_quote! {
                AND (
                    C = x*A + r*B,
                    OR (
                        x=1,
                        x=2,
                    )
                )
            },
            parse_quote! {
                AND (
                    C = x*A + r*B,
                    OR (
                        AND (
                            gen__disj_x_C = gen__disj1_x * A + gen__disj1_x_r * B,
                            gen__disj1_x=1,
                        ),
                        AND (
                            gen__disj_x_C = gen__disj2_x * A + gen__disj2_x_r * B,
                            gen__disj2_x=2,
                        ),
                    )
                )
            },
        );

        enforce_disjunction_invariant_tester(
            vars,
            parse_quote! {
                AND (
                    C = x*A,
                    OR (
                        x=1,
                        x=2,
                    )
                )
            },
            parse_quote! {
                AND (
                    C = x*A,
                    OR (
                        AND (
                            gen__disj_x_C = gen__disj1_x * A + gen__disj1_x_r * B,
                            gen__disj1_x=1,
                        ),
                        AND (
                            gen__disj_x_C = gen__disj2_x * A + gen__disj2_x_r * B,
                            gen__disj2_x=2,
                        ),
                    ),
                    gen__disj_x_genC = x*A + gen__disj_x_genr*B,
                )
            },
        );

        enforce_disjunction_invariant_tester(
            vars,
            parse_quote! {
                OR (
                    x=1,
                    x=2,
                )
            },
            parse_quote! {
                AND (
                    gen__disj_x_genC = x*A + gen__disj_x_genr*B,
                    OR (
                        AND (
                            gen__disj_x_C = gen__disj1_x * A + gen__disj1_x_r * B,
                            gen__disj1_x=1,
                        ),
                        AND (
                            gen__disj_x_C = gen__disj2_x * A + gen__disj2_x_r * B,
                            gen__disj2_x=2,
                        ),
                    ),
                )
            },
        );
    }
}

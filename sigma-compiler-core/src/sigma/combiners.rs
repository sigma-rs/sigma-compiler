//! This module creates and manipulates trees of basic statements
//! combined with `AND`, `OR`, and `THRESH`.

use super::types::*;
use quote::quote;
use std::collections::{HashMap, HashSet};
use syn::parse::Result;
use syn::visit::Visit;
use syn::{parse_quote, Expr, Ident};

/// For each [`Ident`](struct@syn::Ident) representing a private
/// `Scalar` (as listed in a [`VarDict`]) that appears in an [`Expr`],
/// call a given closure.
pub struct PrivScalarMap<'a> {
    /// The [`VarDict`] that maps variable names to their types
    pub vars: &'a VarDict,

    /// The closure that is called for each [`Ident`](struct@syn::Ident)
    /// found in the [`Expr`] (provided in the call to
    /// [`visit_expr`](PrivScalarMap::visit_expr)) that represents a
    /// private `Scalar`
    pub closure: &'a mut dyn FnMut(&syn::Ident) -> Result<()>,

    /// The accumulated result.  This will be the first
    /// [`Err`](Result::Err) returned from the closure, or
    /// [`Ok(())`](Result::Ok) if all calls to the closure succeeded.
    pub result: Result<()>,
}

impl<'a> Visit<'a> for PrivScalarMap<'a> {
    fn visit_path(&mut self, path: &'a syn::Path) {
        // Whenever we see a `Path`, check first if it's just a bare
        // `Ident`
        let Some(id) = path.get_ident() else {
            return;
        };
        // Then check if that `Ident` appears in the `VarDict`
        let Some(vartype) = self.vars.get(&id.to_string()) else {
            return;
        };
        // If so, and the `Ident` represents a private Scalar,
        // call the closure if we haven't seen an `Err` returned from
        // the closure yet.
        if let AExprType::Scalar { is_pub: false, .. } = vartype {
            if self.result.is_ok() {
                self.result = (self.closure)(id);
            }
        }
    }
}

/// The statements in the ZKP form a tree.  The leaves are basic
/// statements of various kinds; for example, equations or inequalities
/// about Scalars and Points.  The interior nodes are combiners: `And`,
/// `Or`, or `Thresh` (with a given constant threshold).  A leaf is true
/// if the basic statement it contains is true.  An `And` node is true
/// if all of its children are true.  An `Or` node is true if at least
/// one of its children is true.  A `Thresh` node (with threshold `k`) is
/// true if at least `k` of its children are true.

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatementTree {
    Leaf(Expr),
    And(Vec<StatementTree>),
    Or(Vec<StatementTree>),
    Thresh(usize, Vec<StatementTree>),
}

impl StatementTree {
    #[cfg(not(doctest))]
    /// Parse an [`Expr`] (which may contain nested `AND`, `OR`, or
    /// `THRESH`) into a [`StatementTree`].  For example, the
    /// [`Expr`] obtained from:
    /// ```
    /// parse_quote! {
    ///    AND (
    ///        C = c*B + r*A,
    ///        D = d*B + s*A,
    ///        OR (
    ///            AND (
    ///                C = c0*B + r0*A,
    ///                D = d0*B + s0*A,
    ///                c0 = d0,
    ///            ),
    ///            AND (
    ///                C = c1*B + r1*A,
    ///                D = d1*B + s1*A,
    ///                c1 = d1 + 1,
    ///            ),
    ///        )
    ///    )
    /// }
    /// ```
    ///
    /// would yield a [`StatementTree::And`] containing a 3-element
    /// vector.  The first two elements are [`StatementTree::Leaf`], and
    /// the third is [`StatementTree::Or`] containing a 2-element
    /// vector.  Each element is an [`StatementTree::And`] with a vector
    /// containing 3 [`StatementTree::Leaf`]s.
    ///
    /// Note that `AND`, `OR`, and `THRESH` in the expression are
    /// case-insensitive.
    pub fn parse(expr: &Expr) -> Result<Self> {
        // See if the expression describes a combiner
        if let Expr::Call(syn::ExprCall { func, args, .. }) = expr {
            if let Expr::Path(syn::ExprPath { path, .. }) = func.as_ref() {
                if let Some(funcname) = path.get_ident() {
                    match funcname.to_string().to_lowercase().as_str() {
                        "and" => {
                            let children: Result<Vec<StatementTree>> =
                                args.iter().map(Self::parse).collect();
                            return Ok(Self::And(children?));
                        }
                        "or" => {
                            let children: Result<Vec<StatementTree>> =
                                args.iter().map(Self::parse).collect();
                            return Ok(Self::Or(children?));
                        }
                        "thresh" => {
                            if let Some(Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Int(litint),
                                ..
                            })) = args.first()
                            {
                                let thresh = litint.base10_parse::<usize>()?;
                                // Remember that args.len() is one more
                                // than the number of expressions,
                                // because the first arg is the
                                // threshold
                                if thresh < 1 || thresh >= args.len() {
                                    return Err(syn::Error::new(
                                        litint.span(),
                                        "threshold out of range",
                                    ));
                                }
                                let children: Result<Vec<StatementTree>> =
                                    args.iter().skip(1).map(Self::parse).collect();
                                return Ok(Self::Thresh(thresh, children?));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(StatementTree::Leaf(expr.clone()))
    }

    /// A convenience function that takes a list of [`Expr`]s, and
    /// returns the [`StatementTree`] that implicitly puts `AND` around
    /// the [`Expr`]s.  This is useful because a common thing to do is
    /// to just write a list of [`Expr`]s in the top-level macro
    /// invocation, having the semantics of "all of these must be true".
    pub fn parse_andlist(exprlist: &[Expr]) -> Result<Self> {
        let children: Result<Vec<StatementTree>> = exprlist.iter().map(Self::parse).collect();
        Ok(StatementTree::And(children?))
    }

    /// Return a vector of references to all of the leaf expressions in
    /// the [`StatementTree`]
    pub fn leaves(&self) -> Vec<&Expr> {
        match self {
            StatementTree::Leaf(ref e) => vec![e],
            StatementTree::And(v) | StatementTree::Or(v) | StatementTree::Thresh(_, v) => {
                v.iter().fold(Vec::<&Expr>::new(), |mut b, st| {
                    b.extend(st.leaves());
                    b
                })
            }
        }
    }

    /// Return a vector of mutable references to all of the leaf
    /// expressions in the [`StatementTree`]
    pub fn leaves_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            StatementTree::Leaf(ref mut e) => vec![e],
            StatementTree::And(v) | StatementTree::Or(v) | StatementTree::Thresh(_, v) => {
                v.iter_mut().fold(Vec::<&mut Expr>::new(), |mut b, st| {
                    b.extend(st.leaves_mut());
                    b
                })
            }
        }
    }

    /// Return a vector of mutable references to all of the leaves in
    /// the [`StatementTree`]
    pub fn leaves_st_mut(&mut self) -> Vec<&mut StatementTree> {
        match self {
            StatementTree::Leaf(_) => vec![self],
            StatementTree::And(v) | StatementTree::Or(v) | StatementTree::Thresh(_, v) => v
                .iter_mut()
                .fold(Vec::<&mut StatementTree>::new(), |mut b, st| {
                    b.extend(st.leaves_st_mut());
                    b
                }),
        }
    }

    #[cfg(not(doctest))]
    /// Verify whether the [`StatementTree`] satisfies the disjunction
    /// invariant.
    ///
    /// A _disjunction node_ is an [`Or`](StatementTree::Or) or
    /// [`Thresh`](StatementTree::Thresh) node in the [`StatementTree`].
    ///
    /// A _disjunction branch_ is a subtree rooted at a non-disjunction
    /// node that is the child of a disjunction node or at the root of
    /// the [`StatementTree`].
    ///
    /// The _disjunction invariant_ is that a private variable (which is
    /// necessarily a `Scalar` since there are no private `Point`
    /// variables) that appears in a disjunction branch cannot also
    /// appear outside of that disjunction branch.
    ///
    /// For example, if all of the lowercase variables are private
    /// `Scalar`s, the [`StatementTree`] created from:
    ///
    /// ```
    ///    AND (
    ///        C = c*B + r*A,
    ///        D = d*B + s*A,
    ///        OR (
    ///            AND (
    ///                C = c0*B + r0*A,
    ///                D = d0*B + s0*A,
    ///                c0 = d0,
    ///            ),
    ///            AND (
    ///                C = c1*B + r1*A,
    ///                D = d1*B + s1*A,
    ///                c1 = d1 + 1,
    ///            ),
    ///        )
    ///    )
    /// ```
    ///
    /// satisfies the disjunction invariant, but
    ///
    /// ```
    ///    AND (
    ///        C = c*B + r*A,
    ///        D = d*B + s*A,
    ///        OR (
    ///            AND (
    ///                D = d0*B + s0*A,
    ///                c = d0,
    ///            ),
    ///            AND (
    ///                C = c1*B + r1*A,
    ///                D = d1*B + s1*A,
    ///                c1 = d1 + 1,
    ///            ),
    ///        )
    ///    )
    /// ```
    ///
    /// does not, because `c` appears in the first child of the `OR` and
    /// also outside of the `OR` entirely.  Indeed, the reason to write
    /// the first expression above rather than the more natural
    ///
    /// ```
    ///    AND (
    ///        C = c*B + r*A,
    ///        D = d*B + s*A,
    ///        OR (
    ///            c = d,
    ///            c = d + 1,
    ///        )
    ///    )
    /// ```
    ///
    /// is exactly that the invariant must be satisfied.
    ///
    /// If you don't know that your [`StatementTree`] already satisfies
    /// the invariant, call
    /// [`enforce_disjunction_invariant`](super::super::enforce_disjunction_invariant),
    /// which will transform the [`StatementTree`] so that it does (and
    /// also call this
    /// [`check_disjunction_invariant`](StatementTree::check_disjunction_invariant)
    /// function as a sanity check).
    pub fn check_disjunction_invariant(&self, vars: &VarDict) -> Result<()> {
        let mut disjunct_map: HashMap<String, usize> = HashMap::new();

        // If the recursive call returns Err, return that Err.
        // Otherwise, we don't care about the Ok(usize) returned, so
        // just return Ok(())
        self.check_disjunction_invariant_rec(vars, &mut disjunct_map, 0, 0)?;
        Ok(())
    }

    /// Internal recursive helper for
    /// [`check_disjunction_invariant`](StatementTree::check_disjunction_invariant).
    ///
    /// The `disjunct_map` is a [`HashMap`] that maps the names of
    /// variables to an identifier of which child of a disjunction node
    /// the variable appears in (or the root if none).  In the case of
    /// nested disjunction node, the closest one to the leaf is what
    /// matters.  Nodes are numbered in pre-order fashion, starting at 0
    /// for the root, 1 for the first child of the root, 2 for the first
    /// child of node 1, etc.  `cur_node` is the node id of `self`, and
    /// `cur_disjunct_child` is the node id of the closest child of a
    /// disjunction node (or 0 for the root if none).  Returns the next
    /// node id to use in the preorder traversal.
    fn check_disjunction_invariant_rec(
        &self,
        vars: &VarDict,
        disjunct_map: &mut HashMap<String, usize>,
        cur_node: usize,
        cur_disjunct_child: usize,
    ) -> Result<usize> {
        let mut next_node = cur_node;
        match self {
            Self::And(v) => {
                for st in v {
                    next_node = st.check_disjunction_invariant_rec(
                        vars,
                        disjunct_map,
                        next_node + 1,
                        cur_disjunct_child,
                    )?;
                }
            }
            Self::Or(v) | Self::Thresh(_, v) => {
                for st in v {
                    next_node = st.check_disjunction_invariant_rec(
                        vars,
                        disjunct_map,
                        next_node + 1,
                        next_node + 1,
                    )?;
                }
            }
            Self::Leaf(e) => {
                let mut psmap = PrivScalarMap {
                    vars,
                    closure: &mut |ident| {
                        let varname = ident.to_string();
                        if let Some(dis_id) = disjunct_map.get(&varname) {
                            if *dis_id != cur_disjunct_child {
                                return Err(syn::Error::new(
                                    ident.span(),
                                    "Disjunction invariant violation: a private variable cannot appear both inside and outside a single term of an OR or THRESH"));
                            }
                        } else {
                            disjunct_map.insert(varname, cur_disjunct_child);
                        }
                        Ok(())
                    },
                    result: Ok(()),
                };
                psmap.visit_expr(e);
                psmap.result?;
            }
        }
        Ok(next_node)
    }

    /// Call the supplied closure for each [disjunction branch] of the
    /// given [`StatementTree`] (including the root, if the root is a
    /// non-disjunction node).
    ///
    /// The calls are in preorder traversal (parents before children).
    /// The given `closure` will be called with the root of each
    /// [disjunction branch] as well as a slice of [`usize`] indicating
    /// the path through the [`StatementTree`] to that disjunction
    /// branch.  The disjunction branch at the root has path `[]`.
    /// The disjunction branch rooted at, say, the 2nd child of an `Or`
    /// node in the root disjunction branch will have path `[2]`.  The
    /// disjunction branch rooted at the 1st child of an `Or` node in
    /// that disjunction branch will have path `[2,1]`, and so on.
    ///
    /// Abort and return `Err` if any call to the closure returns `Err`.
    ///
    /// [disjunction branch]: StatementTree::check_disjunction_invariant
    pub fn for_each_disjunction_branch(
        &mut self,
        closure: &mut dyn FnMut(&mut StatementTree, &[usize]) -> Result<()>,
    ) -> Result<()> {
        let mut path: Vec<usize> = Vec::new();
        self.for_each_disjunction_branch_rec(closure, &mut path, 0, true)?;
        Ok(())
    }

    /// Internal recursive helper for
    /// [`for_each_disjunction_branch`](StatementTree::for_each_disjunction_branch).
    ///
    ///   - `path` is the path to this disjunction branch
    ///   - `last_index` is the last index used for a child of this
    ///     disjunction branch
    ///   - `is_new_branch` is `true` if this node is the start of a new
    ///     disjunction branch
    ///
    /// The return value (if `Ok`) is the updated value of `last_index`.
    fn for_each_disjunction_branch_rec(
        &mut self,
        closure: &mut dyn FnMut(&mut StatementTree, &[usize]) -> Result<()>,
        path: &mut Vec<usize>,
        mut last_index: usize,
        is_new_branch: bool,
    ) -> Result<usize> {
        // We're starting a new branch (and should call the closure) if
        // and only if both is_new_branch is true, and also we're at a
        // non-disjunction node
        match self {
            StatementTree::Leaf(_) | StatementTree::And(_) => {
                if is_new_branch {
                    (closure)(self, path)?;
                }
            }
            _ => {}
        }
        match self {
            StatementTree::Leaf(_) => {}
            StatementTree::And(stvec) => {
                stvec.iter_mut().try_for_each(|st| -> Result<()> {
                    last_index =
                        st.for_each_disjunction_branch_rec(closure, path, last_index, false)?;
                    Ok(())
                })?;
            }
            StatementTree::Or(stvec) | StatementTree::Thresh(_, stvec) => {
                path.push(last_index);
                let pathlen = path.len();
                stvec.iter_mut().try_for_each(|st| -> Result<()> {
                    last_index += 1;
                    path[pathlen - 1] = last_index;
                    st.for_each_disjunction_branch_rec(closure, path, 0, true)?;
                    Ok(())
                })?;
                path.pop();
            }
        }
        Ok(last_index)
    }

    /// Call the supplied closure for each [`StatementTree::Leaf`] of
    /// the given [disjunction branch].
    ///
    /// Abort and return `Err` if any call to the closure returns `Err`.
    ///
    /// [disjunction branch]: StatementTree::check_disjunction_invariant
    pub fn for_each_disjunction_branch_leaf(
        &mut self,
        closure: &mut dyn FnMut(&mut StatementTree) -> Result<()>,
    ) -> Result<()> {
        match self {
            StatementTree::Leaf(_) => {
                (closure)(self)?;
            }
            StatementTree::And(stvec) => {
                stvec
                    .iter_mut()
                    .try_for_each(|st| st.for_each_disjunction_branch_leaf(closure))?;
            }
            StatementTree::Or(_) | StatementTree::Thresh(_, _) => {
                // Don't recurse into Or or Thresh nodes, since the
                // children of those nodes are in different disjunction
                // branches.
            }
        }
        Ok(())
    }

    /// Produce a [`HashSet`] of the private Scalars that appear in any
    /// leaf of the given [disjunction branch].
    ///
    /// [disjunction branch]: StatementTree::check_disjunction_invariant
    pub fn disjunction_branch_priv_scalars(&mut self, vars: &VarDict) -> HashSet<Ident> {
        let mut priv_scalars: HashSet<Ident> = HashSet::new();
        self.for_each_disjunction_branch_leaf(&mut |leaf| {
            if let StatementTree::Leaf(leafexpr) = leaf {
                let mut psmap = PrivScalarMap {
                    vars,
                    closure: &mut |ident| {
                        priv_scalars.insert(ident.clone());
                        Ok(())
                    },
                    result: Ok(()),
                };
                psmap.visit_expr(leafexpr);
            }
            Ok(())
        })
        .unwrap();
        priv_scalars
    }

    #[cfg(not(doctest))]
    /// Flatten nested `And` nodes in a [`StatementTree`].
    ///
    /// The underlying `sigma-proofs` crate can share `Scalars` across
    /// statements that are direct children of the same `And` node, but
    /// not in nested `And` nodes.
    ///
    /// So a [`StatementTree`] like this:
    ///
    /// ```
    ///    AND (
    ///        C = x*B + r*A,
    ///        AND (
    ///            D = x*B + s*A,
    ///            E = x*B + t*A,
    ///        ),
    ///    )
    /// ```
    ///
    /// Needs to be flattened to:
    ///
    /// ```
    ///    AND (
    ///        C = x*B + r*A,
    ///        D = x*B + s*A,
    ///        E = x*B + t*A,
    ///    )
    /// ```
    pub fn flatten_ands(&mut self) {
        match self {
            StatementTree::Leaf(_) => {}
            StatementTree::Or(svec) | StatementTree::Thresh(_, svec) => {
                // Flatten each child
                svec.iter_mut().for_each(|st| st.flatten_ands());
            }
            StatementTree::And(svec) => {
                // Flatten each child, and if any of the children are
                // `And`s, replace that child with the list of its
                // children
                let old_svec = std::mem::take(svec);
                let mut new_svec: Vec<StatementTree> = Vec::new();
                for mut st in old_svec {
                    st.flatten_ands();
                    match st {
                        StatementTree::And(mut child_svec) => {
                            new_svec.append(&mut child_svec);
                        }
                        _ => {
                            new_svec.push(st);
                        }
                    }
                }
                *self = StatementTree::And(new_svec);
            }
        }
    }

    /// Produce a [`StatementTree`] that represents the constant `true`
    pub fn leaf_true() -> StatementTree {
        StatementTree::Leaf(parse_quote! { true })
    }

    /// Test if the given [`StatementTree`] represents the constant `true`
    pub fn is_leaf_true(&self) -> bool {
        if let StatementTree::Leaf(Expr::Lit(exprlit)) = self {
            if let syn::Lit::Bool(syn::LitBool { value: true, .. }) = exprlit.lit {
                return true;
            }
        }
        false
    }

    fn dump_int(&self, depth: usize) {
        match self {
            StatementTree::Leaf(e) => {
                println!(
                    "{:1$}{2},",
                    "",
                    depth * 2,
                    quote! { #e }.to_string().replace('\n', " ")
                )
            }
            StatementTree::And(v) => {
                println!("{:1$}And (", "", depth * 2);
                v.iter().for_each(|n| n.dump_int(depth + 1));
                println!("{:1$})", "", depth * 2);
            }
            StatementTree::Or(v) => {
                println!("{:1$}Or (", "", depth * 2);
                v.iter().for_each(|n| n.dump_int(depth + 1));
                println!("{:1$})", "", depth * 2);
            }
            StatementTree::Thresh(thresh, v) => {
                println!("{:1$}Thresh ({2}", "", depth * 2, thresh);
                v.iter().for_each(|n| n.dump_int(depth + 1));
                println!("{:1$})", "", depth * 2);
            }
        }
    }

    pub fn dump(&self) {
        self.dump_int(0);
    }
}

#[cfg(test)]
mod test {
    use super::StatementTree::*;
    use super::*;
    use quote::quote;

    #[test]
    fn leaf_true_test() {
        assert!(StatementTree::leaf_true().is_leaf_true());
        assert!(!StatementTree::Leaf(parse_quote! { false }).is_leaf_true());
        assert!(!StatementTree::Leaf(parse_quote! { 1 }).is_leaf_true());
        assert!(!StatementTree::parse(&parse_quote! {
            OR(1=1, a=b)
        })
        .unwrap()
        .is_leaf_true());
    }

    #[test]
    fn combiners_simple_test() {
        let exprlist: Vec<Expr> = vec![
            parse_quote! { C = c*B + r*A },
            parse_quote! { D = d*B + s*A },
            parse_quote! { c = d },
        ];

        let statementtree = StatementTree::parse_andlist(&exprlist).unwrap();
        let And(v) = statementtree else {
            panic!("Incorrect result");
        };
        let [Leaf(l0), Leaf(l1), Leaf(l2)] = v.as_slice() else {
            panic!("Incorrect result");
        };
        assert_eq!(quote! {#l0}.to_string(), "C = c * B + r * A");
        assert_eq!(quote! {#l1}.to_string(), "D = d * B + s * A");
        assert_eq!(quote! {#l2}.to_string(), "c = d");
    }

    #[test]
    fn combiners_nested_test() {
        let exprlist: Vec<Expr> = vec![
            parse_quote! { C = c*B + r*A },
            parse_quote! { D = d*B + s*A },
            parse_quote! {
            OR (
                AND (
                    C = c0*B + r0*A,
                    D = d0*B + s0*A,
                    c0 = d0,
                ),
                AND (
                    C = c1*B + r1*A,
                    D = d1*B + s1*A,
                    c1 = d1 + 1,
                ),
            ) },
        ];

        let statementtree = StatementTree::parse_andlist(&exprlist).unwrap();
        let And(v0) = statementtree else {
            panic!("Incorrect result");
        };
        let [Leaf(l0), Leaf(l1), Or(v1)] = v0.as_slice() else {
            panic!("Incorrect result");
        };
        assert_eq!(quote! {#l0}.to_string(), "C = c * B + r * A");
        assert_eq!(quote! {#l1}.to_string(), "D = d * B + s * A");
        let [And(v2), And(v3)] = v1.as_slice() else {
            panic!("Incorrect result");
        };
        let [Leaf(l20), Leaf(l21), Leaf(l22)] = v2.as_slice() else {
            panic!("Incorrect result");
        };
        assert_eq!(quote! {#l20}.to_string(), "C = c0 * B + r0 * A");
        assert_eq!(quote! {#l21}.to_string(), "D = d0 * B + s0 * A");
        assert_eq!(quote! {#l22}.to_string(), "c0 = d0");
        let [Leaf(l30), Leaf(l31), Leaf(l32)] = v3.as_slice() else {
            panic!("Incorrect result");
        };
        assert_eq!(quote! {#l30}.to_string(), "C = c1 * B + r1 * A");
        assert_eq!(quote! {#l31}.to_string(), "D = d1 * B + s1 * A");
        assert_eq!(quote! {#l32}.to_string(), "c1 = d1 + 1");
    }

    #[test]
    fn combiners_thresh_test() {
        let exprlist: Vec<Expr> = vec![
            parse_quote! { C = c*B + r*A },
            parse_quote! { D = d*B + s*A },
            parse_quote! {
            THRESH (1,
                AND (
                    C = c0*B + r0*A,
                    D = d0*B + s0*A,
                    c0 = d0,
                ),
                AND (
                    C = c1*B + r1*A,
                    D = d1*B + s1*A,
                    c1 = d1 + 1,
                ),
            ) },
        ];

        let statementtree = StatementTree::parse_andlist(&exprlist).unwrap();
        let And(v0) = statementtree else {
            panic!("Incorrect result");
        };
        let [Leaf(l0), Leaf(l1), Thresh(thresh, v1)] = v0.as_slice() else {
            panic!("Incorrect result");
        };
        assert_eq!(*thresh, 1);
        assert_eq!(quote! {#l0}.to_string(), "C = c * B + r * A");
        assert_eq!(quote! {#l1}.to_string(), "D = d * B + s * A");
        let [And(v2), And(v3)] = v1.as_slice() else {
            panic!("Incorrect result");
        };
        let [Leaf(l20), Leaf(l21), Leaf(l22)] = v2.as_slice() else {
            panic!("Incorrect result");
        };
        assert_eq!(quote! {#l20}.to_string(), "C = c0 * B + r0 * A");
        assert_eq!(quote! {#l21}.to_string(), "D = d0 * B + s0 * A");
        assert_eq!(quote! {#l22}.to_string(), "c0 = d0");
        let [Leaf(l30), Leaf(l31), Leaf(l32)] = v3.as_slice() else {
            panic!("Incorrect result");
        };
        assert_eq!(quote! {#l30}.to_string(), "C = c1 * B + r1 * A");
        assert_eq!(quote! {#l31}.to_string(), "D = d1 * B + s1 * A");
        assert_eq!(quote! {#l32}.to_string(), "c1 = d1 + 1");
    }

    #[test]
    #[should_panic]
    fn combiners_bad_thresh_test() {
        // The threshold is out of range
        let exprlist: Vec<Expr> = vec![
            parse_quote! { C = c*B + r*A },
            parse_quote! { D = d*B + s*A },
            parse_quote! {
            THRESH (3,
                AND (
                    C = c0*B + r0*A,
                    D = d0*B + s0*A,
                    c0 = d0,
                ),
                AND (
                    C = c1*B + r1*A,
                    D = d1*B + s1*A,
                    c1 = d1 + 1,
                ),
            ) },
        ];

        StatementTree::parse_andlist(&exprlist).unwrap();
    }

    #[test]
    // Test the disjunction invariant checker
    fn disjunction_invariant_test() {
        let vars: VarDict = vardict_from_strs(&[
            ("c", "S"),
            ("d", "S"),
            ("c0", "S"),
            ("c1", "S"),
            ("d0", "S"),
            ("d1", "S"),
            ("A", "pP"),
            ("B", "pP"),
            ("C", "pP"),
            ("D", "pP"),
        ]);
        // This one is OK
        let st_ok = StatementTree::parse(&parse_quote! {
           AND (
               C = c*B + r*A,
               D = d*B + s*A,
               OR (
                   AND (
                       C = c0*B + r0*A,
                       D = d0*B + s0*A,
                       c0 = d0,
                   ),
                   AND (
                       C = c1*B + r1*A,
                       D = d1*B + s1*A,
                       c1 = d1 + 1,
                   ),
               )
           )
        })
        .unwrap();
        // not OK: c0 appears in two branches of the OR
        let st_nok1 = StatementTree::parse(&parse_quote! {
           AND (
               C = c*B + r*A,
               D = d*B + s*A,
               OR (
                   AND (
                       C = c0*B + r0*A,
                       D = d0*B + s0*A,
                       c0 = d0,
                   ),
                   AND (
                       C = c0*B + r0*A,
                       D = d1*B + s1*A,
                       c0 = d1 + 1,
                   ),
               )
           )
        })
        .unwrap();
        // not OK: c appears in one branch of the OR and also outside
        // the OR
        let st_nok2 = StatementTree::parse(&parse_quote! {
           AND (
               C = c*B + r*A,
               D = d*B + s*A,
               OR (
                   AND (
                       D = d0*B + s0*A,
                       c = d0,
                   ),
                   AND (
                       C = c1*B + r1*A,
                       D = d1*B + s1*A,
                       c1 = d1 + 1,
                   ),
               )
           )
        })
        .unwrap();
        // not OK: c and d appear in both branches of the OR, and also
        // outside it
        let st_nok3 = StatementTree::parse(&parse_quote! {
           AND (
               C = c*B + r*A,
               D = d*B + s*A,
               OR (
                   c = d,
                   c = d + 1,
               )
           )
        })
        .unwrap();
        st_ok.check_disjunction_invariant(&vars).unwrap();
        st_nok1.check_disjunction_invariant(&vars).unwrap_err();
        st_nok2.check_disjunction_invariant(&vars).unwrap_err();
        st_nok3.check_disjunction_invariant(&vars).unwrap_err();
    }

    fn disjunction_branch_tester(e: Expr, expected: Vec<(Vec<usize>, Expr)>) {
        let mut output: Vec<(Vec<usize>, StatementTree)> = Vec::new();
        let expected_st: Vec<(Vec<usize>, StatementTree)> = expected
            .iter()
            .map(|(path, ex)| (path.clone(), StatementTree::parse(ex).unwrap()))
            .collect();
        let mut st = StatementTree::parse(&e).unwrap();
        st.for_each_disjunction_branch(&mut |db, path| {
            output.push((path.to_vec(), db.clone()));
            Ok(())
        })
        .unwrap();
        assert_eq!(output, expected_st);
    }

    fn disjunction_branch_abort_tester(e: Expr, expected: Vec<(Vec<usize>, Expr)>) {
        let mut output: Vec<(Vec<usize>, StatementTree)> = Vec::new();
        let expected_st: Vec<(Vec<usize>, StatementTree)> = expected
            .iter()
            .map(|(path, ex)| (path.clone(), StatementTree::parse(ex).unwrap()))
            .collect();
        let mut st = StatementTree::parse(&e).unwrap();
        st.for_each_disjunction_branch(&mut |st, path| {
            if st.is_leaf_true() {
                return Err(syn::Error::new(proc_macro2::Span::call_site(), "true leaf"));
            }
            output.push((path.to_vec(), st.clone()));
            Ok(())
        })
        .unwrap_err();
        assert_eq!(output, expected_st);
    }

    #[test]
    fn disjunction_branch_test() {
        disjunction_branch_tester(
            parse_quote! {
                C = c*B + r*A
            },
            vec![(
                vec![],
                parse_quote! {
                    C = c*B + r*A
                },
            )],
        );

        disjunction_branch_tester(
            parse_quote! {
               AND (
                   C = c*B + r*A,
                   D = d*B + s*A,
                   OR (
                       c = d,
                       c = d + 1,
                   )
               )
            },
            vec![
                (
                    vec![],
                    parse_quote! {
                       AND (
                           C = c*B + r*A,
                           D = d*B + s*A,
                           OR (
                               c = d,
                               c = d + 1,
                           )
                       )
                    },
                ),
                (
                    vec![1],
                    parse_quote! {
                        c = d
                    },
                ),
                (
                    vec![2],
                    parse_quote! {
                        c = d + 1
                    },
                ),
            ],
        );

        disjunction_branch_tester(
            parse_quote! {
                OR (
                    C = c*B + r*A,
                    D = c*B + r*A,
                )
            },
            vec![
                (vec![1], parse_quote! { C = c*B + r*A }),
                (vec![2], parse_quote! { D = c*B + r*A }),
            ],
        );

        disjunction_branch_tester(
            parse_quote! {
                AND (
                    C = c*B + r*A,
                    D = d*B + s*A,
                    OR (
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                d = 6,
                            )
                        ),
                        c = d + 1,
                    )
                )
            },
            vec![
                (
                    vec![],
                    parse_quote! {
                        AND (
                            C = c*B + r*A,
                            D = d*B + s*A,
                            OR (
                                AND (
                                    c = d,
                                    D = a*B + b*A,
                                    OR (
                                        d = 5,
                                        d = 6,
                                    )
                                ),
                                c = d + 1,
                            )
                        )
                    },
                ),
                (
                    vec![1],
                    parse_quote! {
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                d = 6,
                            )
                        )
                    },
                ),
                (
                    vec![1, 1],
                    parse_quote! {
                        d = 5
                    },
                ),
                (
                    vec![1, 2],
                    parse_quote! {
                        d = 6
                    },
                ),
                (
                    vec![2],
                    parse_quote! {
                        c = d + 1
                    },
                ),
            ],
        );

        disjunction_branch_tester(
            parse_quote! {
                AND (
                    C = c*B + r*A,
                    D = d*B + s*A,
                    AND (
                        c = d + 1,
                        AND (
                            s = r,
                            OR (
                                d = 1,
                                AND (
                                    d = 2,
                                    s = 1,
                                )
                            )
                        )
                    ),
                    OR (
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                d = 6,
                            )
                        ),
                        c = d + 1,
                    )
                )
            },
            vec![
                (
                    vec![],
                    parse_quote! {
                        AND (
                            C = c*B + r*A,
                            D = d*B + s*A,
                            AND (
                                c = d + 1,
                                AND (
                                    s = r,
                                    OR (
                                        d = 1,
                                        AND (
                                            d = 2,
                                            s = 1,
                                        )
                                    )
                                )
                            ),
                            OR (
                                AND (
                                    c = d,
                                    D = a*B + b*A,
                                    OR (
                                        d = 5,
                                        d = 6,
                                    )
                                ),
                                c = d + 1,
                            )
                        )
                    },
                ),
                (vec![1], parse_quote! { d = 1 }),
                (
                    vec![2],
                    parse_quote! {
                        AND (
                            d = 2,
                            s = 1,
                        )
                    },
                ),
                (
                    vec![3],
                    parse_quote! {
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                d = 6,
                            )
                        )
                    },
                ),
                (
                    vec![3, 1],
                    parse_quote! {
                        d = 5
                    },
                ),
                (
                    vec![3, 2],
                    parse_quote! {
                        d = 6
                    },
                ),
                (
                    vec![4],
                    parse_quote! {
                        c = d + 1
                    },
                ),
            ],
        );

        disjunction_branch_abort_tester(
            parse_quote! {
                AND (
                    C = c*B + r*A,
                    D = d*B + s*A,
                    OR (
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                true,
                                d = 6,
                            )
                        ),
                        c = d + 1,
                    )
                )
            },
            vec![
                (
                    vec![],
                    parse_quote! {
                        AND (
                            C = c*B + r*A,
                            D = d*B + s*A,
                            OR (
                                AND (
                                    c = d,
                                    D = a*B + b*A,
                                    OR (
                                        d = 5,
                                        true,
                                        d = 6,
                                    )
                                ),
                                c = d + 1,
                            )
                        )
                    },
                ),
                (
                    vec![1],
                    parse_quote! {
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                true,
                                d = 6,
                            )
                        )
                    },
                ),
                (
                    vec![1, 1],
                    parse_quote! {
                        d = 5
                    },
                ),
            ],
        );
    }

    fn disjunction_branch_leaf_tester(e: Expr, expected: Vec<(Vec<usize>, Vec<Expr>)>) {
        let mut output: Vec<(Vec<usize>, Vec<StatementTree>)> = Vec::new();
        let expected_st: Vec<(Vec<usize>, Vec<StatementTree>)> = expected
            .iter()
            .map(|(path, vex)| {
                (
                    path.clone(),
                    vex.iter()
                        .map(|ex| StatementTree::parse(ex).unwrap())
                        .collect(),
                )
            })
            .collect();
        let mut st = StatementTree::parse(&e).unwrap();
        st.for_each_disjunction_branch(&mut |db, path| {
            let mut dis_branch_output: Vec<StatementTree> = Vec::new();
            db.for_each_disjunction_branch_leaf(&mut |leaf| {
                dis_branch_output.push(leaf.clone());
                Ok(())
            })
            .unwrap();
            output.push((path.to_vec(), dis_branch_output));
            Ok(())
        })
        .unwrap();
        assert_eq!(output, expected_st);
    }

    fn disjunction_branch_leaf_abort_tester(e: Expr, expected: Vec<(Vec<usize>, Vec<Expr>)>) {
        let mut output: Vec<(Vec<usize>, Vec<StatementTree>)> = Vec::new();
        let expected_st: Vec<(Vec<usize>, Vec<StatementTree>)> = expected
            .iter()
            .map(|(path, vex)| {
                (
                    path.clone(),
                    vex.iter()
                        .map(|ex| StatementTree::parse(ex).unwrap())
                        .collect(),
                )
            })
            .collect();
        let mut st = StatementTree::parse(&e).unwrap();
        st.for_each_disjunction_branch(&mut |db, path| {
            let mut dis_branch_output: Vec<StatementTree> = Vec::new();
            db.for_each_disjunction_branch_leaf(&mut |leaf| {
                if leaf.is_leaf_true() {
                    return Err(syn::Error::new(proc_macro2::Span::call_site(), "true leaf"));
                }
                dis_branch_output.push(leaf.clone());
                Ok(())
            })?;
            output.push((path.to_vec(), dis_branch_output));
            Ok(())
        })
        .unwrap_err();
        assert_eq!(output, expected_st);
    }

    #[test]
    fn disjunction_branch_leaf_test() {
        disjunction_branch_leaf_tester(
            parse_quote! {
                C = c*B + r*A
            },
            vec![(vec![], vec![parse_quote! { C = c*B + r*A }])],
        );

        disjunction_branch_leaf_tester(
            parse_quote! {
               AND (
                   C = c*B + r*A,
                   D = d*B + s*A,
                   OR (
                       c = d,
                       c = d + 1,
                   )
               )
            },
            vec![
                (
                    vec![],
                    vec![
                        parse_quote! { C = c*B + r*A },
                        parse_quote! { D = d*B + s*A },
                    ],
                ),
                (vec![1], vec![parse_quote! { c = d }]),
                (vec![2], vec![parse_quote! { c = d + 1 }]),
            ],
        );

        disjunction_branch_leaf_tester(
            parse_quote! {
               AND (
                   C = c*B + r*A,
                   D = d*B + s*A,
                   OR (
                       c = d,
                       OR (
                           c = d + 1,
                           c = d + 2,
                        )
                   )
               )
            },
            vec![
                (
                    vec![],
                    vec![
                        parse_quote! { C = c*B + r*A },
                        parse_quote! { D = d*B + s*A },
                    ],
                ),
                (vec![1], vec![parse_quote! { c = d }]),
                (vec![2, 1], vec![parse_quote! { c = d + 1 }]),
                (vec![2, 2], vec![parse_quote! { c = d + 2 }]),
            ],
        );

        disjunction_branch_leaf_tester(
            parse_quote! {
                AND (
                    C = c*B + r*A,
                    D = d*B + s*A,
                    OR (
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                d = 6,
                            )
                        ),
                        c = d + 1,
                    )
                )
            },
            vec![
                (
                    vec![],
                    vec![
                        parse_quote! { C = c*B + r*A },
                        parse_quote! { D = d*B + s*A },
                    ],
                ),
                (
                    vec![1],
                    vec![
                        parse_quote! { c = d },
                        parse_quote! { D
                        = a*B + b*A },
                    ],
                ),
                (vec![1, 1], vec![parse_quote! { d = 5 }]),
                (vec![1, 2], vec![parse_quote! { d = 6 }]),
                (vec![2], vec![parse_quote! { c = d + 1 }]),
            ],
        );

        disjunction_branch_leaf_abort_tester(
            parse_quote! {
                AND (
                    C = c*B + r*A,
                    D = d*B + s*A,
                    OR (
                        AND (
                            c = d,
                            D = a*B + b*A,
                            OR (
                                d = 5,
                                true,
                                d = 6,
                            )
                        ),
                        c = d + 1,
                    )
                )
            },
            vec![
                (
                    vec![],
                    vec![
                        parse_quote! { C = c*B + r*A },
                        parse_quote! { D = d*B + s*A },
                    ],
                ),
                (
                    vec![1],
                    vec![
                        parse_quote! { c = d },
                        parse_quote! { D
                        = a*B + b*A },
                    ],
                ),
                (vec![1, 1], vec![parse_quote! { d = 5 }]),
            ],
        );
    }

    fn flatten_ands_tester(e: Expr, flattened_e: Expr) {
        let mut st = StatementTree::parse(&e).unwrap();
        st.flatten_ands();
        assert_eq!(st, StatementTree::parse(&flattened_e).unwrap());
    }

    #[test]
    // Test flatten_ands
    fn flatten_ands_test() {
        flatten_ands_tester(
            parse_quote! {
                C = x*B + r*A
            },
            parse_quote! {
                C = x*B + r*A
            },
        );

        flatten_ands_tester(
            parse_quote! {
                AND (
                    C = x*B + r*A,
                    AND (
                        D = x*B + s*A,
                        E = x*B + t*A,
                    ),
                )
            },
            parse_quote! {
                AND (
                    C = x*B + r*A,
                    D = x*B + s*A,
                    E = x*B + t*A,
                )
            },
        );

        flatten_ands_tester(
            parse_quote! {
                AND (
                    AND (
                        OR (
                            D = B + s*A,
                            D = s*A,
                        ),
                        D = x*B + t*A,
                    ),
                    C = x*B + r*A,
                )
            },
            parse_quote! {
                AND (
                    OR (
                        D = B + s*A,
                        D = s*A,
                    ),
                    D = x*B + t*A,
                    C = x*B + r*A,
                )
            },
        );

        flatten_ands_tester(
            parse_quote! {
                AND (
                    AND (
                        OR (
                            D = B + s*A,
                            AND (
                                D = s*A,
                                AND (
                                    E = s*B,
                                    F = s*C,
                                ),
                            ),
                        ),
                        D = x*B + t*A,
                    ),
                    C = x*B + r*A,
                )
            },
            parse_quote! {
                AND (
                    OR (
                        D = B + s*A,
                        AND (
                            D = s*A,
                            E = s*B,
                            F = s*C,
                        )
                    ),
                    D = x*B + t*A,
                    C = x*B + r*A,
                )
            },
        );
    }
}

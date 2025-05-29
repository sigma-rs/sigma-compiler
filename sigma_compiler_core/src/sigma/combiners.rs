use syn::parse::Result;
use syn::Expr;

/// The statements in the ZKP form a tree.  The leaves are basic
/// statements of various kinds; for example, equations or inequalities
/// about Scalars and Points.  The interior nodes are combiners: `And`,
/// `Or`, or `Thresh` (with a given constant threshold).  A leaf is true
/// if the basic statement it contains is true.  An `And` node is true
/// if all of its children are true.  An `Or` node is true if at least
/// one of its children is true.  A `Thresh` node (with threshold `k`) is
/// true if at least `k` of its children are true.

#[derive(Clone, Debug)]
pub enum StatementTree {
    Leaf(Expr),
    And(Vec<StatementTree>),
    Or(Vec<StatementTree>),
    Thresh(usize, Vec<StatementTree>),
}

impl StatementTree {
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

    pub fn parse_andlist(exprlist: &[Expr]) -> Result<Self> {
        let children: Result<Vec<StatementTree>> = exprlist.iter().map(Self::parse).collect();
        Ok(StatementTree::And(children?))
    }

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
}

#[cfg(test)]
mod test {
    use super::StatementTree::*;
    use super::*;
    use quote::quote;
    use syn::parse_quote;

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
}

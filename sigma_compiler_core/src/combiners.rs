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
        Ok(StatementTree::Leaf(expr.clone()))
    }

    pub fn parse_andlist(exprlist: &[Expr]) -> Result<Self> {
        let children: Result<Vec<StatementTree>> =
            exprlist.iter().map(|e| Self::parse(e)).collect();
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

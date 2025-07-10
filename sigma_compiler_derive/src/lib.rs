use proc_macro::TokenStream;
use sigma_compiler_core::{sigma_compiler_core, SigmaCompSpec};
use syn::parse_macro_input;

#[cfg(not(doctest))]
/// The main macro provided by this crate.
///
/// The general form of this macro is:
/// ```
/// sigma_compiler! { proto_name<Grp>,
///    (scalar_list),
///    (point_list),
///    statement_1,
///    statement_2,
///    ...
/// }
/// ```
///
/// The pieces are as follows:
///
///   - `proto_name`: The name of the protocol.  A Rust submodule will
///      be created with this name, containing all of the data
///      structures and code associated with this sigma protocol.
///   - `<Grp>`: an optional indication of the mathematical group to use
///     (a set of `Point`s and associated `Scalar`s) for this sigma
///     protocol.  The group must implement the
///     [`PrimeGroup`](https://docs.rs/group/0.13.0/group/prime/trait.PrimeGroup.html)
///     trait.  If `<Grp>` is omitted, it defaults to assuming there is
///     a group called `G` in the current scope.
///   - `scalar_list` is a list of variables representing `Scalar`s.
///     Each variable can be optionally tagged with one or more of the
///     tags `pub`, `rand`, or `vec`.  The tags `pub` and `rand` cannot
///     both be used on the same variable.
///     - `pub` means that the `Scalar` is public; this can be used for
///       public parameters to the protocol, such as the limits of
///       ranges, or other constants that can appear in the statements.
///       Any `Scalar` not marked as `pub` is assumed to be private to
///       the prover, and the verifier will learn nothing about that
///       value other than what is implied by the truth of the
///       statements.
///     - `rand` means that the `Scalar` is a uniform random `Scalar`.
///       A `rand` `Scalar` must be used only once in the statements.
///       These are typically used as randomizers in Pedersen
///       commitments.
///     - `vec` means that the variable represents a vector of
///       `Scalar`s, as opposed to a single `Scalar`.  The number of
///       entries in the vector can be set at runtime, when the sigma
///       protocol is executed.
///   - `point_list` is a list of variables representing `Point`s.  All
///     `Point` variables are considered public.  Each variable can be
///     optionally tagged with one or more of the tags `cind`, `const`,
///     or `vec`.  All combinations of tags are valid.
///     - All `Point`s tagged with `cind` are _computationally
///       independent_.  This means that the _prover_ does not know a
///       discrete logarithm relationship between any of them.
///       Formally, `P1`, `P2`, ..., `Pn` being computationally
///       independent `Point`s means that if the prover knows `Scalar`s
///       `s1`, `s2`, ..., `sn` such that `s1*P1 + s2*P2 + ... + sn*Pn =
///       0` (where `0` is the identity element of the group), then it
///       must be the case that each of `s1`, `s2`, ..., `sn` is the
///       zero `Scalar` (modulo the order of the group).  Typically,
///       these elements would be generators of the group, generated
///       with a hash-to-group function, as opposed to multiplying the
///       standard generator by a random number (at least not one known
///       by the prover).  `Point`s marked `cind` are typically the
///       bases used in Pedersen commitments.
///     - `const` means that the value of the `Point` will always be the
///       same for each invocation of the sigma protocol.  This is
///       typical for fixed generators, but possibly other `Point`s as
///       well.
///     - `vec` means that the variable represents a vector of `Point`s,
///       as opposed to a single `Point`.  The number of entries in the
///       vector can be set at runtime, when the sigma protocol is
///       executed.
///    - Each `statement` is a statement that the prover is proving the
///      truth of to the verifier.  Each statement can have one of the
///      following forms:
///      - `C = arith_expr`, where `C` is a variable representing a
///        `Point`, and `arith_expr` is an _arithmetic expression_
///        evaluating to a `Point`.  This is a _linear combination
///        statement_.  An arithmetic expression can consist of:
///        - `Scalar` or `Point` variables
///        - integer constants
///        - the operations `*`, `+`, `-` (binary or unary)
///        - the operation `<<` where both operands are expressions with no
///          variables
///        - parens
///
///        You cannot multiply together two private subexpressions, and
///        you cannot multiply together two subexpressions that both
///        evaluate to `Point`s.  You cannot add a `Point` to a
///        `Scalar`.  Integer constants are considered `Scalar`s, but
///        all arithmetic subexpressions involving only constants must
///        have values that fit in an [`i128`].
///
///        If any variable in `arith_expr` is marked `vec`, then this is
///        a vector expression, and `C` must also be marked `vec`.  The
///        statement is considered to hold in 'SIMD' style; that is, the
///        lengths of all of the vector variables involved in the
///        statement must be the same, and the statement is proven to
///        hold component-wise.  Any non-vector variable in the
///        statement is considered equivalent to a vector variable, all
///        of whose entries have the same value.
///      - `a = arith_expr`, where `a` is a variable representing a
///        private `Scalar`.  This is a _substitution statement_.  Its
///        meaning is to say that the private `Scalar` `a` has the value
///        given by the arithmetic expression, which must evaluate to a
///        `Scalar`.  The effect is to substitute `a` anywhere it
///        appears in the list of statements (including the right side
///        of other substitutions) with the given expression.  The
///        expression must not contain the variable `a` itself, either
///        directly, or after other substitutions.  For example, the
///        statement `a = a + b` is not allowed, nor is the combination
///        of substitutions `a = b + 1, b = c + 2, c = 2*a`.
///      - `(a..b).contains(x)`, where `a` and `b` are _public_
///        `Scalar`s (or arithmetic expressions evaluating to public
///        `Scalar`s), and `x` is a private `Scalar`, possibly
///        multiplied by a constant and adding or subtracting an
///        expression evaluating to a public `Scalar`.  For example,
///        `((a+2)..(3*b-7)).contains(2*x+2*c*c+12)` is allowed, if `a`,
///        `b`, and `c` are public `Scalar`s and `x` is a private
///        `Scalar`.  `(a..b).contains(x)` is a _range statement_, and
///        it means that `x` lies in the range `a..b`.  As usual in
///        Rust, the range `a..b` _includes_ `a`, but _excludes_ `b`.
///        If you want to include both endpoints, you can also use the
///        usual Rust notation `a..=b`.  The size of the range must fit
///        in an [`i128`].

#[proc_macro]
pub fn sigma_compiler(input: TokenStream) -> TokenStream {
    let mut spec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_core(&mut spec, true, true).into()
}

/// A version of the [`sigma_compiler!`] macro that only outputs the code
/// needed by the prover.
#[proc_macro]
pub fn sigma_compiler_prover(input: TokenStream) -> TokenStream {
    let mut spec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_core(&mut spec, true, false).into()
}

/// A version of the [`sigma_compiler!`] macro that only outputs the code
/// needed by the verifier.
#[proc_macro]
pub fn sigma_compiler_verifier(input: TokenStream) -> TokenStream {
    let mut spec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_core(&mut spec, false, true).into()
}

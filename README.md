`sigma-compiler` by Ian Goldberg, iang@uwaterloo.ca  
Version 0.1.0, 2025-10-10

This crate provides the `sigma_compiler!` macro as an easy interface
to the [`sigma-proofs`](https://crates.io/crates/sigma-proofs) API
for non-interactive zero-knowledge sigma protocols.

The general form of this macro is:
```
sigma_compiler! { proto_name<Grp>,
   (scalar_list),
   (point_list),
   statement_1,
   statement_2,
   ...
}
```

The pieces are as follows:

  - `proto_name`: The name of the protocol.  A Rust submodule will
     be created with this name, containing all of the data
     structures and code associated with this sigma protocol.
  - `<Grp>`: an optional indication of the mathematical group to use
    (a set of `Point`s and associated `Scalar`s) for this sigma
    protocol.  The group must implement the
    [`PrimeGroup`]
    trait.  If `<Grp>` is omitted, it defaults to assuming there is
    a group called `G` in the current scope.
  - `scalar_list` is a list of variables representing `Scalar`s.
    Each variable can be optionally tagged with one or more of the
    tags `pub`, `rand`, or `vec`.  The tags `pub` and `rand` cannot
    both be used on the same variable.
    - `pub` means that the `Scalar` is public; this can be used for
      public parameters to the protocol, such as the limits of
      ranges, or other constants that can appear in the statements.
      Any `Scalar` not marked as `pub` is assumed to be private to
      the prover, and the verifier will learn nothing about that
      value other than what is implied by the truth of the
      statements.
    - `rand` means that the `Scalar` is a uniform random `Scalar`.
      A `rand` `Scalar` may not be used in range or not-equals
      statements.  These are typically used as randomizers in Pedersen
      commitments.
    - `vec` means that the variable represents a vector of
      `Scalar`s, as opposed to a single `Scalar`.  The number of
      entries in the vector can be set at runtime, when the sigma
      protocol is executed.
  - `point_list` is a list of variables representing `Point`s.  All
    `Point` variables are considered public.  Each variable can be
    optionally tagged with one or more of the tags `cind`, `const`,
    or `vec`.  All combinations of tags are valid.
    - All `Point`s tagged with `cind` are _computationally
      independent_.  This means that the _prover_ does not know a
      discrete logarithm relationship between any of them.
      Formally, `P1`, `P2`, ..., `Pn` being computationally
      independent `Point`s means that if the prover knows `Scalar`s
      `s1`, `s2`, ..., `sn` such that `s1*P1 + s2*P2 + ... + sn*Pn =
      0` (where `0` is the identity element of the group), then it
      must be the case that each of `s1`, `s2`, ..., `sn` is the
      zero `Scalar` (modulo the order of the group).  Typically,
      these elements would be generators of the group, generated
      with a hash-to-group function, as opposed to multiplying the
      standard generator by a random number (at least not one known
      by the prover).  `Point`s marked `cind` are typically the
      bases used in Pedersen commitments.
    - `const` means that the value of the `Point` will always be the
      same for each invocation of the sigma protocol.  This is
      typical for fixed generators, but possibly other `Point`s as
      well.
    - `vec` means that the variable represents a vector of `Point`s,
      as opposed to a single `Point`.  The number of entries in the
      vector can be set at runtime, when the sigma protocol is
      executed.
   - Each `statement` is a statement that the prover is proving the
     truth of to the verifier.  Each statement can have one of the
     following forms:
     - `C = arith_expr`, where `C` is a variable representing a
       `Point`, and `arith_expr` is an _arithmetic expression_
       evaluating to a `Point`.  This is a _linear combination
       statement_.  An arithmetic expression can consist of:
       - `Scalar` or `Point` variables
       - integer constants
       - the operations `*`, `+`, `-` (binary or unary)
       - the operation `<<` where both operands are expressions with no
         variables
       - the function `sum` that takes a single vector argument and
         returns the sum of its elements
       - parens

       You cannot multiply together two private subexpressions, and
       you cannot multiply together two subexpressions that both
       evaluate to `Point`s.  You cannot add a `Point` to a
       `Scalar`.  Integer constants are considered `Scalar`s, but
       all arithmetic subexpressions involving only constants must
       have values that fit in an [`i128`].

       If any variable in `arith_expr` is marked `vec`, then this is
       a vector expression, and `C` must also be marked `vec`.  The
       statement is considered to hold in 'SIMD' style; that is, the
       lengths of all of the vector variables involved in the
       statement must be the same, and the statement is proven to
       hold component-wise.  Any non-vector variable in the
       statement is considered equivalent to a vector variable, all
       of whose entries have the same value.  Note that you can do a
       dot product between two vectors `x` and `A` with `sum(x*A)`.

       As an extension, you can also use an arithmetic expression
       evaluating to a _public_ `Point` in place of `C` on the left
       side of the `=`.  For example, if `a` is a `Scalar` tagged
       `pub`, and `C` is a `Point`, then the expression `(2*a+1)*C =
       arith_expr` is a valid linear combination statement.
     - `a = arith_expr`, where `a` is a variable representing a
       private `Scalar`.  This is a _substitution statement_.  Its
       meaning is to say that the private `Scalar` `a` has the value
       given by the arithmetic expression, which must evaluate to a
       `Scalar`.  The effect is to substitute `a` anywhere it
       appears in the list of statements (including the right side
       of other substitutions) with the given expression.  The
       expression must not contain the variable `a` itself, either
       directly, or after other substitutions.  For example, the
       statement `a = a + b` is not allowed, nor is the combination
       of substitutions `a = b + 1, b = c + 2, c = 2*a`.
     - `a = arith_expr`, where `a` is a variable representing a
       public `Scalar`.  This is a _public Scalar equality
       statement_.  Its meaning is to say that the public `Scalar`
       `a` has the value given by the arithmetic expression, which
       must evaluate to a public `Scalar`.  The statement is simply
       removed from the list of statements to be proven in the
       zero-knowledge sigma protocol, and code is emitted for the
       prover and verifier to each just check that the statement is
       satisfied.  Currently, there can be no vector variables in
       this kind of statement.
     - `(a..b).contains(x)`, where `a` and `b` are constants or
       _public_ `Scalar`s (or arithmetic expressions evaluating to
       public `Scalar`s), and `x` is a private `Scalar`, possibly
       multiplied by a constant and adding or subtracting an
       expression evaluating to a public `Scalar`.  For example,
       `((a+2)..(3*b-7)).contains(2*x+2*c*c+12)` is allowed, if `a`,
       `b`, and `c` are public `Scalar`s and `x` is a private
       `Scalar`.  `(a..b).contains(x)` is a _range statement_, and
       it means that `x` lies in the range `a..b`.  As usual in
       Rust, the range `a..b` _includes_ `a`, but _excludes_ `b`.
       If you want to include both endpoints, you can also use the
       usual Rust notation `a..=b`.  The size of the range must fit
       in an [`i128`].
     - `x != a`, where `x` is a private `Scalar`, possibly
       multiplied by a constant and adding or subtracting an
       expression evaluating to a public `Scalar`, and `a` is a
       constant or _public_ `Scalar` (or an arithmetic expression
       evaluating to a public `Scalar`).  For example, `2*x+2*c*c+12
       != a*b+17` is allowed, if `a`, `b`, and `c` are public
       `Scalar`s and `x` is a private `Scalar`.  `x != 0` is a more
       typical example.  This is a _not-equals statement_, and it
       means that the value of the expression on the left is not
       equal to the value of the expression on the right.
   - Statements can also be combined with `AND(st1,st2,...,stn)`,
     `OR(st1,st2,...,stn)`, or `THRESH(t,st1,st2,...,stn)`.  The list of
     statements in the macro invocation are implicitly put into a
     top-level `AND`.  `AND`s, `OR`s, and `THRESH`s can be arbitrarily
     nested.  As usual, an `AND` statement is true when all of its
     component statements are true; an `OR` statement is true when at
     least one of its component statements is true; a `THRESH` statement
     is true when at least `t` of its component statements are true.
   

The macro creates a submodule with the name specified by
`proto_name`.  This module contains:
  - A struct `Instance` containing all the `Point`s and _public_
    `Scalar`s specified in the macro invocation.  Any public vector
    `Scalar` or `Point` variable will be represented as a
    `Vec<Scalar>` or `Vec<Point>` respectively.
  - A struct `Witness` containing all the _private_ `Scalar`s
    specified in the macro invocation. Any private vector variable
    will be represented as a `Vec<Scalar>`.
  - A function `prove` with the signature
    ```
    pub fn prove(
        instance: &Instance,
        witness: &Witness,
        session_id: &[u8],
        rng: &mut (impl CryptoRng + RngCore),
    ) -> sigma_proofs::errors::Result<Vec<u8>>
    ```
    The parameter `instance` contains the public variables (also
    known to the verifier).  The parameter `witness` contains the
    private variables known only to the prover.  The parameter
    `session_id` can be any byte slice; the proof is bound to this
    byte slice, and the verifier must use the same byte slice in
    order to verify the proof.  The parameter `rng` is a random
    number generator that implements the [`CryptoRng`] and
    [`RngCore`] traits.  The output, if successful, is the proof as
    a byte vector.
  - A function `verify` with the signature
    ```
    pub fn verify(
        instance: &Instance,
        proof: &[u8],
        session_id: &[u8],
    ) -> sigma_proofs::errors::Result<()>
    ```
    The parameter `instance` contains the public variables, and must
    be the same as passed to the `prove` function.  The parameter
    `proof` contains the output of the `prove` function.  The
    parameter `session_id` must be the same as passed to the `prove`
    function.  If `verify` returns `Ok(())`, then the verifier can
    assume that the prover did know a `Witness` struct for which the
    statements (with public values specified by the given
    `Instance` struct) are all true, but the verifier does not learn
    any other information about that `Witness` struct.

[`PrimeGroup`]: https://docs.rs/group/0.13.0/group/prime/trait.PrimeGroup.html
[`CryptoRng`]: https://docs.rs/rand/0.8.5/rand/trait.CryptoRng.html
[`RngCore`]: https://docs.rs/rand/0.8.5/rand/trait.RngCore.html
[`i128`]: https://doc.rust-lang.org/1.78.0/std/primitive.i128.html

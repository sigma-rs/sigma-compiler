use proc_macro::TokenStream;
use sigma_compiler_core::{sigma_compiler_core, SigmaCompSpec};
use syn::parse_macro_input;

#[cfg(not(doctest))]
/// The main macro provided by the `sigma_compiler` crate.
///
#[doc = include_str!("../macro-doc.md")]
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

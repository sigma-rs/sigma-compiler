use proc_macro::TokenStream;
use sigma_compiler_core::{sigma_compiler_core, SigmaCompSpec};
use syn::parse_macro_input;

#[proc_macro]
pub fn sigma_compiler(input: TokenStream) -> TokenStream {
    let mut spec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_core(&mut spec, true, true).into()
}

#[proc_macro]
pub fn sigma_compiler_prover(input: TokenStream) -> TokenStream {
    let mut spec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_core(&mut spec, true, false).into()
}

#[proc_macro]
pub fn sigma_compiler_verifier(input: TokenStream) -> TokenStream {
    let mut spec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_core(&mut spec, false, true).into()
}

use proc_macro::TokenStream;
use sigma_compiler_core::{SigmaCompSpec, sigma_compiler_core};
use syn::parse_macro_input;

#[proc_macro]
pub fn sigma_compiler(input: TokenStream) -> TokenStream {
    let spec = parse_macro_input!(input as SigmaCompSpec);
    sigma_compiler_core(&spec, true, true).into()
}

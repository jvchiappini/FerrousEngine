use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Derive implementation of [`ferrous_ecs::component::Component`].
///
/// This macro expands to a trivial impl block.  We provide it as a convenience
/// so users can write `#[derive(Component)]` instead of typing the empty
/// impl themselves.  The derive is enabled by default via the
/// `ferrous_ecs` crate's `derive` feature.
#[proc_macro_derive(Component)]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let expanded = quote! {
        impl ferrous_ecs::component::Component for #name {}
    };
    TokenStream::from(expanded)
}

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Meta, NestedMeta, Lit};

pub fn derive_ferrous_controller(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let mut inject_arms = Vec::new();
    

    // Look for path in #[fui_view(path = "...")]
    let mut fui_path = None;
    for attr in &input.attrs {
        if attr.path.is_ident("fui_view") {
            if let Ok(Meta::List(meta)) = attr.parse_meta() {
                for nested in meta.nested {
                    if let NestedMeta::Meta(Meta::NameValue(nv)) = nested {
                        if nv.path.is_ident("path") {
                            if let Lit::Str(lit_str) = nv.lit {
                                fui_path = Some(lit_str.value());
                            }
                        }
                    }
                }
            }
        }
    }

    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields) = &data_struct.fields {
            for field in &fields.named {
                let field_name = field.ident.as_ref().unwrap();
                for attr in &field.attrs {
                    if attr.path.is_ident("fui_id") {
                        let id_str = field_name.to_string();
                        // Support Option<NodeId> or direct NodeId
                        inject_arms.push(quote! {
                            #id_str => {
                                // Try assigning directly if Option, else direct assignment
                                // Since we don't know the exact type, we assume Option<NodeId> for simplicity in macro demo
                                // OR we can just expect it to be `NodeId` or `Option<NodeId>`
                                // We will use a trait or direct assignment if it matches.
                                self.#field_name = Some(node.clone());
                                true
                            }
                        });
                    }
                }
            }
        }
    }

    // In a real scenario we'd parse the `impl AppController` and look at #[fui_action].
    // Since we are limited in derive macros (they only see the struct, not the impl), 
    // we use a workaround where methods named `on_*_click` are registered, or we require an attribute macro on the impl.
    // For now, let's keep the action_arms open for a standard naming convention like `on_action_name`.
    // We can rely on a hack or require the user to implement `dispatch_fui_action` manually if we don't use an attribute macro.
    // Wait, let's just make the macro search for standard naming conventions. It won't know the methods.
    
    let _path_literal = match fui_path {
        Some(path) => quote! { Some(include_str!(#path)) }, // Assuming the path is relative to Cargo.toml
        None => quote! { None },
    };

    let expanded = quote! {
        impl ferrous_ui_core::FerrousController for #name {
            fn inject_fui_id(&mut self, id: &str, node: ferrous_ui_core::NodeId) -> bool {
                match id {
                    #(#inject_arms,)*
                    _ => false,
                }
            }
            
            fn dispatch_fui_action(
                &mut self, 
                action: &str, 
                ctx: &mut ferrous_ui_core::EventContext<Self>, 
                event: &ferrous_ui_core::UiEvent
            ) -> ferrous_ui_core::EventResponse {
                // Here we would match action to self.method()
                // For this MVP, we ignore this and return Ignored, user can override or we can implement an attribute macro `#[fui_controller]` later
                ferrous_ui_core::EventResponse::Ignored
            }

            fn static_fui_view(&self) -> Option<&'static str> {
                // We'd compile the FUI string if provided
                None // #path_literal
            }
        }
    };

    TokenStream::from(expanded)
}

// Dummy attribute macro for #[fui_action]
pub fn fui_action_attr(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

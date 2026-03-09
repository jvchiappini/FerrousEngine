extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::{Parse, ParseStream}, parse_macro_input, Token, Ident, LitStr, braced, parenthesized, DeriveInput, Data, Fields, Meta, NestedMeta, Lit};

struct UiElement {
    name: Ident,
    args: Vec<syn::Expr>,
    children: Vec<UiElement>,
}

impl Parse for UiElement {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        
        let mut args = Vec::new();
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            while !content.is_empty() {
                args.push(content.parse()?);
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                }
            }
        }

        let mut children = Vec::new();
        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);
            while !content.is_empty() {
                children.push(content.parse()?);
            }
        }

        Ok(UiElement { name, args, children })
    }
}

#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    let root = parse_macro_input!(input as UiElement);
    
    let expanded = expand_element(&root);
    
    TokenStream::from(expanded)
}

fn expand_element(el: &UiElement) -> proc_macro2::TokenStream {
    let name = &el.name;
    let args = &el.args;
    let children: Vec<_> = el.children.iter().map(expand_element).collect();

    quote! {
        {
            let __id = ctx.add_child(Box::new(#name::new(#(#args),*)));
            {
                let mut ctx = BuildContext { 
                    tree: ctx.tree, 
                    node_id: __id, 
                    theme: ctx.theme 
                };
                #(#children)*
            }
            __id
        }
    }
}

// ─── Derive FerrousWidget ───────────────────────────────────────────────────

#[proc_macro_derive(FerrousWidget, attributes(prop))]
pub fn derive_ferrous_widget(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let name_str = name.to_string();

    let fields = match &input.data {
        Data::Struct(s) => &s.fields,
        _ => return TokenStream::from(quote! { compile_error!("FerrousWidget solo puede derivarse en structs"); }),
    };

    let mut inspect_chunks = Vec::new();
    let mut apply_chunks = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().expect("Campos deben tener nombre");
        let field_key = field_name.to_string();
        
        let mut prop_data = None;
        for attr in &field.attrs {
            if attr.path.is_ident("prop") {
                let mut label = field_key.clone();
                let mut category = "General".to_string();
                let mut min = None;
                let mut max = None;

                if let Ok(Meta::List(list)) = attr.parse_meta() {
                    for nested in list.nested {
                        if let NestedMeta::Meta(Meta::NameValue(nv)) = nested {
                            if nv.path.is_ident("label") {
                                if let Lit::Str(s) = nv.lit { label = s.value(); }
                            } else if nv.path.is_ident("category") {
                                if let Lit::Str(s) = nv.lit { category = s.value(); }
                            } else if nv.path.is_ident("min") {
                                if let Lit::Float(f) = nv.lit { min = Some(f.base10_parse::<f32>().unwrap()); }
                            } else if nv.path.is_ident("max") {
                                if let Lit::Float(f) = nv.lit { max = Some(f.base10_parse::<f32>().unwrap()); }
                            }
                        }
                    }
                }
                prop_data = Some((label, category, min, max));
                break;
            }
        }

        if let Some((label, category, min, max)) = prop_data {
            let range_tokens = match (min, max) {
                (Some(mn), Some(mx)) => quote! { Some((#mn, #mx)) },
                _ => quote! { None },
            };

            let field_type = &field.ty;
            let type_str = quote!(#field_type).to_string();
            
            let prop_val_expr = if type_str.contains("f32") {
                quote! { crate::PropValue::Float(self.#field_name) }
            } else if type_str.contains("bool") {
                quote! { crate::PropValue::Bool(self.#field_name) }
            } else if type_str.contains("String") {
                quote! { crate::PropValue::String(self.#field_name.clone()) }
            } else if type_str.contains("Color") {
                quote! { crate::PropValue::Color(self.#field_name.to_array()) }
            } else if type_str.contains("Rect") {
                quote! { crate::PropValue::Rect(self.#field_name.to_array()) }
            } else {
                quote! { crate::PropValue::Bool(false) }
            };

            inspect_chunks.push(quote! {
                crate::InspectorProp {
                    key: #field_key.to_string(),
                    label: #label.to_string(),
                    category: #category.to_string(),
                    value: #prop_val_expr,
                    range: #range_tokens,
                    tooltip: None,
                }
            });

            let apply_expr = if type_str.contains("f32") {
                quote! { if let crate::PropValue::Float(v) = value { self.#field_name = v; true } else { false } }
            } else if type_str.contains("bool") {
                quote! { if let crate::PropValue::Bool(v) = value { self.#field_name = v; true } else { false } }
            } else if type_str.contains("String") {
                quote! { if let crate::PropValue::String(v) = value { self.#field_name = v; true } else { false } }
            } else if type_str.contains("Color") {
                quote! { if let crate::PropValue::Color(v) = value { self.#field_name = crate::Color::from_array(v); true } else { false } }
            } else {
                quote! { false }
            };

            apply_chunks.push(quote! {
                #field_key => { #apply_expr }
            });
        }
    }

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics crate::FerrousWidgetReflect for #name #ty_generics #where_clause {
            fn widget_type_name(&self) -> &'static str {
                #name_str
            }

            fn inspect_props(&self) -> Vec<crate::InspectorProp> {
                vec![
                    #(#inspect_chunks),*
                ]
            }

            fn apply_prop(&mut self, key: &str, value: crate::PropValue) -> bool {
                match key {
                    #(#apply_chunks,)*
                    _ => false,
                }
            }
        }
    };

    TokenStream::from(expanded)
}


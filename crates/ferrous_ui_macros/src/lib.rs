extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::{Parse, ParseStream}, parse_macro_input, Token, Ident, LitStr, braced, parenthesized};

struct UiElement {
    name: Ident,
    args: Vec<syn::Expr>,
    children: Vec<UiElement>,
}

impl Parse for UiElement {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        
        let mut args = Vec::new();
        if input.peek(parenthesized) {
            let content;
            parenthesized!(content in input);
            while !content.is_empty() {
                args.push(content.parse()?);
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                }
            }
        }

        let mut children = Vec::new();
        if input.peek(braced) {
            let content;
            braced!(content in input);
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

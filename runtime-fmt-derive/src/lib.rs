//! A custom-derive implementation for the `FormatArgs` trait.
#![recursion_limit="128"]

extern crate proc_macro;
extern crate syn;
#[macro_use] extern crate quote;

use proc_macro::TokenStream;

/// Derive a `FormatArgs` implementation for the provided input struct.
#[proc_macro_derive(FormatArgs)]
pub fn derive_format_args(input: TokenStream) -> TokenStream {
    let string = input.to_string();
    let ast = syn::parse_derive_input(&string).unwrap();
    implement(&ast).parse().unwrap()
}

fn implement(ast: &syn::DeriveInput) -> quote::Tokens {
    // The rough structure of this (dummy_ident, extern crate/use) is based on
    // how serde_derive does it.

    let ident = &ast.ident;
    let variant = match ast.body {
        syn::Body::Struct(ref variant) => variant,
        _ => panic!("#[derive(FormatArgs)] is not implemented for enums")
    };

    let dummy_ident = syn::Ident::new(format!("_IMPL_FORMAT_ARGS_FOR_{}", ident));

    let (validate_name, validate_index, get_child, as_usize);
    match *variant {
        syn::VariantData::Struct(ref fields) => {
            get_child = build_fields(fields);
            as_usize = build_usize(ast, fields);
            validate_index = quote! { false };

            let index = 0..fields.len();
            let ident: Vec<_> = fields.iter()
                .map(|field| field.ident.as_ref().unwrap())
                .map(ToString::to_string)
                .collect();
            validate_name = quote! {
                match name {
                    #(#ident => _Option::Some(#index),)*
                    _ => _Option::None,
                }
            };
        }
        syn::VariantData::Tuple(ref fields) => {
            get_child = build_fields(fields);
            as_usize = build_usize(ast, fields);
            validate_name = quote! { _Option::None };

            let len = fields.len();
            validate_index = quote! { index < #len };
        }
        syn::VariantData::Unit => {
            validate_name = quote! { _Option::None };
            validate_index = quote! { false };
            get_child = quote! { panic!("bad index {}", index) };
            as_usize = get_child.clone();
        }
    };

    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    quote! {
        #[allow(non_upper_case_globals, unused_attributes)]
        #[allow(unused_variables, unused_qualifications)]
        const #dummy_ident: () = {
            extern crate runtime_fmt as _runtime_fmt;
            use std::fmt::{Formatter as _Formatter, Result as _Result};
            use std::option::Option as _Option;
            #[automatically_derived]
            impl #impl_generics _runtime_fmt::FormatArgs for #ident #ty_generics #where_clause {
                fn validate_name(name: &str) -> _Option<usize> {
                    #validate_name
                }
                fn validate_index(index: usize) -> bool {
                    #validate_index
                }
                fn get_child<__F>(index: usize) -> _Option<fn(&Self, &mut _Formatter) -> _Result>
                    where __F: _runtime_fmt::codegen::FormatTrait + ?Sized
                {
                    #get_child
                }
                fn as_usize(index: usize) -> Option<fn(&Self) -> &usize> {
                    #as_usize
                }
            }
        };
    }
}

fn build_fields(fields: &[syn::Field]) -> quote::Tokens {
    let index = 0..fields.len();
    let ty: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let ident: Vec<_> = fields.iter().enumerate().map(|(idx, field)| match field.ident {
        Some(ref ident) => ident.clone(),
        None => syn::Ident::from(idx),
    }).collect();
    quote! {
        match index {
            #(
                #index => _runtime_fmt::codegen::combine::<__F, Self, #ty, _>(
                    |this| &this.#ident
                ),
            )*
            _ => panic!("bad index {}", index)
        }
    }
}

fn build_usize(ast: &syn::DeriveInput, fields: &[syn::Field]) -> quote::Tokens {
    let self_ = &ast.ident;
    let (_, ty_generics, where_clause) = ast.generics.split_for_impl();

    // To avoid causing trouble with lifetime elision rules, an explicit
    // lifetime for the input and output is used.
    let lifetime = syn::Ident::new("'__as_usize_inner");
    let mut generics2 = ast.generics.clone();
    generics2.lifetimes.insert(0, syn::LifetimeDef {
        attrs: vec![],
        lifetime: syn::Lifetime { ident: lifetime.clone() },
        bounds: vec![],
    });
    let (impl_generics, _, _) = generics2.split_for_impl();

    let mut result = quote::Tokens::new();
    for (idx, field) in fields.iter().enumerate() {
        let ident = match field.ident {
            Some(ref ident) => ident.clone(),
            None => syn::Ident::from(idx),
        };
        let ty = &field.ty;
        result.append(quote! {
            #idx => {
                fn inner #impl_generics (this: &#lifetime #self_ #ty_generics)
                    -> &#lifetime #ty
                    #where_clause { &this.#ident }
                _runtime_fmt::codegen::as_usize(inner)
            },
        });
    }

    quote! {
        match index {
            #result
            _ => panic!("bad index {}", index)
        }
    }
}

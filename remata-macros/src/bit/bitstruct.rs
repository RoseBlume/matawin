use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};

use super::shared::{BitTracker, get_bit};

pub fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    // backing type
    let mut backing = quote! { u8 };

    for attr in &input.attrs {
        if attr.path().is_ident("bitstruct") {
            let ty = attr.parse_args::<syn::Type>().unwrap();
            backing = quote! { #ty };
        }
    }

    let Data::Struct(data_struct) = input.data else {
        panic!("BitStruct only supports structs");
    };

    let Fields::Named(fields) = data_struct.fields else {
        panic!("BitStruct requires named fields");
    };

    let mut tracker = BitTracker::new();

    let mut from_fields = Vec::new();
    let mut into_exprs = Vec::new();

    for field in fields.named {
        let ident = field.ident.unwrap();

        let explicit = get_bit(&field.attrs);
        let bit = tracker.assign(explicit);

        from_fields.push(quote! {
            #ident: (value & (1 << #bit)) != 0
        });

        into_exprs.push(quote! {
            ((self.#ident as #backing) << #bit)
        });
    }

    quote! {
        impl From<#backing> for #name {
            fn from(value: #backing) -> Self {
                Self {
                    #(#from_fields,)*
                }
            }
        }

        impl From<#name> for #backing {
            fn from(self: #name) -> #backing {
                0 #( | #into_exprs )*
            }
        }

        impl #name {
            pub fn from_bits(value: #backing) -> Self {
                value.into()
            }

            pub fn bits(self) -> #backing {
                self.into()
            }
        }
    }
    .into()
}
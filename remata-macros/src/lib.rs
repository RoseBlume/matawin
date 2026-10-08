use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, LitInt};

#[proc_macro_derive(FromPrimitive, attributes(value))]
pub fn derive_from_primitive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let Data::Enum(data) = input.data else {
        return syn::Error::new_spanned(name, "FromPrimitive only supports enums")
            .to_compile_error()
            .into();
    };

    let mut variants = Vec::new();
    for variant in data.variants {
        let ident = variant.ident;
        let mut value = None::<LitInt>;
        for attr in variant.attrs {
            if attr.path().is_ident("value") {
                match attr.parse_args::<LitInt>() {
                    Ok(v) => value = Some(v),
                    Err(e) => return e.to_compile_error().into(),
                }
            }
        }
        let Some(value) = value else {
            return syn::Error::new_spanned(
                ident,
                "FromPrimitive requires #[value(...)] on every enum variant",
            )
            .to_compile_error()
            .into();
        };
        variants.push((ident, value));
    }

    let impl_for = |ty: proc_macro2::TokenStream| {
        let arms = variants.iter().map(|(ident, value)| {
            quote! { x if x == (#value as #ty) => ::core::result::Result::Ok(Self::#ident), }
        });
        quote! {
            impl ::core::convert::TryFrom<#ty> for #name {
                type Error = ();
                fn try_from(value: #ty) -> ::core::result::Result<Self, Self::Error> {
                    match value {
                        #(#arms)*
                        _ => ::core::result::Result::Err(()),
                    }
                }
            }
        }
    };

    let generated = [
        impl_for(quote! { u8 }),
        impl_for(quote! { u16 }),
        impl_for(quote! { u32 }),
        impl_for(quote! { u64 }),
        impl_for(quote! { usize }),
        impl_for(quote! { i8 }),
        impl_for(quote! { i16 }),
        impl_for(quote! { i32 }),
        impl_for(quote! { i64 }),
        impl_for(quote! { isize }),
    ];

    quote! { #(#generated)* }.into()
}

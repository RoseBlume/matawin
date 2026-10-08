use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, Data, Fields};

use super::shared::{BitTracker, get_discriminant};

pub fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let set_name = format_ident!("{}Set", name);

    let Data::Enum(data_enum) = input.data else {
        panic!("BitEnum only supports enums");
    };

    let mut tracker = BitTracker::new();
    let mut variants = Vec::new();

    for variant in data_enum.variants {
        if !matches!(variant.fields, Fields::Unit) {
            panic!("BitEnum only supports unit variants");
        }

        let ident = variant.ident;

        let explicit = variant
            .discriminant
            .as_ref()
            .and_then(|(_, expr)| get_discriminant(expr));

        let bit = tracker.assign(explicit);

        variants.push((ident, bit));
    }

    // -----------------------------
    // CORE ARM GENERATION
    // -----------------------------

    let bit_match_arms = variants.iter().map(|(ident, bit)| {
        quote! { Self::#ident => 1 << #bit }
    });

    let all_bits = variants.iter().map(|(_, bit)| {
        quote! { (1 << #bit) }
    });

    let contains_arms = variants.iter().map(|(ident, bit)| {
        quote! {
            #name::#ident => (self.0 & (1 << #bit)) != 0
        }
    });

    let display_arms = variants.iter().map(|(ident, bit)| {
        quote! {
            if (self.0 & (1 << #bit)) != 0 {
                if !first {
                    write!(f, " | ")?;
                }
                write!(f, "{}", stringify!(#ident))?;
                first = false;
            }
        }
    });

    // -----------------------------
    // GENERATED CODE
    // -----------------------------

    quote! {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub struct #set_name(pub u32);

        impl #name {
            pub const fn bit(self) -> u32 {
                match self {
                    #(#bit_match_arms,)*
                }
            }
        }

        impl #set_name {
            pub const fn empty() -> Self { Self(0) }

            pub const fn all() -> Self {
                Self(0 #( | #all_bits )*)
            }

            pub const fn from_bits(bits: u32) -> Self {
                Self(bits)
            }

            pub const fn bits(self) -> u32 {
                self.0
            }

            pub const fn contains(self, flag: #name) -> bool {
                match flag {
                    #(#contains_arms,)*
                }
            }

            pub fn insert(&mut self, flag: #name) {
                self.0 |= flag.bit();
            }
        }

        // -----------------------------
        // Bitwise ops
        // -----------------------------

        impl std::ops::BitOr for #name {
            type Output = #set_name;

            fn bitor(self, rhs: Self) -> Self::Output {
                #set_name(self.bit() | rhs.bit())
            }
        }

        impl std::ops::BitOr<#name> for #set_name {
            type Output = Self;

            fn bitor(self, rhs: #name) -> Self {
                Self(self.0 | rhs.bit())
            }
        }

        impl std::ops::BitOr for #set_name {
            type Output = Self;

            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }

        impl std::ops::BitAnd for #set_name {
            type Output = Self;

            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        // -----------------------------
        // Display + Debug for SET
        // -----------------------------

        impl std::fmt::Display for #set_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut first = true;

                #(#display_arms)*

                if first {
                    write!(f, "(empty)")?;
                }

                Ok(())
            }
        }

        impl std::fmt::Debug for #set_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self)
            }
        }
    }
    .into()
}
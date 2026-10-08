use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, LitStr,
};

pub fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let Data::Enum(data_enum) = input.data else {
        panic!("FromGuid only supports enums");
    };

    let mut arms_str = Vec::new();
    let mut arms_bytes = Vec::new();

    for variant in data_enum.variants {
        if !matches!(variant.fields, Fields::Unit) {
            panic!("FromGuid only supports unit variants");
        }

        let ident = variant.ident;

        let mut guid = None;

        for attr in variant.attrs {
            if attr.path().is_ident("guid") {
                let lit: LitStr = attr.parse_args().unwrap();
                guid = Some(lit.value());
            }
        }

        let Some(guid) = guid else {
            continue;
        };

        let parsed_bytes = parse_guid_to_bytes(&guid);

        arms_str.push(quote! {
            #guid => Some(Self::#ident),
        });

        arms_bytes.push(quote! {
            #parsed_bytes => Some(Self::#ident),
        });
    }

    let expanded = quote! {
        impl #name {
            pub fn from_guid_str(s: &str) -> Option<Self> {
                match s.to_ascii_lowercase().as_str() {
                    #( #arms_str )*
                    _ => None,
                }
            }

            pub fn from_guid_bytes(b: [u8; 16]) -> Option<Self> {
                match b {
                    #( #arms_bytes )*
                    _ => None,
                }
            }
        }

        impl std::str::FromStr for #name {
            type Err = ();

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::from_guid_str(s).ok_or(())
            }
        }
    };

    TokenStream::from(expanded)
}

fn parse_guid_to_bytes(g: &str) -> proc_macro2::TokenStream {
    let g = g.trim_matches('\'').trim_matches('"');

    let parts: Vec<&str> = g.split('-').collect();
    assert_eq!(parts.len(), 5);

    let p0 = hex16(parts[0]);
    let p1 = hex16(parts[1]);
    let p2 = hex16(parts[2]);
    let p3 = hex8(parts[3]);
    let p4 = hex12(parts[4]);

    quote! {
        [
            #(#p0),*,
            #(#p1),*,
            #(#p2),*,
            #(#p3),*,
            #(#p4),*
        ]
    }
}



fn hex16(s: &str) -> Vec<proc_macro2::TokenStream> {
    (0..s.len())
        .step_by(2)
        .map(|i| {
            let byte = u8::from_str_radix(&s[i..i + 2], 16).unwrap();
            quote! { #byte }
        })
        .collect()
}

fn hex8(s: &str) -> Vec<proc_macro2::TokenStream> {
    hex16(s)
}

fn hex12(s: &str) -> Vec<proc_macro2::TokenStream> {
    hex16(s)
}
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, DeriveInput, Data, Fields, Expr, ExprRange,
};

pub fn expand(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = input.ident;

    let data_enum = match input.data {
        Data::Enum(e) => e,
        _ => panic!("FromPrimitive can only be derived for enums"),
    };

    struct VariantInfo {
        ident: syn::Ident,
        kind: ValueKind,
        is_tuple: bool,
    }

    enum ValueKind {
        Single(Expr),
        Range(ExprRange),
    }

    let mut variants = Vec::new();
    let mut fallback_variant = None;

    // --- Parse variants
    for variant in data_enum.variants {
        let ident = variant.ident;
        let is_tuple = matches!(variant.fields, Fields::Unnamed(_));

        let mut value_kind = None;

        for attr in variant.attrs {
            if attr.path().is_ident("value") {
                // ✅ FULL expression parsing: #[value(...)]
                let expr = attr.parse_args::<Expr>()
                    .expect("expected #[value(...)]");

                match expr {
                    Expr::Range(range) => {
                        value_kind = Some(ValueKind::Range(range));
                    }
                    other => {
                        value_kind = Some(ValueKind::Single(other));
                    }
                }
            }
        }

        match value_kind {
            Some(kind) => {
                variants.push(VariantInfo {
                    ident,
                    kind,
                    is_tuple,
                });
            }
            None if is_tuple => {
                // fallback variant like Unknown(u16)
                fallback_variant = Some(ident);
            }
            _ => {}
        }
    }

    // --- Generate match arms
    let mut match_arms_from = Vec::new();
    let mut match_arms_try = Vec::new();

    for v in &variants {
        let ident = &v.ident;

        match &v.kind {
            ValueKind::Single(expr) => {
                if v.is_tuple {
                    match_arms_from.push(quote! {
                        #expr => Self::#ident(v as _),
                    });
                    match_arms_try.push(quote! {
                        #expr => Ok(Self::#ident(v as _)),
                    });
                } else {
                    match_arms_from.push(quote! {
                        #expr => Self::#ident,
                    });
                    match_arms_try.push(quote! {
                        #expr => Ok(Self::#ident),
                    });
                }
            }

            ValueKind::Range(range) => {
                let start = range.start.as_ref()
                    .expect("Range must have a start");
                let end = range.end.as_ref()
                    .expect("Range must have an end");

                // ❗ Only allow inclusive ranges
                if !matches!(range.limits, syn::RangeLimits::Closed(_)) {
                    panic!("FromPrimitive only supports inclusive ranges (use ..=)");
                }

                let pat = quote! { #start ..= #end };

                match_arms_from.push(quote! {
                    #pat => Self::#ident(v as _),
                });

                match_arms_try.push(quote! {
                    #pat => Ok(Self::#ident(v as _)),
                });
            }
        }
    }

    // --- Supported primitives
    let primitives = [
        quote! { u8 }, quote! { u16 }, quote! { u32 }, quote! { u64 }, quote! { usize },
        quote! { i8 }, quote! { i16 }, quote! { i32 }, quote! { i64 }, quote! { isize },
    ];

    let mut impls = Vec::new();

    for prim in primitives {
        if let Some(fallback) = &fallback_variant {
            // Infallible
            impls.push(quote! {
                impl core::convert::From<#prim> for #enum_name {
                    fn from(v: #prim) -> Self {
                        match v as i128 {
                            #(#match_arms_from)*
                            _ => Self::#fallback(v as _),
                        }
                    }
                }
            });
        } else {
            // Fallible
            impls.push(quote! {
                impl core::convert::TryFrom<#prim> for #enum_name {
                    type Error = ();

                    fn try_from(v: #prim) -> Result<Self, Self::Error> {
                        match v as i128 {
                            #(#match_arms_try)*
                            _ => Err(()),
                        }
                    }
                }
            });
        }
    }

    TokenStream::from(quote! {
        #(#impls)*
    })
}

// pub fn expand(input: TokenStream) -> TokenStream {
//     let mut tokens = input.into_iter().peekable();

//     // --- Find enum name
//     let enum_name = loop {
//         match tokens.next() {
//             Some(TokenTree::Ident(id)) if id.to_string() == "enum" => {
//                 if let Some(TokenTree::Ident(name)) = tokens.next() {
//                     break name.to_string();
//                 }
//             }
//             _ => {}
//         }
//     };

//     // --- Find enum body
//     let body = tokens.find_map(|t| {
//         if let TokenTree::Group(g) = t {
//             if g.delimiter() == Delimiter::Brace {
//                 return Some(g.stream());
//             }
//         }
//         None
//     }).expect("Expected enum body");

//     // --- Parse variants
//     let mut variants = Vec::new();
//     let mut unknown_variant = None;

//     let mut iter = body.into_iter().peekable();

//     while let Some(mut token) = iter.next() {
//         let mut value_attr = None;

//         // Parse attributes
//         while let TokenTree::Punct(p) = &token {
//             if p.as_char() == '#' {
//                 if let Some(TokenTree::Group(g)) = iter.next() {
//                     let attr = g.stream().to_string();
//                     if attr.contains("value") {
//                         if let Some(eq_pos) = attr.find('=') {
//                             let val = attr[eq_pos + 1..]
//                                 .replace(']', "")
//                                 .trim()
//                                 .to_string();
//                             value_attr = Some(val);
//                         }
//                     }
//                 }
//                 if let Some(next) = iter.next() {
//                     token = next;
//                 }
//             } else {
//                 break;
//             }
//         }

//         // Variant name
//         let name = match token {
//             TokenTree::Ident(id) => id.to_string(),
//             _ => continue,
//         };

//         // Detect tuple variant (Unknown)
//         let is_tuple = matches!(
//             iter.peek(),
//             Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis
//         );

//         if is_tuple {
//             iter.next(); // consume tuple
//             unknown_variant = Some(name);
//         } else if let Some(val) = value_attr {
//             variants.push((name, val));
//         }
//     }

//     // --- Generate match arms
//     let mut match_arms_try = String::new();
//     let mut match_arms_from = String::new();

//     for (name, val) in &variants {
//         match_arms_try.push_str(&format!(
//             "{} => Ok(Self::{}),",
//             val, name
//         ));

//         match_arms_from.push_str(&format!(
//             "{} => Self::{},",
//             val, name
//         ));
//     }

//     // --- Fallback behavior
//     // let _fallback = if let Some(unknown) = &unknown_variant {
//     //     format!("_ => Ok(Self::{}(v as _)),", unknown)
//     // } else {
//     //     "_ => Err(()),".to_string()
//     // };

//     // --- Supported primitives
//     let primitives = [
//         "u8", "u16", "u32", "u64", "usize",
//         "i8", "i16", "i32", "i64", "isize",
//     ];

//     let mut impls = String::new();

//     for prim in primitives {
//         if let Some(unknown) = &unknown_variant {
//             // --- Generate From (infallible)
//             impls.push_str(&format!(
//                 "impl core::convert::From<{prim}> for {enum_name} {{
//                     fn from(v: {prim}) -> Self {{
//                         match v as i128 {{
//                             {match_arms_from}
//                             _ => Self::{unknown}(v as _),
//                         }}
//                     }}
//                 }}"
//             ));
//         } else {
//             // --- Generate TryFrom (fallible)
//             impls.push_str(&format!(
//                 "impl core::convert::TryFrom<{prim}> for {enum_name} {{
//                     type Error = ();

//                     fn try_from(v: {prim}) -> Result<Self, Self::Error> {{
//                         match v as i128 {{
//                             {match_arms_try}
//                             _ => Err(()),
//                         }}
//                     }}
//                 }}"
//             ));
//         }
//     }

//     impls.parse().unwrap()
// }
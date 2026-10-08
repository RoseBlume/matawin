use proc_macro::TokenStream;
use quote::quote;

use syn::{
    Fields, Type, PathArguments,
    TypePath, TypeArray, DeriveInput, Data,
    TypeReference, GenericArgument, DataEnum, DataStruct
};

pub fn expand(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    match input.data {
        Data::Struct(data) => impl_struct(name, data),
        Data::Enum(data) => impl_enum(name, data),
        _ => panic!("DisplayPretty only supports structs and enums"),
    }
}

//
// -----------------------------
// STRUCT IMPLEMENTATION
// -----------------------------
//

fn impl_struct(name: syn::Ident, data: DataStruct) -> TokenStream {
    let fields = match data.fields {
        Fields::Named(fields) => fields.named,
        _ => panic!("DisplayPretty only supports named structs"),
    };

    let field_writes = fields.iter().map(|f| {
        let ident = f.ident.as_ref().unwrap();
        let ty = &f.ty;

        let pretty_name = ident
            .to_string()
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let (base_ty, is_option, is_seq) = peel_sequences(ty);
        let is_struct_like = !is_primitive_like(base_ty);

        if is_option && is_seq {
            if is_struct_like {
                quote! {
                    if let Some(vec) = &self.#ident {
                        if !vec.is_empty() {
                            writeln!(f, "{}:", #pretty_name)?;
                            for item in vec {
                                writeln!(f, "{}", item)?;
                            }
                        }
                    }
                }
            } else {
                quote! {
                    if let Some(vec) = &self.#ident {
                        if !vec.is_empty() {
                            let joined = vec.iter()
                                .map(|v| format!("{}", v))
                                .collect::<Vec<_>>()
                                .join(", ");
                            writeln!(f, "{}: {}", #pretty_name, joined)?;
                        }
                    }
                }
            }
        } else if is_option {
            if is_struct_like {
                quote! {
                    if let Some(value) = &self.#ident {
                        writeln!(f, "{}:", #pretty_name)?;
                        writeln!(f, "{}", value)?;
                    }
                }
            } else {
                quote! {
                    if let Some(value) = &self.#ident {
                        writeln!(f, "{}: {}", #pretty_name, value)?;
                    }
                }
            }
        } else if is_seq {
            if is_struct_like {
                quote! {
                    if !self.#ident.is_empty() {
                        writeln!(f, "{}:", #pretty_name)?;
                        for item in &self.#ident {
                            writeln!(f, "{}", item)?;
                        }
                    }
                }
            } else {
                quote! {
                    if !self.#ident.is_empty() {
                        let joined = self.#ident.iter()
                            .map(|v| format!("{}", v))
                            .collect::<Vec<_>>()
                            .join(", ");
                        writeln!(f, "{}: {}", #pretty_name, joined)?;
                    }
                }
            }
        } else {
            if is_struct_like {
                quote! {
                    writeln!(f, "{}:", #pretty_name)?;
                    writeln!(f, "{}", self.#ident)?;
                }
            } else {
                quote! {
                    writeln!(f, "{}: {}", #pretty_name, self.#ident)?;
                }
            }
        }
    });

    quote! {
        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                #(#field_writes)*
                Ok(())
            }
        }

        // Optional: make Debug match Display
        impl std::fmt::Debug for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self)
            }
        }
    }.into()
}

//
// -----------------------------
// ENUM IMPLEMENTATION
// -----------------------------
//

fn impl_enum(name: syn::Ident, data: DataEnum) -> TokenStream {
    let variants = data.variants.into_iter().map(|v| {
        let v_name = v.ident;

        match v.fields {
            Fields::Unit => {
                quote! {
                    Self::#v_name => write!(f, "{}", stringify!(#v_name)),
                }
            }

            Fields::Unnamed(fields) => {
                let bindings: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| syn::Ident::new(&format!("v{}", i), v_name.span()))
                    .collect();

                quote! {
                    Self::#v_name( #(#bindings),* ) => {
                        write!(f, "{}(", stringify!(#v_name))?;
                        let mut first = true;
                        #(
                            if !first { write!(f, ", ")?; }
                            write!(f, "{}", #bindings)?;
                            first = false;
                        )*
                        write!(f, ")")
                    }
                }
            }

            Fields::Named(fields) => {
                let names: Vec<_> = fields.named.iter()
                    .map(|f| f.ident.as_ref().unwrap())
                    .collect();

                quote! {
                    Self::#v_name { #(ref #names),* } => {
                        writeln!(f, "{}:", stringify!(#v_name))?;
                        #(writeln!(f, "  {}: {}", stringify!(#names), #names)?;)*
                        Ok(())
                    }
                }
            }
        }
    });

    quote! {
        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    #(#variants)*
                }
            }
        }

        impl std::fmt::Debug for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self)
            }
        }
    }.into()
}

//
// -----------------------------
// TYPE PEELING
// -----------------------------
//

fn peel_sequences<'a>(mut ty: &'a Type) -> (&'a Type, bool, bool) {
    let mut is_option = false;
    let mut is_seq = false;

    loop {
        if let Type::Path(TypePath { path, .. }) = ty {
            if let Some(seg) = path.segments.last() {
                if seg.ident == "Option" {
                    if let PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            is_option = true;
                            ty = inner;
                            continue;
                        }
                    }
                }

                if seg.ident == "Vec" {
                    if let PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            is_seq = true;
                            ty = inner;
                            continue;
                        }
                    }
                }
            }
        }

        if let Type::Reference(TypeReference { elem, .. }) = ty {
            if let Type::Slice(slice) = &**elem {
                is_seq = true;
                ty = &slice.elem;
                continue;
            }
        }

        if let Type::Array(TypeArray { elem, .. }) = ty {
            is_seq = true;
            ty = elem;
            continue;
        }

        break;
    }

    (ty, is_option, is_seq)
}

//
// -----------------------------
// PRIMITIVE CHECK
// -----------------------------
//

fn is_primitive_like(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(seg) = path.segments.last() {
            return matches!(
                seg.ident.to_string().as_str(),
                "String"
                    | "str"
                    | "bool"
                    | "char"
                    | "usize"
                    | "u8" | "u16" | "u32" | "u64" | "u128"
                    | "isize"
                    | "i8" | "i16" | "i32" | "i64" | "i128"
                    | "f32" | "f64"
            );
        }
    }
    false
}
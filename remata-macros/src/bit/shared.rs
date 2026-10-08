use syn::{Attribute, Expr, Lit};

/// Extract #[bit(n)]
pub fn get_bit(attrs: &[Attribute]) -> Option<u32> {
    for attr in attrs {
        if attr.path().is_ident("bit") {
            if let Ok(expr) = attr.parse_args::<Expr>() {
                if let Expr::Lit(expr_lit) = expr {
                    if let Lit::Int(lit) = expr_lit.lit {
                        return Some(lit.base10_parse().unwrap());
                    }
                }
            }
        }
    }
    None
}

// /// Extract integer literal from discriminant
// pub fn get_discriminant(expr: &Expr) -> Option<u32> {
//     if let Expr::Lit(expr_lit) = expr {
//         if let Lit::Int(lit) = &expr_lit.lit {
//             return Some(lit.base10_parse().unwrap());
//         }
//     }
//     None
// }

/// Bit assignment helper (shared between enum + struct)
pub struct BitTracker {
    next: u32,
}

impl BitTracker {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    pub fn assign(&mut self, explicit: Option<u32>) -> u32 {
        if let Some(bit) = explicit {
            self.next = bit + 1;
            bit
        } else {
            let bit = self.next;
            self.next += 1;
            bit
        }
    }
}
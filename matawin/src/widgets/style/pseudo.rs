//! Interactive and structural pseudo-class matching.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Stores the state and data for interaction state.
///
/// Visibility: **public**.
pub struct InteractionState {
    /// Stores hover.
    ///
    /// Visibility: **public**.
    pub hover: bool,
    /// Stores focus.
    ///
    /// Visibility: **public**.
    pub focus: bool,
    /// Stores active.
    ///
    /// Visibility: **public**.
    pub active: bool,
    /// Stores link.
    ///
    /// Visibility: **public**.
    pub link: bool,
    /// Stores visited.
    ///
    /// Visibility: **public**.
    pub visited: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Stores the state and data for tree state.
///
/// Visibility: **public**.
pub struct TreeState {
    /// Stores child index.
    ///
    /// Visibility: **public**.
    pub child_index: Option<usize>,
    /// Stores sibling count.
    ///
    /// Visibility: **public**.
    pub sibling_count: Option<usize>,
    /// Stores lang.
    ///
    /// Visibility: **public**.
    pub lang: Option<String>,
}
impl TreeState {
    #[doc = "Returns whether first. Visibility: public."]
    pub fn is_first(&self) -> bool {
        self.child_index == Some(0)
    }
    #[doc = "Returns whether last. Visibility: public."]
    pub fn is_last(&self) -> bool {
        matches!((self.child_index,self.sibling_count),(Some(i),Some(n)) if n>0&&i+1==n)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Stores the state and data for nth child.
///
/// Visibility: **public**.
pub struct NthChild {
    /// Stores a.
    ///
    /// Visibility: **public**.
    pub a: i32,
    /// Stores b.
    ///
    /// Visibility: **public**.
    pub b: i32,
}
impl NthChild {
    /// Performs the exact operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the exact operation. Visibility: public."]
    pub const fn exact(n: usize) -> Self {
        Self { a: 0, b: n as i32 }
    }
    /// Performs the odd operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the odd operation. Visibility: public."]
    pub const fn odd() -> Self {
        Self { a: 2, b: 1 }
    }
    /// Performs the even operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the even operation. Visibility: public."]
    pub const fn even() -> Self {
        Self { a: 2, b: 0 }
    }
    /// Performs the an plus b operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the an plus b operation. Visibility: public."]
    pub const fn an_plus_b(a: i32, b: i32) -> Self {
        Self { a, b }
    }
    /// Parses parse from the supplied representation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim().to_ascii_lowercase().replace(' ', "");
        if s == "odd" {
            return Ok(Self { a: 2, b: 1 });
        }
        if s == "even" {
            return Ok(Self { a: 2, b: 0 });
        }
        if let Some(npos) = s.find('n') {
            let a = match &s[..npos] {
                "" | "+" => 1,
                "-" => -1,
                v => v.parse().map_err(|_| format!("invalid nth-child: {raw}"))?,
            };
            let tail = &s[npos + 1..];
            let b = if tail.is_empty() {
                0
            } else {
                tail.parse().map_err(|_| format!("invalid nth-child: {raw}"))?
            };
            Ok(Self { a, b })
        } else {
            Ok(Self { a: 0, b: s.parse().map_err(|_| format!("invalid nth-child: {raw}"))? })
        }
    }
    /// Performs the matches operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the matches operation. Visibility: public."]
    pub fn matches(self, zero_based: usize) -> bool {
        let n = (zero_based as i32) + 1;
        if self.a == 0 {
            return n == self.b;
        }
        let d = n - self.b;
        d % self.a == 0 && d / self.a >= 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Defines the available pseudo class choices.
///
/// Visibility: **public**.
pub enum PseudoClass {
    /// Represents the hover variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Hover,
    /// Represents the focus variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Focus,
    /// Represents the active variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Active,
    /// Represents the link variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Link,
    /// Represents the visited variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Visited,
    /// Represents the first child variant.
    ///
    /// Visibility: inherited from the **public** enum.
    FirstChild,
    /// Represents the last child variant.
    ///
    /// Visibility: inherited from the **public** enum.
    LastChild,
    /// Represents the nth child variant.
    ///
    /// Visibility: inherited from the **public** enum.
    NthChild(NthChild),
    /// Represents the lang variant.
    ///
    /// Visibility: inherited from the **public** enum.
    Lang(String),
}
impl PseudoClass {
    #[doc = "Implements the matches operation. Visibility: public."]
    pub fn matches(&self, i: InteractionState, t: &TreeState) -> bool {
        match self {
            Self::Hover => i.hover,
            Self::Focus => i.focus,
            Self::Active => i.active,
            Self::Link => i.link,
            Self::Visited => i.visited,
            Self::FirstChild => t.is_first(),
            Self::LastChild => t.is_last(),
            Self::NthChild(n) => t.child_index.map(|x| n.matches(x)).unwrap_or(false),
            Self::Lang(want) =>
                t.lang
                    .as_deref()
                    .map(|have| {
                        let have = have.to_ascii_lowercase();
                        let want = want.to_ascii_lowercase();
                        have == want || have.starts_with(&(want + "-"))
                    })
                    .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Stores the state and data for pseudo selector.
///
/// Visibility: **public**.
pub struct PseudoSelector {
    /// Stores classes.
    ///
    /// Visibility: **public**.
    pub classes: Vec<PseudoClass>,
}
impl PseudoSelector {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(classes: impl IntoIterator<Item = PseudoClass>) -> Self {
        Self { classes: classes.into_iter().collect() }
    }
    /// Performs the matches operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the matches operation. Visibility: public."]
    pub fn matches(&self, i: InteractionState, t: &TreeState) -> bool {
        self.classes.iter().all(|p| p.matches(i, t))
    }
    /// Parses a pseudo-only selector such as `:hover:first-child`,
    /// `:nth-child(2n+1)`, or `:lang(en)`.
    #[doc = "Implements the parse operation. Visibility: public."]
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut classes = Vec::new();
        let bytes = raw.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            if bytes[i] != b':' {
                return Err(format!("expected ':' in pseudo selector: {raw}"));
            }
            i += 1;
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                i += 1;
            }
            let name = raw[start..i].to_ascii_lowercase();
            let arg = if i < bytes.len() && bytes[i] == b'(' {
                i += 1;
                let a = i;
                let mut depth = 1usize;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'(' {
                        depth += 1;
                    } else if bytes[i] == b')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(format!("unclosed pseudo argument: {raw}"));
                }
                let value = raw[a..i].trim().to_string();
                i += 1;
                Some(value)
            } else {
                None
            };
            classes.push(match name.as_str() {
                "hover" => PseudoClass::Hover,
                "focus" => PseudoClass::Focus,
                "active" => PseudoClass::Active,
                "link" => PseudoClass::Link,
                "visited" => PseudoClass::Visited,
                "first-child" => PseudoClass::FirstChild,
                "last-child" => PseudoClass::LastChild,
                "nth-child" =>
                    PseudoClass::NthChild(
                        NthChild::parse(arg.as_deref().ok_or("nth-child requires an argument")?)?
                    ),
                "lang" => PseudoClass::Lang(arg.ok_or("lang requires an argument")?),
                _ => {
                    return Err(format!("unsupported pseudo-class: :{name}"));
                }
            });
        }
        Ok(Self { classes })
    }
}

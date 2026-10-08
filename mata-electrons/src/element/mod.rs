mod data;

pub use data::ELEMENTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsotopeKind {
    /// The isotope with the largest natural terrestrial isotopic abundance.
    NaturalMostAbundant,
    /// Used when an element does not have a characteristic natural isotopic abundance.
    /// For quiz purposes this is a representative isotope rather than a literal
    /// "most common in nature" isotope.
    Representative,
}

#[derive(Debug, Clone, Copy)]
pub struct Element {
    pub atomic_number: u8,
    pub name: &'static str,
    pub symbol: &'static str,
    pub group: Option<u8>,
    pub period: u8,
    /// Printable periodic-table atomic mass. Brackets indicate a representative
    /// mass number rather than a standard atomic weight.
    pub atomic_mass: &'static str,
    pub isotope_mass_number: u16,
    pub isotope_kind: IsotopeKind,
}

impl Element {
    pub const fn neutron_count(self) -> u16 {
        self.isotope_mass_number - self.atomic_number as u16
    }
    pub fn group_to_cas(&self) -> String {
        if let Some(group) = self.group {
            match group {
                1 => "1a",
                2 => "2a",
                3 => "3b",
                4 => "4b",
                5 => "5b",
                6 => "6b",
                7 => "7b",
                8 | 9 | 10 => "8b",
                11 => "1b",
                12 => "2b",
                13 => "3a",
                14 => "4a",
                15 => "5a",
                16 => "6a",
                17 => "7a",
                18 => "8a",
                _ => ""

            }.to_string()
        }
        else {
            String::from("")
        }
    }

    pub fn group_name(&self) -> String {
        if let Some(group) = self.group {
            match group {
                1 => "Alkali Metals",
                2 => "Alkaline Earths",
                17 => "Halogens",
                18 => "Noble Gases",
                _ => "",
            }.to_string()
        }
        else {
            String::from("")
        }
    }
}

pub fn by_atomic_number(atomic_number: u8) -> Option<&'static Element> {
    if !(1..=118).contains(&atomic_number) {
        return None;
    }

    ELEMENTS.get(atomic_number as usize - 1)
}

pub fn find(value: &str) -> Option<&'static Element> {
    let value = value.trim();

    if let Ok(atomic_number) = value.parse::<u8>() {
        if let Some(element) = by_atomic_number(atomic_number) {
            return Some(element);
        }
    }

    ELEMENTS.iter().find(|element| {
        value.eq_ignore_ascii_case(element.symbol)
            || value.eq_ignore_ascii_case(element.name)
    })
}

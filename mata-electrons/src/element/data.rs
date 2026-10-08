use super::{Element, IsotopeKind};
use IsotopeKind::{NaturalMostAbundant as N, Representative as R};

macro_rules! e {
    ($z:expr, $name:expr, $symbol:expr, $group:expr, $period:expr, $mass:expr, $isotope:expr, $kind:expr) => {
        Element {
            atomic_number: $z,
            name: $name,
            symbol: $symbol,
            group: $group,
            period: $period,
            atomic_mass: $mass,
            isotope_mass_number: $isotope,
            isotope_kind: $kind,
        }
    };
}

pub const ELEMENTS: [Element; 118] = [
    e!(1, "Hydrogen", "H", Some(1), 1, "1.008", 1, N),
    e!(2, "Helium", "He", Some(18), 1, "4.0026", 4, N),
    e!(3, "Lithium", "Li", Some(1), 2, "6.94", 7, N),
    e!(4, "Beryllium", "Be", Some(2), 2, "9.0122", 9, N),
    e!(5, "Boron", "B", Some(13), 2, "10.81", 11, N),
    e!(6, "Carbon", "C", Some(14), 2, "12.011", 12, N),
    e!(7, "Nitrogen", "N", Some(15), 2, "14.007", 14, N),
    e!(8, "Oxygen", "O", Some(16), 2, "15.999", 16, N),
    e!(9, "Fluorine", "F", Some(17), 2, "18.998403", 19, N),
    e!(10, "Neon", "Ne", Some(18), 2, "20.180", 20, N),
    e!(11, "Sodium", "Na", Some(1), 3, "22.989769", 23, N),
    e!(12, "Magnesium", "Mg", Some(2), 3, "24.305", 24, N),
    e!(13, "Aluminum", "Al", Some(13), 3, "26.981538", 27, N),
    e!(14, "Silicon", "Si", Some(14), 3, "28.085", 28, N),
    e!(15, "Phosphorus", "P", Some(15), 3, "30.973762", 31, N),
    e!(16, "Sulfur", "S", Some(16), 3, "32.06", 32, N),
    e!(17, "Chlorine", "Cl", Some(17), 3, "35.45", 35, N),
    e!(18, "Argon", "Ar", Some(18), 3, "39.948", 40, N),
    e!(19, "Potassium", "K", Some(1), 4, "39.0983", 39, N),
    e!(20, "Calcium", "Ca", Some(2), 4, "40.078", 40, N),
    e!(21, "Scandium", "Sc", Some(3), 4, "44.955908", 45, N),
    e!(22, "Titanium", "Ti", Some(4), 4, "47.867", 48, N),
    e!(23, "Vanadium", "V", Some(5), 4, "50.9415", 51, N),
    e!(24, "Chromium", "Cr", Some(6), 4, "51.9961", 52, N),
    e!(25, "Manganese", "Mn", Some(7), 4, "54.938044", 55, N),
    e!(26, "Iron", "Fe", Some(8), 4, "55.845", 56, N),
    e!(27, "Cobalt", "Co", Some(9), 4, "58.933194", 59, N),
    e!(28, "Nickel", "Ni", Some(10), 4, "58.6934", 58, N),
    e!(29, "Copper", "Cu", Some(11), 4, "63.546", 63, N),
    e!(30, "Zinc", "Zn", Some(12), 4, "65.38", 64, N),
    e!(31, "Gallium", "Ga", Some(13), 4, "69.723", 69, N),
    e!(32, "Germanium", "Ge", Some(14), 4, "72.630", 74, N),
    e!(33, "Arsenic", "As", Some(15), 4, "74.921595", 75, N),
    e!(34, "Selenium", "Se", Some(16), 4, "78.971", 80, N),
    e!(35, "Bromine", "Br", Some(17), 4, "79.904", 79, N),
    e!(36, "Krypton", "Kr", Some(18), 4, "83.798", 84, N),
    e!(37, "Rubidium", "Rb", Some(1), 5, "85.4678", 85, N),
    e!(38, "Strontium", "Sr", Some(2), 5, "87.62", 88, N),
    e!(39, "Yttrium", "Y", Some(3), 5, "88.90584", 89, N),
    e!(40, "Zirconium", "Zr", Some(4), 5, "91.224", 90, N),
    e!(41, "Niobium", "Nb", Some(5), 5, "92.90637", 93, N),
    e!(42, "Molybdenum", "Mo", Some(6), 5, "95.95", 98, N),
    e!(43, "Technetium", "Tc", Some(7), 5, "[98]", 98, R),
    e!(44, "Ruthenium", "Ru", Some(8), 5, "101.07", 102, N),
    e!(45, "Rhodium", "Rh", Some(9), 5, "102.90550", 103, N),
    e!(46, "Palladium", "Pd", Some(10), 5, "106.42", 106, N),
    e!(47, "Silver", "Ag", Some(11), 5, "107.8682", 107, N),
    e!(48, "Cadmium", "Cd", Some(12), 5, "112.414", 114, N),
    e!(49, "Indium", "In", Some(13), 5, "114.818", 115, N),
    e!(50, "Tin", "Sn", Some(14), 5, "118.710", 120, N),
    e!(51, "Antimony", "Sb", Some(15), 5, "121.760", 121, N),
    e!(52, "Tellurium", "Te", Some(16), 5, "127.60", 130, N),
    e!(53, "Iodine", "I", Some(17), 5, "126.90447", 127, N),
    e!(54, "Xenon", "Xe", Some(18), 5, "131.293", 132, N),
    e!(55, "Cesium", "Cs", Some(1), 6, "132.905452", 133, N),
    e!(56, "Barium", "Ba", Some(2), 6, "137.327", 138, N),
    e!(57, "Lanthanum", "La", Some(3), 6, "138.90547", 139, N),
    e!(58, "Cerium", "Ce", None, 6, "140.116", 140, N),
    e!(59, "Praseodymium", "Pr", None, 6, "140.90766", 141, N),
    e!(60, "Neodymium", "Nd", None, 6, "144.242", 142, N),
    e!(61, "Promethium", "Pm", None, 6, "[145]", 145, R),
    e!(62, "Samarium", "Sm", None, 6, "150.36", 152, N),
    e!(63, "Europium", "Eu", None, 6, "151.964", 153, N),
    e!(64, "Gadolinium", "Gd", None, 6, "157.25", 158, N),
    e!(65, "Terbium", "Tb", None, 6, "158.92535", 159, N),
    e!(66, "Dysprosium", "Dy", None, 6, "162.500", 164, N),
    e!(67, "Holmium", "Ho", None, 6, "164.93033", 165, N),
    e!(68, "Erbium", "Er", None, 6, "167.259", 166, N),
    e!(69, "Thulium", "Tm", None, 6, "168.93422", 169, N),
    e!(70, "Ytterbium", "Yb", None, 6, "173.045", 174, N),
    e!(71, "Lutetium", "Lu", None, 6, "174.9668", 175, N),
    e!(72, "Hafnium", "Hf", Some(4), 6, "178.49", 180, N),
    e!(73, "Tantalum", "Ta", Some(5), 6, "180.94788", 181, N),
    e!(74, "Tungsten", "W", Some(6), 6, "183.84", 184, N),
    e!(75, "Rhenium", "Re", Some(7), 6, "186.207", 187, N),
    e!(76, "Osmium", "Os", Some(8), 6, "190.23", 192, N),
    e!(77, "Iridium", "Ir", Some(9), 6, "192.217", 193, N),
    e!(78, "Platinum", "Pt", Some(10), 6, "195.084", 195, N),
    e!(79, "Gold", "Au", Some(11), 6, "196.96657", 197, N),
    e!(80, "Mercury", "Hg", Some(12), 6, "200.592", 202, N),
    e!(81, "Thallium", "Tl", Some(13), 6, "204.38", 205, N),
    e!(82, "Lead", "Pb", Some(14), 6, "207.2", 208, N),
    e!(83, "Bismuth", "Bi", Some(15), 6, "208.98040", 209, N),
    e!(84, "Polonium", "Po", Some(16), 6, "[209]", 209, R),
    e!(85, "Astatine", "At", Some(17), 6, "[210]", 210, R),
    e!(86, "Radon", "Rn", Some(18), 6, "[222]", 222, R),
    e!(87, "Francium", "Fr", Some(1), 7, "[223]", 223, R),
    e!(88, "Radium", "Ra", Some(2), 7, "[226]", 226, R),
    e!(89, "Actinium", "Ac", Some(3), 7, "[227]", 227, R),
    e!(90, "Thorium", "Th", None, 7, "232.0377", 232, N),
    e!(91, "Protactinium", "Pa", None, 7, "231.03588", 231, N),
    e!(92, "Uranium", "U", None, 7, "238.02891", 238, N),
    e!(93, "Neptunium", "Np", None, 7, "[237]", 237, R),
    e!(94, "Plutonium", "Pu", None, 7, "[244]", 244, R),
    e!(95, "Americium", "Am", None, 7, "[243]", 243, R),
    e!(96, "Curium", "Cm", None, 7, "[247]", 247, R),
    e!(97, "Berkelium", "Bk", None, 7, "[247]", 247, R),
    e!(98, "Californium", "Cf", None, 7, "[251]", 251, R),
    e!(99, "Einsteinium", "Es", None, 7, "[252]", 252, R),
    e!(100, "Fermium", "Fm", None, 7, "[257]", 257, R),
    e!(101, "Mendelevium", "Md", None, 7, "[258]", 258, R),
    e!(102, "Nobelium", "No", None, 7, "[259]", 259, R),
    e!(103, "Lawrencium", "Lr", None, 7, "[266]", 266, R),
    e!(104, "Rutherfordium", "Rf", Some(4), 7, "[267]", 267, R),
    e!(105, "Dubnium", "Db", Some(5), 7, "[268]", 268, R),
    e!(106, "Seaborgium", "Sg", Some(6), 7, "[269]", 269, R),
    e!(107, "Bohrium", "Bh", Some(7), 7, "[270]", 270, R),
    e!(108, "Hassium", "Hs", Some(8), 7, "[269]", 269, R),
    e!(109, "Meitnerium", "Mt", Some(9), 7, "[277]", 277, R),
    e!(110, "Darmstadtium", "Ds", Some(10), 7, "[282]", 282, R),
    e!(111, "Roentgenium", "Rg", Some(11), 7, "[282]", 282, R),
    e!(112, "Copernicium", "Cn", Some(12), 7, "[286]", 286, R),
    e!(113, "Nihonium", "Nh", Some(13), 7, "[286]", 286, R),
    e!(114, "Flerovium", "Fl", Some(14), 7, "[290]", 290, R),
    e!(115, "Moscovium", "Mc", Some(15), 7, "[290]", 290, R),
    e!(116, "Livermorium", "Lv", Some(16), 7, "[293]", 293, R),
    e!(117, "Tennessine", "Ts", Some(17), 7, "[294]", 294, R),
    e!(118, "Oganesson", "Og", Some(18), 7, "[295]", 295, R),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_indexed_by_atomic_number() {
        for (index, element) in ELEMENTS.iter().enumerate() {
            assert_eq!(element.atomic_number as usize, index + 1);
            assert!(element.isotope_mass_number >= element.atomic_number as u16);
        }
    }

    #[test]
    fn oganesson_neutrons_are_derived_from_representative_isotope() {
        let og = ELEMENTS[117];
        assert_eq!(og.symbol, "Og");
        assert_eq!(og.isotope_mass_number, 295);
        assert_eq!(og.neutron_count(), 177);
    }
}

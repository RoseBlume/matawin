const ORBITALS: [(&str, u8); 19] = [
    ("1s", 2),
    ("2s", 2),
    ("2p", 6),
    ("3s", 2),
    ("3p", 6),
    ("4s", 2),
    ("3d", 10),
    ("4p", 6),
    ("5s", 2),
    ("4d", 10),
    ("5p", 6),
    ("6s", 2),
    ("4f", 14),
    ("5d", 10),
    ("6p", 6),
    ("7s", 2),
    ("5f", 14),
    ("6d", 10),
    ("7p", 6),
];

const NOBLE_GASES: [(u8, &str); 6] = [
    (2, "He"),
    (10, "Ne"),
    (18, "Ar"),
    (36, "Kr"),
    (54, "Xe"),
    (86, "Rn"),
];

pub fn electron_configuration(atomic_number: u8) -> Vec<(&'static str, u8)> {
    let mut remaining = atomic_number;
    let mut result: Vec<(&'static str, u8)> = ORBITALS
        .iter()
        .map(|&(name, _)| (name, 0))
        .collect();

    for (index, &(_, capacity)) in ORBITALS.iter().enumerate() {
        if remaining == 0 {
            break;
        }

        let count = remaining.min(capacity);
        result[index].1 = count;
        remaining -= count;
    }

    apply_exceptions(atomic_number, &mut result);
    result
}

/// Full configurations are printed in Aufbau filling order because that is the
/// order normally taught when constructing the configuration from scratch.
pub fn configuration_string(atomic_number: u8) -> String {
    electron_configuration(atomic_number)
        .into_iter()
        .filter(|(_, electrons)| *electrons > 0)
        .map(|(orbital, electrons)| format!("{orbital}{electrons}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Prints noble-gas shorthand in conventional spectroscopic order.
///
/// Example:
/// Oganesson -> [Rn] 5f14 6d10 7s2 7p6
///
/// This deliberately does NOT preserve raw Aufbau fill order after the noble
/// gas core. That was the cause of the earlier `[Rn] 7s2 5f14 6d10 7p6`
/// output.
pub fn abbreviated_configuration_string(atomic_number: u8) -> String {
    let Some((core_z, core_symbol)) = noble_gas_core(atomic_number) else {
        return configuration_string(atomic_number);
    };

    let full = electron_configuration(atomic_number);
    let core = electron_configuration(core_z);

    let mut remainder: Vec<(&str, u8)> = full
        .iter()
        .filter_map(|&(orbital, count)| {
            if count == 0 {
                return None;
            }

            let core_count = orbital_count(&core, orbital);
            (count != core_count).then_some((orbital, count))
        })
        .collect();

    // Conventional shorthand is easiest to read grouped by principal shell,
    // with s/p/d/f order inside a shell. This gives Fe as [Ar] 3d6 4s2 and
    // Og as [Rn] 5f14 6d10 7s2 7p6.
    remainder.sort_by_key(|(orbital, _)| spectroscopic_key(orbital));

    let mut pieces = vec![format!("[{core_symbol}]")];
    pieces.extend(
        remainder
            .into_iter()
            .map(|(orbital, count)| format!("{orbital}{count}")),
    );

    pieces.join(" ")
}

pub(super) fn noble_gas_atomic_number(symbol: &str) -> Option<u8> {
    NOBLE_GASES
        .iter()
        .find(|(_, noble_symbol)| symbol.eq_ignore_ascii_case(noble_symbol))
        .map(|(z, _)| *z)
}

fn noble_gas_core(atomic_number: u8) -> Option<(u8, &'static str)> {
    NOBLE_GASES
        .iter()
        .rev()
        .find(|&&(z, _)| z < atomic_number)
        .copied()
}

fn orbital_count(configuration: &[(&str, u8)], orbital: &str) -> u8 {
    configuration
        .iter()
        .find(|(name, _)| *name == orbital)
        .map(|(_, count)| *count)
        .unwrap_or(0)
}

fn spectroscopic_key(orbital: &str) -> (u8, u8) {
    let bytes = orbital.as_bytes();
    let shell = bytes[0] - b'0';
    let subshell = match bytes[1] {
        b's' => 0,
        b'p' => 1,
        b'd' => 2,
        b'f' => 3,
        _ => 255,
    };

    (shell, subshell)
}

fn set_orbital(
    orbitals: &mut [(&'static str, u8)],
    name: &str,
    electrons: u8,
) {
    if let Some(orbital) = orbitals.iter_mut().find(|orbital| orbital.0 == name) {
        orbital.1 = electrons;
    }
}

fn apply_exceptions(atomic_number: u8, orbitals: &mut [(&'static str, u8)]) {
    match atomic_number {
        24 => {
            set_orbital(orbitals, "4s", 1);
            set_orbital(orbitals, "3d", 5);
        }
        29 => {
            set_orbital(orbitals, "4s", 1);
            set_orbital(orbitals, "3d", 10);
        }
        41 => {
            set_orbital(orbitals, "5s", 1);
            set_orbital(orbitals, "4d", 4);
        }
        42 => {
            set_orbital(orbitals, "5s", 1);
            set_orbital(orbitals, "4d", 5);
        }
        44 => {
            set_orbital(orbitals, "5s", 1);
            set_orbital(orbitals, "4d", 7);
        }
        45 => {
            set_orbital(orbitals, "5s", 1);
            set_orbital(orbitals, "4d", 8);
        }
        46 => {
            set_orbital(orbitals, "5s", 0);
            set_orbital(orbitals, "4d", 10);
        }
        47 => {
            set_orbital(orbitals, "5s", 1);
            set_orbital(orbitals, "4d", 10);
        }
        57 => {
            set_orbital(orbitals, "4f", 0);
            set_orbital(orbitals, "5d", 1);
        }
        58 => {
            set_orbital(orbitals, "4f", 1);
            set_orbital(orbitals, "5d", 1);
        }
        64 => {
            set_orbital(orbitals, "4f", 7);
            set_orbital(orbitals, "5d", 1);
        }
        78 => {
            set_orbital(orbitals, "6s", 1);
            set_orbital(orbitals, "5d", 9);
        }
        79 => {
            set_orbital(orbitals, "6s", 1);
            set_orbital(orbitals, "5d", 10);
        }
        89 => {
            set_orbital(orbitals, "5f", 0);
            set_orbital(orbitals, "6d", 1);
        }
        90 => {
            set_orbital(orbitals, "5f", 0);
            set_orbital(orbitals, "6d", 2);
        }
        91 => {
            set_orbital(orbitals, "5f", 2);
            set_orbital(orbitals, "6d", 1);
        }
        92 => {
            set_orbital(orbitals, "5f", 3);
            set_orbital(orbitals, "6d", 1);
        }
        93 => {
            set_orbital(orbitals, "5f", 4);
            set_orbital(orbitals, "6d", 1);
        }
        96 => {
            set_orbital(orbitals, "5f", 7);
            set_orbital(orbitals, "6d", 1);
        }
        103 => {
            set_orbital(orbitals, "6d", 0);
            set_orbital(orbitals, "7p", 1);
        }
        111 => {
            set_orbital(orbitals, "7s", 1);
            set_orbital(orbitals, "6d", 10);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oganesson_shorthand_is_in_conventional_order() {
        assert_eq!(
            abbreviated_configuration_string(118),
            "[Rn] 5f14 6d10 7s2 7p6"
        );
    }

    #[test]
    fn iron_shorthand_is_in_conventional_order() {
        assert_eq!(abbreviated_configuration_string(26), "[Ar] 3d6 4s2");
    }
}

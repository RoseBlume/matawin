use super::configuration::{electron_configuration, noble_gas_atomic_number};

pub fn full_configuration_answer_correct(atomic_number: u8, answer: &str) -> bool {
    let Some(user) = parse_configuration(answer) else {
        return false;
    };

    configurations_equal(&user, &electron_configuration(atomic_number))
}

pub fn abbreviated_answer_correct(atomic_number: u8, answer: &str) -> bool {
    let Some(user) = parse_abbreviated_configuration(answer) else {
        return false;
    };

    configurations_equal(&user, &electron_configuration(atomic_number))
}

fn parse_abbreviated_configuration(input: &str) -> Option<Vec<(String, u8)>> {
    let cleaned = normalize_input(input);

    if !cleaned.starts_with('[') {
        return parse_configuration(&cleaned);
    }

    let close = cleaned.find(']')?;
    let core_symbol = &cleaned[1..close];
    let core_z = noble_gas_atomic_number(core_symbol)?;

    let mut result: Vec<(String, u8)> = electron_configuration(core_z)
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(orbital, count)| (orbital.to_string(), count))
        .collect();

    let remainder = cleaned[close + 1..].trim();
    if remainder.is_empty() {
        return Some(result);
    }

    for (orbital, count) in parse_configuration(remainder)? {
        if let Some(existing) = result.iter_mut().find(|(name, _)| *name == orbital) {
            existing.1 = count;
        } else {
            result.push((orbital, count));
        }
    }

    Some(result)
}

fn parse_configuration(input: &str) -> Option<Vec<(String, u8)>> {
    let cleaned = normalize_input(input);
    if cleaned.trim().is_empty() {
        return None;
    }

    let mut result = Vec::new();

    for token in cleaned.split_whitespace() {
        if token.len() < 3 {
            return None;
        }

        let mut chars = token.chars();
        let shell = chars.next()?;
        let orbital_type = chars.next()?.to_ascii_lowercase();

        if !('1'..='7').contains(&shell) {
            return None;
        }

        let capacity = match orbital_type {
            's' => 2,
            'p' => 6,
            'd' => 10,
            'f' => 14,
            _ => return None,
        };

        let electrons = chars.collect::<String>().parse::<u8>().ok()?;
        if electrons > capacity {
            return None;
        }

        let orbital = format!("{shell}{orbital_type}");
        if result.iter().any(|(name, _)| name == &orbital) {
            return None;
        }

        result.push((orbital, electrons));
    }

    Some(result)
}

fn normalize_input(input: &str) -> String {
    input
        .replace(',', " ")
        .replace('^', "")
        .replace('¹', "1")
        .replace('²', "2")
        .replace('³', "3")
        .replace('⁴', "4")
        .replace('⁵', "5")
        .replace('⁶', "6")
        .replace('⁷', "7")
        .replace('⁸', "8")
        .replace('⁹', "9")
        .replace('⁰', "0")
}

fn configurations_equal(user: &[(String, u8)], correct: &[(&str, u8)]) -> bool {
    for &(orbital, expected) in correct {
        let actual = user
            .iter()
            .find(|(name, _)| name == orbital)
            .map(|(_, count)| *count)
            .unwrap_or(0);

        if actual != expected {
            return false;
        }
    }

    for (orbital, actual) in user {
        let expected = correct
            .iter()
            .find(|(name, _)| *name == orbital)
            .map(|(_, count)| *count)
            .unwrap_or(0);

        if *actual != expected {
            return false;
        }
    }

    true
}

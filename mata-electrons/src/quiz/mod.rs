use rand::RngExt;

use crate::{
    electron::{
        abbreviated_answer_correct, abbreviated_configuration_string,
        configuration_string, full_configuration_answer_correct,
    },
    element::{self, Element, IsotopeKind},
    io_utils::input,
};

pub fn run() {
    loop {
        println!();
        println!("======================================");
        println!("       Electron Configuration Quiz");
        println!("======================================");
        println!("1. Lookup element");
        println!("2. Element -> full configuration");
        println!("3. Full configuration -> element");
        println!("4. Element -> abbreviated configuration");
        println!("5. Abbreviated configuration -> element");
        println!("6. Quit");
        println!();

        match input("Choice: ").trim() {
            "1" => lookup(),
            "2" => quiz_full_configuration(),
            "3" => quiz_element_from_full(),
            "4" => quiz_abbreviated_configuration(),
            "5" => quiz_element_from_abbreviated(),
            "6" | "q" | "Q" => break,
            _ => println!("Invalid choice."),
        }
    }
}

fn lookup() {
    println!();
    println!("Enter an element name, symbol, or atomic number.");
    let value = input("> ");

    let Some(element) = element::find(&value) else {
        println!("Element not found.");
        return;
    };

    print_element(element);
}

fn print_element(element: &Element) {
    println!();
    println!("Name:                  {}", element.name);
    println!("Symbol:                {}", element.symbol);
    println!("Atomic number:         {}", element.atomic_number);
    println!("Group:                 {} ({}) {}", format_group(element.group), element.group_to_cas(), element.group_name());
    println!("Period:                {}", element.period);
    println!("Atomic mass:           {} u", element.atomic_mass);

    match element.isotope_kind {
        IsotopeKind::NaturalMostAbundant => {
            println!(
                "Most common isotope:   {}-{}",
                element.symbol, element.isotope_mass_number
            );
            println!("Neutrons in isotope:   {}", element.neutron_count());
        }
        IsotopeKind::Representative => {
            println!(
                "Representative isotope: {}-{}",
                element.symbol, element.isotope_mass_number
            );
            println!("Neutrons in isotope:    {}", element.neutron_count());
        }
    }

    println!(
        "Full configuration:    {}",
        configuration_string(element.atomic_number)
    );
    println!(
        "Abbreviated config:    {}",
        abbreviated_configuration_string(element.atomic_number)
    );
}

fn format_group(group: Option<u8>) -> String {
    match group {
        Some(group) => group.to_string(),
        None => "f-block (no single group number)".to_string(),
    }
}

fn quiz_full_configuration() {
    let element = random_element(1, 118);

    println!();
    println!("Enter the FULL electron configuration for:");
    print_random_hint(element);
    println!();

    let answer = input("> ");

    if full_configuration_answer_correct(element.atomic_number, &answer) {
        println!("Correct!");
    } else {
        println!("Incorrect.");
        println!("Correct answer: {}", configuration_string(element.atomic_number));
    }
}

fn quiz_element_from_full() {
    let element = random_element(1, 118);

    println!();
    println!("Which element has this electron configuration?");
    println!("{}", configuration_string(element.atomic_number));
    println!();

    check_element_answer(element, &input("Element: "));
}

fn quiz_abbreviated_configuration() {
    let element = random_element(3, 118);

    println!();
    println!("Enter the ABBREVIATED electron configuration for:");
    print_random_hint(element);
    println!();

    let answer = input("> ");

    if abbreviated_answer_correct(element.atomic_number, &answer) {
        println!("Correct!");
    } else {
        println!("Incorrect.");
        println!(
            "Correct answer: {}",
            abbreviated_configuration_string(element.atomic_number)
        );
    }
}

fn quiz_element_from_abbreviated() {
    let element = random_element(3, 118);

    println!();
    println!("Which element has this abbreviated configuration?");
    println!("{}", abbreviated_configuration_string(element.atomic_number));
    println!();

    check_element_answer(element, &input("Element: "));
}

fn check_element_answer(correct: &Element, answer: &str) {
    match element::find(answer) {
        Some(answered) if answered.atomic_number == correct.atomic_number => {
            println!("Correct!");
        }
        _ => {
            println!("Incorrect.");
            println!(
                "Correct answer: {} ({}) - atomic number {}",
                correct.name, correct.symbol, correct.atomic_number
            );
        }
    }
}

fn random_element(min: u8, max: u8) -> &'static Element {
    let atomic_number = rand::rng().random_range(min..=max);
    element::by_atomic_number(atomic_number).unwrap()
}

fn print_random_hint(element: &Element) {
    match rand::rng().random_range(0..3) {
        0 => println!("Name: {}", element.name),
        1 => println!("Symbol: {}", element.symbol),
        _ => println!("Atomic number: {}", element.atomic_number),
    }
}

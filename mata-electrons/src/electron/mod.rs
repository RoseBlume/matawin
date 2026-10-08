mod configuration;
mod parser;

pub use configuration::{
    abbreviated_configuration_string,
    configuration_string,
    electron_configuration,
};
pub use parser::{abbreviated_answer_correct, full_configuration_answer_correct};

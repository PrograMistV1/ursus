use crate::value::PropertyId;
use std::fmt;

#[derive(Debug)]
pub enum MaterialError {
    MissingProperty {
        material: String,
        strategy: &'static str,
        property: PropertyId,
    },
    TypeMismatch {
        material: String,
        property: PropertyId,
        expected: &'static str,
        actual: &'static str,
    },
    UnknownStrategy(String),
}

impl fmt::Display for MaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProperty { material, strategy, property } => {
                write!(f, "material '{material}' is missing required property '{property}' for strategy '{strategy}'")
            }
            Self::TypeMismatch { material, property, expected, actual } => write!(
                f,
                "material '{material}' property '{property}' has wrong type: expected {expected}, got {actual}"
            ),
            Self::UnknownStrategy(name) => write!(f, "no shading strategy registered under name '{name}'"),
        }
    }
}

impl std::error::Error for MaterialError {}

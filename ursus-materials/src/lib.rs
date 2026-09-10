pub mod error;
pub mod field;
pub mod layout;
pub mod material;
pub mod pack;
pub mod requirements;
pub mod strategies;
pub mod strategy;
pub mod value;

pub use error::MaterialError;
pub use field::{FieldDesc, FieldType};
pub use layout::MaterialLayout;
pub use material::{Material, MaterialHandle};
pub use requirements::Requirements;
pub use strategy::{ShadingStrategy, StrategyRegistry};
pub use value::{MaterialValue, PropertyId, TextureRef};

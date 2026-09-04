pub mod error;
pub mod material;
pub mod requirements;
pub mod strategies;
pub mod strategy;
pub mod value;

pub use error::MaterialError;
pub use material::{Material, MaterialId};
pub use requirements::Requirements;
pub use strategy::{ShadingStrategy, StrategyRegistry};
pub use value::{MaterialValue, PropertyId, TextureRef};

use crate::material::MaterialHandle;
use crate::strategy::ShadingStrategy;
use crate::value::MaterialValue;

/// Consumes resolved material values and decides how they are laid out in
/// memory - as a single array-of-structs blob (see the `pack::aos` module),
/// as per-field structure-of-arrays buffers, or otherwise. `ShadingStrategy`
/// describes what fields exist and what their values are; `MaterialLayout`
/// decides where those values live.
pub trait MaterialLayout: Send + Sync {
    /// Prepares this layout to store values for `strategy`'s fields. Called
    /// once per strategy, before any `upload` for a material using it.
    fn register_strategy(&mut self, strategy: &dyn ShadingStrategy);

    /// Writes `values` (as resolved by `strategy.resolve()`, in
    /// `strategy.fields()` order) into this layout's storage for `handle`.
    fn upload(&mut self, handle: MaterialHandle, strategy: &dyn ShadingStrategy, values: &[MaterialValue]);
}

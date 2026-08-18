use crate::app::EngineContext;

/// A unit of engine extension: registers whatever an engine module needs
/// (extract systems, tick systems, asset loaders, ...) into an already
/// running [`EngineContext`].
pub trait Plugin {
    fn build(&self, ctx: &mut EngineContext);
}

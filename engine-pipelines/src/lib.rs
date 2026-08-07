pub mod builtin_shaders;
pub mod deferred;
pub mod loading;
pub mod passes;

pub use deferred::DefaultPipeline;
use engine_core::assets::AssetRegistry;
pub use loading::LoadingPipeline;

pub fn register_builtin_loaders(registry: &AssetRegistry) {
    #[cfg(feature = "gltf-loader")]
    registry.register_loader(engine_gltf_loader::GltfLoader);

    #[cfg(feature = "obj-loader")]
    registry.register_loader(engine_obj_loader::ObjLoader);
}

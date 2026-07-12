pub mod builtin_shaders;
pub mod deferred;
pub mod loading;
pub mod passes;

pub use deferred::DefaultPipeline;
pub use loading::LoadingPipeline;

pub fn register_builtin_loaders(registry: &mut engine_core::assets::loader_registry::LoaderRegistry) {
    #[cfg(feature = "gltf-loader")]
    registry.register(engine_gltf_loader::GltfLoader::default());

    #[cfg(feature = "obj-loader")]
    registry.register(engine_obj_loader::ObjLoader::default());
}

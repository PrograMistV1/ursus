use crate::assets::loader_registry::{AssetLoader, LoadedMaterial, LoadedMeshSource, LoadedPrimitive, LoadedTexture};
use crate::assets::loaders;
use std::path::Path;

#[derive(Default)]
pub struct ObjLoader;

impl AssetLoader for ObjLoader {
    fn extensions(&self) -> &[&str] {
        &["obj"]
    }

    fn load(&self, path: &Path) -> anyhow::Result<LoadedMeshSource> {
        let mesh = loaders::load_obj(path)?;
        Ok(LoadedMeshSource {
            primitives: vec![LoadedPrimitive {
                mesh,
                material: None,
                node_translation: [0.0; 3],
                node_rotation: [0.0, 0.0, 0.0, 1.0],
                node_scale: [1.0; 3],
            }],
        })
    }

    fn name(&self) -> &str {
        "obj (built-in)"
    }
}

#[derive(Default)]
pub struct GltfLoader;

impl AssetLoader for GltfLoader {
    fn extensions(&self) -> &[&str] {
        &["gltf", "glb"]
    }

    fn load(&self, path: &Path) -> anyhow::Result<LoadedMeshSource> {
        let primitives = loaders::load_gltf(path)?;
        let primitives = primitives
            .into_iter()
            .map(|p| LoadedPrimitive {
                mesh: p.mesh,
                material: p.material.map(|payload| LoadedMaterial {
                    payload,
                    textures: p
                        .textures
                        .into_iter()
                        .map(|(role, pixels, width, height, _name, _image_index)| {
                            let format = match role.as_str() {
                                "base_color" | "emissive" => ash::vk::Format::R8G8B8A8_SRGB,
                                _ => ash::vk::Format::R8G8B8A8_UNORM,
                            };
                            (role, LoadedTexture { pixels, width, height, format })
                        })
                        .collect(),
                }),
                node_translation: p.node_translation,
                node_rotation: p.node_rotation,
                node_scale: p.node_scale,
            })
            .collect();
        Ok(LoadedMeshSource { primitives })
    }

    fn name(&self) -> &str {
        "gltf (built-in)"
    }
}

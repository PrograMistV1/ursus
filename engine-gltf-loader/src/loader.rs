use crate::materials::{PbrMetallicRoughness, UnlitMaterial};
use crate::tangents::compute_tangents;
use engine_core::assets::loader_registry::{
    AssetLoader, LoadedMaterial, LoadedMeshSource, LoadedPrimitive, LoadedTexture,
};
use engine_core::assets::material::MaterialPayload;
use engine_core::assets::mesh::{CpuMesh, Vertex};
use engine_core::render::gfx::Format;
use glam::{Vec2, Vec3};
use image::DynamicImage;
use std::path::Path;

pub struct GltfPrimitive {
    pub mesh: CpuMesh,
    pub textures: Vec<(String, Vec<u8>, u32, u32, String, usize)>,
    pub material: Option<Box<dyn MaterialPayload>>,
    pub node_translation: [f32; 3],
    pub node_rotation: [f32; 4],
    pub node_scale: [f32; 3],
}

pub fn load_gltf(path: &Path) -> anyhow::Result<Vec<GltfPrimitive>> {
    let (gltf, buffers, images) = gltf::import(path)?;
    let mut primitives = Vec::new();

    for node in gltf.nodes() {
        let Some(mesh) = node.mesh() else { continue };

        let (translation, rotation, scale) = node.transform().decomposed();

        let mesh_base_name = mesh.name().unwrap_or("gltf_mesh").to_string();

        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buf| buffers.get(buf.index()).map(|b| b.0.as_slice()));

            let positions: Vec<Vec3> = reader
                .read_positions()
                .ok_or_else(|| anyhow::anyhow!("Меш '{}' не имеет позиций", mesh_base_name))?
                .map(Vec3::from)
                .collect();

            let normals: Vec<Vec3> = reader
                .read_normals()
                .map(|n| n.map(Vec3::from).collect())
                .unwrap_or_else(|| vec![Vec3::Y; positions.len()]);

            let uvs: Vec<Vec2> = reader
                .read_tex_coords(0)
                .map(|uv| uv.into_f32().map(Vec2::from).collect())
                .unwrap_or_else(|| vec![Vec2::ZERO; positions.len()]);

            let indices: Vec<u32> = reader
                .read_indices()
                .map(|ri| ri.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());

            let tangents: Vec<[f32; 4]> = reader
                .read_tangents()
                .map(|t| t.collect())
                .unwrap_or_else(|| compute_tangents(&positions, &normals, &uvs, &indices));

            let vertices: Vec<Vertex> = positions
                .iter()
                .zip(normals.iter())
                .zip(uvs.iter())
                .zip(tangents.iter())
                .map(|(((pos, normal), uv), tan)| {
                    let mut v = Vertex::new(*pos, *normal, *uv);
                    v.tangent = *tan;
                    v
                })
                .collect();

            let prim_index = primitive.index();
            let mesh_name = if mesh.primitives().len() > 1 {
                format!("{}_{}", mesh_base_name, prim_index)
            } else {
                mesh_base_name.clone()
            };

            log::debug!("glTF '{}': {} вершин, {} индексов", mesh_name, vertices.len(), indices.len());

            let mut tex_data: Vec<(String, Vec<u8>, u32, u32, String, usize)> = Vec::new();
            let mut mat_out: Option<Box<dyn MaterialPayload>> = None;

            if let Some(mat) = primitive.material().index().map(|_| primitive.material()) {
                let pbr = mat.pbr_metallic_roughness();

                let base_color = pbr.base_color_factor();
                let metallic = pbr.metallic_factor();
                let roughness = pbr.roughness_factor();
                let emissive = mat.emissive_factor();
                let name = mat.name().unwrap_or("gltf_material").to_string();

                let is_unlit = mat.unlit();

                mat_out = Some(if is_unlit {
                    Box::new(UnlitMaterial { name, base_color }) as Box<dyn MaterialPayload>
                } else {
                    Box::new(PbrMetallicRoughness { name, base_color, metallic, roughness, emissive })
                        as Box<dyn MaterialPayload>
                });

                if let Some(info) = pbr.base_color_texture() {
                    if let Some((bytes, w, h)) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            "base_color".to_string(),
                            bytes,
                            w,
                            h,
                            format!("{}_diffuse", mesh_name),
                            info.texture().source().index(),
                        ));
                    }
                }

                if let Some(info) = pbr.metallic_roughness_texture() {
                    if let Some((bytes, w, h)) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            "metallic_roughness".to_string(),
                            bytes,
                            w,
                            h,
                            format!("{}_metallic_roughness", mesh_name),
                            info.texture().source().index(),
                        ));
                    }
                }

                if let Some(info) = mat.normal_texture() {
                    if let Some((bytes, w, h)) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            "normal".to_string(),
                            bytes,
                            w,
                            h,
                            format!("{}_normal", mesh_name),
                            info.texture().source().index(),
                        ));
                    }
                }

                if let Some(info) = mat.emissive_texture() {
                    if let Some((bytes, w, h)) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            "emissive".to_string(),
                            bytes,
                            w,
                            h,
                            format!("{}_emissive", mesh_name),
                            info.texture().source().index(),
                        ));
                    }
                }

                if let Some(info) = mat.occlusion_texture() {
                    if let Some((bytes, w, h)) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            "occlusion".to_string(),
                            bytes,
                            w,
                            h,
                            format!("{}_occlusion", mesh_name),
                            info.texture().source().index(),
                        ));
                    }
                }
            }

            primitives.push(GltfPrimitive {
                mesh: CpuMesh::new(mesh_name, vertices, indices),
                textures: tex_data,
                material: mat_out,
                node_translation: translation,
                node_rotation: rotation,
                node_scale: scale,
            });
        }
    }

    if primitives.is_empty() {
        anyhow::bail!("glTF файл не содержит мешей: {:?}", path);
    }

    Ok(primitives)
}

fn image_bytes(images: &[gltf::image::Data], index: usize) -> Option<(Vec<u8>, u32, u32)> {
    let data = images.get(index)?;
    log::debug!("image_bytes: index={} format={:?} {}x{}", index, data.format, data.width, data.height);

    let img = match data.format {
        gltf::image::Format::R8G8B8A8 => {
            image::RgbaImage::from_raw(data.width, data.height, data.pixels.clone()).map(DynamicImage::ImageRgba8)?
        }
        gltf::image::Format::R8G8B8 => {
            image::RgbImage::from_raw(data.width, data.height, data.pixels.clone()).map(DynamicImage::ImageRgb8)?.into()
        }
        _ => {
            log::warn!("Неподдерживаемый формат текстуры glTF: {:?}", data.format);
            return None;
        }
    };
    let rgba = img.into_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some((rgba.into_raw(), w, h))
}

#[derive(Default)]
pub struct GltfLoader;

impl AssetLoader for GltfLoader {
    fn extensions(&self) -> &[&str] {
        &["gltf", "glb"]
    }

    fn load(&self, path: &Path) -> anyhow::Result<LoadedMeshSource> {
        let primitives = load_gltf(path)?;
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
                                "base_color" | "emissive" => Format::Rgba8Srgb,
                                _ => Format::Rgba8Unorm,
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
        "gltf (engine-gltf-loader)"
    }
}

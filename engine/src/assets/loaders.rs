use crate::assets::mesh::{CpuMesh, Vertex};
use crate::assets::shader_registry::TextureSlot;
use glam::{Vec2, Vec3};

pub fn load_obj(path: &std::path::Path) -> anyhow::Result<CpuMesh> {
    let (models, _materials) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
    )?;

    if models.is_empty() {
        anyhow::bail!("OBJ файл не содержит мешей: {:?}", path);
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut base = 0u32;

    for model in &models {
        let mesh = &model.mesh;
        let has_normals = !mesh.normals.is_empty();
        let has_uvs = !mesh.texcoords.is_empty();
        let vertex_count = mesh.positions.len() / 3;

        for i in 0..vertex_count {
            let pos = Vec3::new(
                mesh.positions[i * 3],
                mesh.positions[i * 3 + 1],
                mesh.positions[i * 3 + 2],
            );
            let normal = if has_normals {
                Vec3::new(
                    mesh.normals[i * 3],
                    mesh.normals[i * 3 + 1],
                    mesh.normals[i * 3 + 2],
                )
            } else {
                Vec3::Y
            };
            let uv = if has_uvs {
                Vec2::new(mesh.texcoords[i * 2], 1.0 - mesh.texcoords[i * 2 + 1])
            } else {
                Vec2::ZERO
            };

            vertices.push(Vertex::new(pos, normal, uv));
        }

        for &idx in &mesh.indices {
            indices.push(base + idx);
        }
        base += vertex_count as u32;
    }

    let name = models[0].name.clone();
    log::info!(
        "OBJ '{}': {} вершин, {} индексов",
        name,
        vertices.len(),
        indices.len()
    );
    Ok(CpuMesh::new(name, vertices, indices))
}

pub struct GltfPrimitive {
    pub mesh: CpuMesh,
    /// Сырые байты embedded текстур по слотам.
    /// AssetServer загрузит их через `load_texture_bytes`.
    pub textures: Vec<(TextureSlot, Vec<u8>, String)>,
    pub material: Option<GltfMaterial>,
}

pub struct GltfMaterial {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
}

pub fn load_gltf(path: &std::path::Path) -> anyhow::Result<Vec<GltfPrimitive>> {
    let (gltf, buffers, images) = gltf::import(path)?;
    let mut primitives = Vec::new();

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buf| buffers.get(buf.index()).map(|b| b.0.as_slice()));

            let positions: Vec<Vec3> = reader
                .read_positions()
                .ok_or_else(|| {
                    anyhow::anyhow!("Меш '{}' не имеет позиций", mesh.name().unwrap_or("?"))
                })?
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

            let vertices: Vec<Vertex> = positions
                .iter()
                .zip(normals.iter())
                .zip(uvs.iter())
                .map(|((pos, normal), uv)| Vertex::new(*pos, *normal, *uv))
                .collect();

            let indices: Vec<u32> = reader
                .read_indices()
                .map(|ri| ri.into_u32().collect())
                .unwrap_or_else(|| (0..vertices.len() as u32).collect());

            let mesh_name = mesh.name().unwrap_or("gltf_mesh").to_string();
            log::info!(
                "glTF '{}': {} вершин, {} индексов",
                mesh_name,
                vertices.len(),
                indices.len()
            );

            let mut tex_data: Vec<(TextureSlot, Vec<u8>, String)> = Vec::new();
            let mut mat_out = None;

            if let Some(mat) = primitive.material().index().map(|_| primitive.material()) {
                let pbr = mat.pbr_metallic_roughness();

                let base_color = pbr.base_color_factor();
                let metallic = pbr.metallic_factor();
                let roughness = pbr.roughness_factor();
                let emissive = mat.emissive_factor();

                mat_out = Some(GltfMaterial {
                    name: mat.name().unwrap_or("gltf_material").to_string(),
                    base_color,
                    metallic,
                    roughness,
                    emissive,
                });

                if let Some(info) = pbr.base_color_texture() {
                    if let Some(bytes) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            TextureSlot::Diffuse,
                            bytes,
                            format!("{}_diffuse", mesh_name),
                        ));
                    }
                }

                if let Some(info) = pbr.metallic_roughness_texture() {
                    if let Some(bytes) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            TextureSlot::MetallicRoughness,
                            bytes,
                            format!("{}_metallic_roughness", mesh_name),
                        ));
                    }
                }

                if let Some(info) = mat.normal_texture() {
                    if let Some(bytes) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            TextureSlot::Normal,
                            bytes,
                            format!("{}_normal", mesh_name),
                        ));
                    }
                }

                if let Some(info) = mat.emissive_texture() {
                    if let Some(bytes) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            TextureSlot::Emissive,
                            bytes,
                            format!("{}_emissive", mesh_name),
                        ));
                    }
                }

                if let Some(info) = mat.occlusion_texture() {
                    if let Some(bytes) = image_bytes(&images, info.texture().source().index()) {
                        tex_data.push((
                            TextureSlot::Occlusion,
                            bytes,
                            format!("{}_occlusion", mesh_name),
                        ));
                    }
                }
            }

            primitives.push(GltfPrimitive {
                mesh: CpuMesh::new(mesh_name, vertices, indices),
                textures: tex_data,
                material: mat_out,
            });
        }
    }

    if primitives.is_empty() {
        anyhow::bail!("glTF файл не содержит мешей: {:?}", path);
    }

    Ok(primitives)
}

fn image_bytes(images: &[gltf::image::Data], index: usize) -> Option<Vec<u8>> {
    let data = images.get(index)?;
    // Конвертируем в RGBA8 через image крейт
    use image::DynamicImage;
    let img = match data.format {
        gltf::image::Format::R8G8B8A8 => {
            image::RgbaImage::from_raw(data.width, data.height, data.pixels.clone())
                .map(DynamicImage::ImageRgba8)?
        }
        gltf::image::Format::R8G8B8 => {
            image::RgbImage::from_raw(data.width, data.height, data.pixels.clone())
                .map(DynamicImage::ImageRgb8)?
                .into()
        }
        _ => {
            log::warn!("Неподдерживаемый формат текстуры glTF: {:?}", data.format);
            return None;
        }
    };
    Some(img.into_rgba8().into_raw())
}

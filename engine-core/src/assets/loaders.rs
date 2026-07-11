use crate::assets::mesh::{CpuMesh, Vertex};
use crate::assets::MaterialPayload;
use glam::{Vec2, Vec3};
use image::DynamicImage;

pub fn load_obj(path: &std::path::Path) -> anyhow::Result<CpuMesh> {
    let (models, _materials) =
        tobj::load_obj(path, &tobj::LoadOptions { triangulate: true, single_index: true, ..Default::default() })?;

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
            let pos = Vec3::new(mesh.positions[i * 3], mesh.positions[i * 3 + 1], mesh.positions[i * 3 + 2]);
            let normal = if has_normals {
                Vec3::new(mesh.normals[i * 3], mesh.normals[i * 3 + 1], mesh.normals[i * 3 + 2])
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
    log::info!("OBJ '{}': {} вершин, {} индексов", name, vertices.len(), indices.len());
    Ok(CpuMesh::new(name, vertices, indices))
}

pub struct GltfPrimitive {
    pub mesh: CpuMesh,
    pub textures: Vec<(String, Vec<u8>, u32, u32, String, usize)>,
    pub material: Option<Box<dyn MaterialPayload>>,
    pub node_translation: [f32; 3],
    pub node_rotation: [f32; 4],
    pub node_scale: [f32; 3],
}

pub struct PbrMetallicRoughness {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
}

pub struct UnlitMaterial {
    pub name: String,
    pub base_color: [f32; 4],
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
            let mut mat_out = None;

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

pub fn compute_tangents(positions: &[Vec3], normals: &[Vec3], uvs: &[Vec2], indices: &[u32]) -> Vec<[f32; 4]> {
    let n = positions.len();
    let mut tan1 = vec![Vec3::ZERO; n];
    let mut tan2 = vec![Vec3::ZERO; n];

    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);

        let e1 = positions[i1] - positions[i0];
        let e2 = positions[i2] - positions[i0];
        let du1 = uvs[i1].x - uvs[i0].x;
        let dv1 = uvs[i1].y - uvs[i0].y;
        let du2 = uvs[i2].x - uvs[i0].x;
        let dv2 = uvs[i2].y - uvs[i0].y;

        let r = du1 * dv2 - du2 * dv1;
        if r.abs() < 1e-7 {
            continue;
        }
        let f = 1.0 / r;

        let sdir = (e1 * dv2 - e2 * dv1) * f;
        let tdir = (e2 * du1 - e1 * du2) * f;

        tan1[i0] += sdir;
        tan1[i1] += sdir;
        tan1[i2] += sdir;
        tan2[i0] += tdir;
        tan2[i1] += tdir;
        tan2[i2] += tdir;
    }

    positions
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let n = normals[i];
            let t = tan1[i];
            let tangent = (t - n * n.dot(t)).normalize_or_zero();
            let w = if n.cross(t).dot(tan2[i]) < 0.0 { -1.0f32 } else { 1.0f32 };
            [tangent.x, tangent.y, tangent.z, w]
        })
        .collect()
}

pub fn compute_tangents_flat(vertices: &[Vertex], indices: &[u32]) -> Vec<[f32; 4]> {
    let positions: Vec<Vec3> = vertices.iter().map(|v| Vec3::from(v.position)).collect();
    let normals: Vec<Vec3> = vertices.iter().map(|v| Vec3::from(v.normal)).collect();
    let uvs: Vec<Vec2> = vertices.iter().map(|v| Vec2::from(v.uv)).collect();
    compute_tangents(&positions, &normals, &uvs, indices)
}

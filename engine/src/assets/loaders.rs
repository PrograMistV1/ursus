use crate::assets::mesh::{CpuMesh, Vertex};
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

pub fn load_gltf(path: &std::path::Path) -> anyhow::Result<Vec<CpuMesh>> {
    let (gltf, buffers, _images) = gltf::import(path)?;
    let mut meshes = Vec::new();

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buf| buffers.get(buf.index()).map(|b| b.0.as_slice()));

            let positions: Vec<Vec3> = reader
                .read_positions()
                .ok_or_else(|| {
                    anyhow::anyhow!("Меш '{}' не имеет позиций", mesh.name().unwrap_or("?"))
                })?
                .map(|p| Vec3::from(p))
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

            let name = mesh.name().unwrap_or("gltf_mesh").to_string();
            log::info!(
                "GLTF '{}': {} вершин, {} индексов",
                name,
                vertices.len(),
                indices.len()
            );

            meshes.push(CpuMesh::new(name, vertices, indices));
        }
    }

    if meshes.is_empty() {
        anyhow::bail!("GLTF файл не содержит мешей: {:?}", path);
    }

    Ok(meshes)
}

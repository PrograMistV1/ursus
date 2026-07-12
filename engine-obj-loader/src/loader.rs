use engine_core::assets::loader_registry::{AssetLoader, LoadedMeshSource, LoadedPrimitive};
use engine_core::assets::mesh::{CpuMesh, Vertex};
use glam::{Vec2, Vec3};
use std::path::Path;

pub fn load_obj(path: &Path) -> anyhow::Result<CpuMesh> {
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

#[derive(Default)]
pub struct ObjLoader;

impl AssetLoader for ObjLoader {
    fn extensions(&self) -> &[&str] {
        &["obj"]
    }

    fn load(&self, path: &Path) -> anyhow::Result<LoadedMeshSource> {
        let mesh = load_obj(path)?;
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
        "obj (engine-obj-loader)"
    }
}

use crate::render::gfx::types::{Format, VertexAttribute, VertexFormat, VertexLayout};
use glam::{Vec2, Vec3};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
}
unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}

// TODO(vertex-format): Vertex is opinionated (pipelines-specific) and belongs in ursus-pipelines, not core; make CpuMesh/GpuMesh generic and add a #[derive(VertexFormat)] macro.
impl VertexFormat for Vertex {
    fn layout() -> VertexLayout {
        VertexLayout {
            stride: size_of::<Self>() as u32,
            attributes: vec![
                VertexAttribute { location: 0, format: Format::Rgb32Float, offset: 0 }, // position
                VertexAttribute { location: 1, format: Format::Rgb32Float, offset: 12 }, // normal
                VertexAttribute { location: 2, format: Format::Rg32Float, offset: 24 }, // uv
                VertexAttribute { location: 3, format: Format::Rgba32Float, offset: 32 }, // tangent
            ],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_vertices(vertices: &[Vertex]) -> Self {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in vertices {
            let p = Vec3::from(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        Self { min, max }
    }

    pub fn intersects_frustum(&self, planes: &[glam::Vec4; 6]) -> bool {
        for plane in planes {
            let normal = Vec3::new(plane.x, plane.y, plane.z);
            let p = Vec3::new(
                if normal.x >= 0.0 { self.max.x } else { self.min.x },
                if normal.y >= 0.0 { self.max.y } else { self.min.y },
                if normal.z >= 0.0 { self.max.z } else { self.min.z },
            );
            if normal.dot(p) + plane.w < 0.0 {
                return false;
            }
        }
        true
    }
}

impl Vertex {
    pub fn new(position: Vec3, normal: Vec3, uv: Vec2) -> Self {
        Self { position: position.into(), normal: normal.into(), uv: uv.into(), tangent: [1.0, 0.0, 0.0, 1.0] }
    }
}

#[derive(Debug, Clone)]
pub struct CpuMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub name: String,
}

impl CpuMesh {
    pub fn new(name: impl Into<String>, vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        Self { vertices, indices, name: name.into() }
    }

    pub fn vertex_count(&self) -> u32 {
        self.vertices.len() as u32
    }
    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }

    pub fn triangle() -> Self {
        let vertices = vec![
            Vertex::new(Vec3::new(0.0, 0.5, 0.0), Vec3::Z, Vec2::new(0.5, 0.0)),
            Vertex::new(Vec3::new(-0.5, -0.5, 0.0), Vec3::Z, Vec2::new(0.0, 1.0)),
            Vertex::new(Vec3::new(0.5, -0.5, 0.0), Vec3::Z, Vec2::new(1.0, 1.0)),
        ];
        Self::new("triangle", vertices, vec![0, 1, 2])
    }

    pub fn cube() -> Self {
        let faces: &[(Vec3, Vec3, Vec3)] = &[
            (Vec3::Z, Vec3::X, Vec3::Y),         // front
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y), // back
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),     // right
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),     // left
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),     // top
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),     // bottom
        ];

        let uvs = [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
        ];

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for (normal, right, up) in faces {
            let base = vertices.len() as u32;
            let center = *normal * 0.5;
            let corners = [
                center - *right * 0.5 - *up * 0.5,
                center + *right * 0.5 - *up * 0.5,
                center + *right * 0.5 + *up * 0.5,
                center - *right * 0.5 + *up * 0.5,
            ];
            for (pos, uv) in corners.iter().zip(uvs.iter()) {
                vertices.push(Vertex::new(*pos, *normal, *uv));
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        Self::new("cube", vertices, indices)
    }

    pub fn plane(size: f32, subdivisions: u32) -> Self {
        let n = subdivisions + 1;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for z in 0..n {
            for x in 0..n {
                let fx = x as f32 / subdivisions as f32;
                let fz = z as f32 / subdivisions as f32;
                vertices.push(Vertex::new(
                    Vec3::new((fx - 0.5) * size, 0.0, (fz - 0.5) * size),
                    Vec3::Y,
                    Vec2::new(fx, fz),
                ));
            }
        }

        for z in 0..subdivisions {
            for x in 0..subdivisions {
                let i = z * n + x;
                indices.extend_from_slice(&[i, i + n, i + 1, i + 1, i + n, i + n + 1]);
            }
        }

        Self::new(format!("plane_{size}"), vertices, indices)
    }
}

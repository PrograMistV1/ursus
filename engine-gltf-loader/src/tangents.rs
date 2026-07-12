use engine_core::assets::mesh::Vertex;
use glam::{Vec2, Vec3};

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

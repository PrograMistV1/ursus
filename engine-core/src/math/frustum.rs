use crate::assets::mesh::Aabb;
use glam::{Mat4, Vec4};

pub fn extract_planes(view_proj: Mat4) -> [Vec4; 6] {
    let m = view_proj.transpose();
    let rows = [m.x_axis, m.y_axis, m.z_axis, m.w_axis];

    [
        rows[3] + rows[0], // left
        rows[3] - rows[0], // right
        rows[3] + rows[1], // bottom
        rows[3] - rows[1], // top
        rows[3] + rows[2], // near
        rows[3] - rows[2], // far
    ]
}

pub fn transform_aabb(aabb: &Aabb, model: Mat4) -> Aabb {
    let corners = [
        glam::Vec3::new(aabb.min.x, aabb.min.y, aabb.min.z),
        glam::Vec3::new(aabb.max.x, aabb.min.y, aabb.min.z),
        glam::Vec3::new(aabb.min.x, aabb.max.y, aabb.min.z),
        glam::Vec3::new(aabb.max.x, aabb.max.y, aabb.min.z),
        glam::Vec3::new(aabb.min.x, aabb.min.y, aabb.max.z),
        glam::Vec3::new(aabb.max.x, aabb.min.y, aabb.max.z),
        glam::Vec3::new(aabb.min.x, aabb.max.y, aabb.max.z),
        glam::Vec3::new(aabb.max.x, aabb.max.y, aabb.max.z),
    ];

    let mut min = glam::Vec3::splat(f32::MAX);
    let mut max = glam::Vec3::splat(f32::MIN);
    for c in &corners {
        let world = model.transform_point3(*c);
        min = min.min(world);
        max = max.max(world);
    }
    Aabb { min, max }
}

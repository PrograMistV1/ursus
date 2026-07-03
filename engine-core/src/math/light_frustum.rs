use glam::camera::rh::proj::directx::orthographic;
use glam::camera::rh::view::look_at_mat4;

pub fn compute_light_view_proj(direction: [f32; 3], scene_center: glam::Vec3, scene_radius: f32) -> glam::Mat4 {
    let dir = glam::Vec3::from(direction).normalize();
    let light_pos = scene_center - dir * scene_radius;

    let up = [glam::Vec3::Y, glam::Vec3::Z, glam::Vec3::X]
        .iter()
        .find(|&&u| dir.cross(u).length() > 0.01)
        .copied()
        .unwrap_or(glam::Vec3::Y);

    let view = look_at_mat4(light_pos, scene_center, up);
    let ortho = orthographic(-scene_radius, scene_radius, -scene_radius, scene_radius, 0.1, scene_radius * 2.0);
    ortho * view
}

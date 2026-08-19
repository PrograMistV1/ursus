use crate::passes::light_buffer::{DirectionalLight, GpuPointLight, MAX_POINT_LIGHTS};
use engine_core::assets::upload::GpuUploadRequest;
use engine_core::assets::AssetRegistry;
use engine_core::components::light::{DirectionalLightComponent, PointLightComponent};
use engine_core::math::light_frustum::compute_light_view_proj;
use engine_core::render::extract::ExtractSystem;
use engine_core::render::world::RenderWorld;
use engine_core::GameWorld;
use glam::Mat4;
use std::sync::mpsc::Sender;

#[derive(Clone)]
pub struct ExtractedLights {
    pub directional: DirectionalLight,
    pub point_lights: [GpuPointLight; MAX_POINT_LIGHTS],
    pub point_light_count: u32,
    pub light_view_proj: Mat4,
}

impl Default for ExtractedLights {
    fn default() -> Self {
        Self {
            directional: DirectionalLight { direction: [-0.3, -1.0, -0.2, 0.0], color: [1.0, 0.95, 0.85, 2.0] },
            point_lights: [GpuPointLight { position: [0.0; 4], color: [0.0; 4] }; MAX_POINT_LIGHTS],
            point_light_count: 0,
            light_view_proj: Mat4::IDENTITY,
        }
    }
}

pub struct LightExtract;
impl ExtractSystem for LightExtract {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        _cpu_assets: &mut AssetRegistry,
        _upload_tx: &Sender<GpuUploadRequest>,
    ) {
        let directional = match world.inner.query::<&DirectionalLightComponent>().iter().next() {
            Some(light) => DirectionalLight {
                direction: [light.direction.x, light.direction.y, light.direction.z, 0.0],
                color: light.color,
            },
            None => {
                log::warn!("extract_lights: в мире нет DirectionalLightComponent, используется дефолт");
                let light = DirectionalLightComponent::default();
                DirectionalLight {
                    direction: [light.direction.x, light.direction.y, light.direction.z, 0.0],
                    color: light.color,
                }
            }
        };

        let mut point_lights = [GpuPointLight { position: [0.0; 4], color: [0.0; 4] }; MAX_POINT_LIGHTS];
        let mut point_light_count = 0u32;
        for light in world.inner.query::<&PointLightComponent>().iter() {
            if point_light_count as usize >= MAX_POINT_LIGHTS {
                break;
            }
            point_lights[point_light_count as usize] = GpuPointLight {
                position: [light.position.x, light.position.y, light.position.z, light.radius],
                color: light.color,
            };
            point_light_count += 1;
        }

        let (scene_center, scene_radius) = compute_scene_bounds(rw); //todo: scene_radius must be transmitted via push constant

        let light_dir = glam::Vec3::new(directional.direction[0], directional.direction[1], directional.direction[2]);
        let light_view_proj = compute_light_view_proj(light_dir.into(), scene_center, scene_radius);

        rw.insert(ExtractedLights { directional, point_lights, point_light_count, light_view_proj });
    }
    fn name(&self) -> &'static str {
        "extract_lights"
    }
}

fn compute_scene_bounds(rw: &RenderWorld) -> (glam::Vec3, f32) {
    use engine_core::render::world::ExtractedShadowMeshes;

    let Some(meshes) = rw.get::<ExtractedShadowMeshes>() else {
        return (glam::Vec3::new(0.0, 2.0, 0.0), 20.0); // fallback
    };

    let mut min = glam::Vec3::splat(f32::MAX);
    let mut max = glam::Vec3::splat(f32::MIN);
    let mut any = false;

    for inst in &meshes.instances {
        let Some(local_aabb) = &inst.aabb else { continue };
        let world_aabb = engine_core::math::frustum::transform_aabb(local_aabb, inst.model);
        min = min.min(world_aabb.min);
        max = max.max(world_aabb.max);
        any = true;
    }

    if !any {
        return (glam::Vec3::new(0.0, 2.0, 0.0), 20.0);
    }

    let center = (min + max) * 0.5;
    let radius = (max - min).length() * 0.5;
    (center, radius.max(0.1))
}

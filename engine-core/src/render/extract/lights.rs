use crate::assets::upload::GpuUploadRequest;
use crate::assets::CpuAssetServer;
use crate::components::light::{DirectionalLightComponent, PointLightComponent};
use crate::math::light_frustum::compute_light_view_proj;
use crate::render::extract::ExtractSystem;
use crate::render::gfx::{DirectionalLight, GpuPointLight, MAX_POINT_LIGHTS};
use crate::render::world::{ExtractedLights, RenderWorld};
use crate::GameWorld;
use std::sync::mpsc::Sender;

pub struct LightExtract;
impl ExtractSystem for LightExtract {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        _cpu_assets: &mut CpuAssetServer,
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
    use crate::render::world::ExtractedShadowMeshes;

    let Some(meshes) = rw.get::<ExtractedShadowMeshes>() else {
        return (glam::Vec3::new(0.0, 2.0, 0.0), 20.0); // fallback
    };

    let mut min = glam::Vec3::splat(f32::MAX);
    let mut max = glam::Vec3::splat(f32::MIN);
    let mut any = false;

    for inst in &meshes.instances {
        let Some(local_aabb) = &inst.aabb else { continue };
        let world_aabb = crate::math::frustum::transform_aabb(local_aabb, inst.model);
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

use crate::components::light::{DirectionalLightComponent, PointLightComponent};
use crate::math::light_frustum::compute_light_view_proj;
use crate::render::extract::ExtractSystem;
use crate::render::world::{ExtractedLights, RenderWorld};
use crate::vulkan::resources::light_buffer::{DirectionalLight, GpuPointLight, MAX_POINT_LIGHTS};
use crate::GameWorld;

pub struct LightExtract;
impl ExtractSystem for LightExtract {
    fn extract(&self, world: &GameWorld, rw: &mut RenderWorld) {
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

        let light_dir = glam::Vec3::new(directional.direction[0], directional.direction[1], directional.direction[2]);
        let light_view_proj = compute_light_view_proj(light_dir.into(), glam::Vec3::new(0.0, 2.0, 0.0), 20.0);

        rw.insert(ExtractedLights { directional, point_lights, point_light_count, light_view_proj });
    }
    fn name(&self) -> &'static str {
        "extract_lights"
    }
}

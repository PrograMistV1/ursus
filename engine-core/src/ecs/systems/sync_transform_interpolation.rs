use crate::components::transform::Transform;
use crate::components::transform_interpolation::TransformInterpolation;
use crate::ecs::tick::TickSystem;
use crate::GameWorld;

pub struct SyncTransformInterpolation;

impl TickSystem for SyncTransformInterpolation {
    fn tick(&self, world: &mut GameWorld, _dt: f32) {
        for (transform, interp) in world.inner.query_mut::<(&Transform, &mut TransformInterpolation)>() {
            interp.sync(transform);
        }
    }

    fn name(&self) -> &'static str {
        "sync_transform_interpolation"
    }
}

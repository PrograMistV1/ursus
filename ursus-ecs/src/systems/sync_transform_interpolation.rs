use crate::TickSystem;
use crate::components::transform::Transform;
use crate::components::transform_interpolation::TransformInterpolation;
use hecs::World;

pub struct SyncTransformInterpolation;

impl TickSystem for SyncTransformInterpolation {
    fn tick(&self, world: &mut World, _dt: f32) {
        for (transform, interp) in world.query_mut::<(&Transform, &mut TransformInterpolation)>() {
            interp.sync(transform);
        }
    }

    fn name(&self) -> &'static str {
        "sync_transform_interpolation"
    }
}

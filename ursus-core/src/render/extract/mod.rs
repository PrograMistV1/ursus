mod camera;
pub mod meshes;

use crate::render::extract::camera::CameraExtract;
use crate::render::extract::meshes::MeshExtract;
use crate::render::world::RWorld;
use ursus_ecs::World;

pub trait ExtractSystem: Send + Sync {
    fn extract(&self, world: &World, r_world: &mut RWorld);
    fn name(&self) -> &'static str;
}

pub struct ExtractSchedule {
    systems: Vec<Box<dyn ExtractSystem>>,
}

impl ExtractSchedule {
    pub fn add(&mut self, system: impl ExtractSystem + 'static) {
        self.systems.push(Box::new(system));
    }

    pub fn run(&self, world: &World, r_world: &mut RWorld) {
        for system in &self.systems {
            puffin::profile_scope!("extract_system", system.name());
            system.extract(world, r_world);
        }
    }
}

impl Default for ExtractSchedule {
    fn default() -> Self {
        let mut schedule = ExtractSchedule { systems: Vec::new() };
        schedule.add(CameraExtract);
        schedule.add(MeshExtract);
        schedule
    }
}

mod camera;
pub mod lights;
pub mod meshes;
pub mod ui;

use crate::render::extract::camera::CameraExtract;
use crate::render::extract::lights::LightExtract;
use crate::render::extract::meshes::MeshExtract;
use crate::render::extract::ui::UiExtract;
use crate::render::world::RenderWorld;
use crate::GameWorld;

pub trait ExtractSystem: Send + Sync {
    fn extract(&self, world: &GameWorld, rw: &mut RenderWorld);
    fn name(&self) -> &'static str;
}

pub struct ExtractSchedule {
    systems: Vec<Box<dyn ExtractSystem>>,
}

impl ExtractSchedule {
    pub fn new() -> Self {
        Self { systems: Vec::new() }
    }

    pub fn add(&mut self, system: impl ExtractSystem + 'static) {
        self.systems.push(Box::new(system));
    }

    pub fn run(&self, world: &GameWorld, dst: &mut RenderWorld) {
        for system in &self.systems {
            puffin::profile_scope!("extract_system", system.name());
            system.extract(world, dst);
        }
    }
}

pub fn default_extract_schedule() -> ExtractSchedule {
    let mut schedule = ExtractSchedule::new();
    schedule.add(CameraExtract);
    schedule.add(MeshExtract);
    schedule.add(LightExtract);
    schedule.add(UiExtract);
    schedule
}

mod camera;
pub mod lights;
pub mod meshes;
mod shape_ui;
pub mod ui;

use crate::assets::upload::GpuUploadRequest;
use crate::assets::AssetRegistry;
use crate::render::extract::camera::CameraExtract;
use crate::render::extract::lights::LightExtract;
use crate::render::extract::meshes::MeshExtract;
use crate::render::extract::shape_ui::ShapeUiSystem;
use crate::render::extract::ui::UiExtract;
use crate::render::world::RenderWorld;
use crate::GameWorld;
use std::sync::mpsc::Sender;

pub trait ExtractSystem: Send + Sync {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        cpu_assets: &mut AssetRegistry,
        upload_tx: &Sender<GpuUploadRequest>,
    );
    fn name(&self) -> &'static str;
}

pub struct ExtractSchedule {
    systems: Vec<Box<dyn ExtractSystem>>,
}

impl ExtractSchedule {
    pub fn add(&mut self, system: impl ExtractSystem + 'static) {
        self.systems.push(Box::new(system));
    }

    pub fn run(
        &self,
        world: &GameWorld,
        dst: &mut RenderWorld,
        cpu_assets: &mut AssetRegistry,
        upload_tx: &Sender<GpuUploadRequest>,
    ) {
        for system in &self.systems {
            puffin::profile_scope!("extract_system", system.name());
            system.extract(world, dst, cpu_assets, upload_tx);
        }
    }
}

impl Default for ExtractSchedule {
    fn default() -> Self {
        let mut schedule = ExtractSchedule { systems: Vec::new() };
        schedule.add(CameraExtract);
        schedule.add(MeshExtract);
        schedule.add(LightExtract);
        schedule.add(UiExtract);
        schedule.add(ShapeUiSystem);
        schedule
    }
}

use crate::GameWorld;

pub trait TickSystem: Send + Sync {
    fn tick(&self, world: &mut GameWorld, dt: f32);
    fn name(&self) -> &'static str;
}

pub struct TickSchedule {
    systems: Vec<Box<dyn TickSystem>>,
}

impl TickSchedule {
    pub fn new() -> Self {
        Self { systems: Vec::new() }
    }

    pub fn add(&mut self, system: impl TickSystem + 'static) {
        self.systems.push(Box::new(system));
    }

    pub fn run(&self, world: &mut GameWorld, dt: f32) {
        for system in &self.systems {
            puffin::profile_scope!("tick_system", system.name());
            system.tick(world, dt);
        }
    }
}

impl Default for TickSchedule {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_tick_schedule() -> TickSchedule {
    let mut schedule = TickSchedule::new();
    schedule.add(crate::ecs::systems::SyncTransformInterpolation);
    schedule
}

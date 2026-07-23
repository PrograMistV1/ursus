use hecs::{Entity, World};

pub struct GameWorld {
    pub inner: World,
}

impl GameWorld {
    pub fn new() -> Self {
        Self { inner: World::new() }
    }

    pub fn spawn(&mut self) -> EntityBuilder<'_> {
        EntityBuilder::new(self)
    }

    pub fn despawn(&mut self, entity: Entity) -> Result<(), hecs::NoSuchEntity> {
        self.inner.despawn(entity)
    }

    pub fn entity_count(&self) -> u32 {
        self.inner.len()
    }
}

impl Default for GameWorld {
    fn default() -> Self {
        Self::new()
    }
}

pub trait Component: hecs::Component + Default {
    #[doc(hidden)]
    fn check(component: &mut Self, builder: &hecs::EntityBuilder);
}

pub trait ComponentInit {
    fn on_init(_component: &mut Self, _builder: &hecs::EntityBuilder) {}
}

pub struct EntityBuilder<'w> {
    world: &'w mut GameWorld,
    builder: hecs::EntityBuilder,
}

impl<'w> EntityBuilder<'w> {
    fn new(world: &'w mut GameWorld) -> Self {
        Self { world, builder: hecs::EntityBuilder::new() }
    }

    pub fn insert<T: Component>(mut self, mut component: T) -> Self {
        T::check(&mut component, &self.builder);
        self.builder.add(component);
        self
    }

    pub fn build(mut self) -> Entity {
        self.world.inner.spawn(self.builder.build())
    }
}

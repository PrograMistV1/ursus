use crate::{Component, ComponentInit};
use ursus_materials::MaterialHandle;

impl Component for MaterialHandle {
    fn check(_component: &mut Self, _builder: &hecs::EntityBuilder) {}
}

impl ComponentInit for MaterialHandle {}

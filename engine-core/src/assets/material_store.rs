use crate::assets::asset_registry::TextureHandle;
use crate::assets::material::MaterialPayload;
use crate::components::mesh::MaterialHandle;
use std::collections::HashMap;

#[derive(Default)]
pub struct MaterialStore {
    payloads: HashMap<MaterialHandle, Box<dyn MaterialPayload>>,
    textures: HashMap<MaterialHandle, Vec<(String, TextureHandle)>>,
}

impl MaterialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        handle: MaterialHandle,
        payload: Box<dyn MaterialPayload>,
        texture_slots: Vec<(String, TextureHandle)>,
    ) {
        self.payloads.insert(handle, payload);
        self.textures.insert(handle, texture_slots);
    }

    pub fn get<T: 'static>(&self, handle: MaterialHandle) -> Option<&T> {
        self.payloads.get(&handle)?.as_ref().as_any().downcast_ref::<T>()
    }

    pub fn textures(&self, handle: MaterialHandle) -> &[(String, TextureHandle)] {
        self.textures.get(&handle).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn handles(&self) -> impl Iterator<Item = MaterialHandle> + '_ {
        self.payloads.keys().copied()
    }
}

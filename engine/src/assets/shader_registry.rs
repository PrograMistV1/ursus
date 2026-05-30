use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureSlot {
    Diffuse,           // base color / albedo
    Normal,            // tangent-space normal map
    MetallicRoughness, // R=metallic, G=roughness (glTF convention)
    Emissive,          // emissive color
    Occlusion,         // ambient occlusion
}

impl TextureSlot {
    pub fn index(self) -> usize {
        match self {
            Self::Diffuse => 0,
            Self::Normal => 1,
            Self::MetallicRoughness => 2,
            Self::Emissive => 3,
            Self::Occlusion => 4,
        }
    }
}

pub const MAX_TEXTURE_SLOTS: usize = 5;

#[derive(Debug, Clone)]
pub struct ShaderDef {
    pub name: String,
    pub vert: String,
    pub frag: String,
    pub slots: Vec<TextureSlot>,
}

impl ShaderDef {
    pub fn new(name: impl Into<String>, vert: impl Into<String>, frag: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            vert: vert.into(),
            frag: frag.into(),
            slots: Vec::new(),
        }
    }

    pub fn with_slot(mut self, slot: TextureSlot) -> Self {
        self.slots.push(slot);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShaderHandle(pub u32);

pub struct ShaderRegistry {
    shaders: Vec<ShaderDef>,
    by_name: HashMap<String, ShaderHandle>,
}

impl ShaderRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            shaders: Vec::new(),
            by_name: HashMap::new(),
        };

        reg.register(ShaderDef::new(
            "unlit",
            "shaders/unlit.vert",
            "shaders/unlit.frag",
        ));
        reg.register(
            ShaderDef::new("diffuse", "shaders/mesh.vert", "shaders/mesh.frag")
                .with_slot(TextureSlot::Diffuse),
        );
        reg.register(
            ShaderDef::new("pbr", "shaders/pbr.vert", "shaders/pbr.frag")
                .with_slot(TextureSlot::Diffuse)
                .with_slot(TextureSlot::Normal)
                .with_slot(TextureSlot::MetallicRoughness)
                .with_slot(TextureSlot::Emissive)
                .with_slot(TextureSlot::Occlusion),
        );

        reg
    }

    pub fn register(&mut self, def: ShaderDef) -> ShaderHandle {
        let handle = ShaderHandle(self.shaders.len() as u32);
        self.by_name.insert(def.name.clone(), handle);
        self.shaders.push(def);
        handle
    }

    pub fn get(&self, handle: ShaderHandle) -> Option<&ShaderDef> {
        self.shaders.get(handle.0 as usize)
    }

    pub fn by_name(&self, name: &str) -> Option<ShaderHandle> {
        self.by_name.get(name).copied()
    }

    pub fn diffuse(&self) -> ShaderHandle {
        ShaderHandle(1)
    }

    pub fn pbr(&self) -> ShaderHandle {
        ShaderHandle(2)
    }

    pub fn unlit(&self) -> ShaderHandle {
        ShaderHandle(0)
    }
}

impl Default for ShaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

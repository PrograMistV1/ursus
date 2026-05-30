use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureSlot {
    Diffuse,
    Normal,
    MetallicRoughness,
    Emissive,
    Occlusion,
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

pub enum ShaderSource {
    File(PathBuf),
    Bytes(Vec<u8>),
}

pub struct ShaderDef {
    pub name: String,
    pub vert: ShaderSource,
    pub frag: ShaderSource,
    pub slots: Vec<TextureSlot>,
}

impl ShaderDef {
    pub fn from_files(
        name: impl Into<String>,
        vert: impl Into<PathBuf>,
        frag: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            vert: ShaderSource::File(vert.into()),
            frag: ShaderSource::File(frag.into()),
            slots: Vec::new(),
        }
    }

    pub fn from_bytes(
        name: impl Into<String>,
        vert: Vec<u8>,
        frag: Vec<u8>,
    ) -> Self {
        Self {
            name: name.into(),
            vert: ShaderSource::Bytes(vert),
            frag: ShaderSource::Bytes(frag),
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

struct CompiledShader {
    vert_spv: Vec<u8>,
    frag_spv: Vec<u8>,
}

pub struct ShaderRegistry {
    shaders: Vec<ShaderDef>,
    by_name: HashMap<String, ShaderHandle>,
    compiled: HashMap<ShaderHandle, CompiledShader>,
}

impl ShaderRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            shaders: Vec::new(),
            by_name: HashMap::new(),
            compiled: HashMap::new(),
        };

        reg.register(ShaderDef::from_bytes(
            "unlit",
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")).to_vec(),
        ));

        reg.register(
            ShaderDef::from_bytes(
                "diffuse",
                include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")).to_vec(),
                include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")).to_vec(),
            )
                .with_slot(TextureSlot::Diffuse),
        );

        reg.register(
            ShaderDef::from_bytes(
                "pbr",
                include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")).to_vec(),
                include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")).to_vec(),
            )
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

    pub fn load_spv(&mut self, handle: ShaderHandle) -> anyhow::Result<(&[u8], &[u8])> {
        if !self.compiled.contains_key(&handle) {
            let def = self.shaders
                .get(handle.0 as usize)
                .ok_or_else(|| anyhow::anyhow!("ShaderHandle {:?} не найден", handle))?;

            let vert_spv = load_source(&def.vert)
                .map_err(|e| anyhow::anyhow!("Ошибка загрузки vert шейдера '{}': {}", def.name, e))?;
            let frag_spv = load_source(&def.frag)
                .map_err(|e| anyhow::anyhow!("Ошибка загрузки frag шейдера '{}': {}", def.name, e))?;

            self.compiled.insert(handle, CompiledShader { vert_spv, frag_spv });
            log::info!("Шейдер '{}' загружен", def.name);
        }

        let compiled = &self.compiled[&handle];
        Ok((&compiled.vert_spv, &compiled.frag_spv))
    }

    pub fn unload(&mut self, handle: ShaderHandle) {
        self.compiled.remove(&handle);
    }

    pub fn get(&self, handle: ShaderHandle) -> Option<&ShaderDef> {
        self.shaders.get(handle.0 as usize)
    }

    pub fn by_name(&self, name: &str) -> Option<ShaderHandle> {
        self.by_name.get(name).copied()
    }

    pub fn diffuse(&self) -> ShaderHandle { ShaderHandle(1) }
    pub fn pbr(&self) -> ShaderHandle { ShaderHandle(2) }
    pub fn unlit(&self) -> ShaderHandle { ShaderHandle(0) }
}

impl Default for ShaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn load_source(source: &ShaderSource) -> anyhow::Result<Vec<u8>> {
    match source {
        ShaderSource::Bytes(bytes) => Ok(bytes.clone()),
        ShaderSource::File(path) => {
            std::fs::read(path)
                .map_err(|e| anyhow::anyhow!("Не удалось прочитать {:?}: {}", path, e))
        }
    }
}
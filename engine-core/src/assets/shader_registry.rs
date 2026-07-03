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
    pub frag: Option<ShaderSource>,
    pub slots: Vec<TextureSlot>,
}

impl ShaderDef {
    pub fn from_files(name: impl Into<String>, vert: impl Into<PathBuf>, frag: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            vert: ShaderSource::File(vert.into()),
            frag: Some(ShaderSource::File(frag.into())),
            slots: Vec::new(),
        }
    }

    pub fn from_files_vert_only(name: impl Into<String>, vert: impl Into<PathBuf>) -> Self {
        Self { name: name.into(), vert: ShaderSource::File(vert.into()), frag: None, slots: Vec::new() }
    }

    pub fn from_bytes(name: impl Into<String>, vert: Vec<u8>, frag: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            vert: ShaderSource::Bytes(vert),
            frag: Some(ShaderSource::Bytes(frag)),
            slots: Vec::new(),
        }
    }

    pub fn from_bytes_vert_only(name: impl Into<String>, vert: Vec<u8>) -> Self {
        Self { name: name.into(), vert: ShaderSource::Bytes(vert), frag: None, slots: Vec::new() }
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
    frag_spv: Option<Vec<u8>>,
}

pub struct ShaderRegistry {
    shaders: Vec<ShaderDef>,
    by_name: HashMap<String, ShaderHandle>,
    compiled: HashMap<ShaderHandle, CompiledShader>,
    version: HashMap<ShaderHandle, u32>,
}

impl ShaderRegistry {
    pub fn empty() -> Self {
        Self { shaders: Vec::new(), by_name: HashMap::new(), compiled: HashMap::new(), version: HashMap::new() }
    }

    pub fn register(&mut self, def: ShaderDef) -> ShaderHandle {
        let handle = ShaderHandle(self.shaders.len() as u32);
        self.by_name.insert(def.name.clone(), handle);
        self.shaders.push(def);
        handle
    }

    pub fn register_if_absent(&mut self, def: ShaderDef) -> ShaderHandle {
        if let Some(h) = self.by_name(&def.name) {
            return h;
        }
        self.register(def)
    }

    pub fn load_spv(&mut self, handle: ShaderHandle) -> anyhow::Result<(&[u8], Option<&[u8]>)> {
        if !self.compiled.contains_key(&handle) {
            let def = self
                .shaders
                .get(handle.0 as usize)
                .ok_or_else(|| anyhow::anyhow!("ShaderHandle {:?} не найден", handle))?;

            let vert_spv = load_source(&def.vert)
                .map_err(|e| anyhow::anyhow!("Ошибка загрузки vert шейдера '{}': {}", def.name, e))?;
            let frag_spv = def
                .frag
                .as_ref()
                .map(|src| load_source(src))
                .transpose()
                .map_err(|e| anyhow::anyhow!("Ошибка загрузки frag шейдера '{}': {}", def.name, e))?;

            self.compiled.insert(handle, CompiledShader { vert_spv, frag_spv });
            log::info!("Шейдер '{}' загружен", def.name);
        }

        let compiled = &self.compiled[&handle];
        Ok((&compiled.vert_spv, compiled.frag_spv.as_deref()))
    }

    pub fn unload(&mut self, handle: ShaderHandle) {
        self.compiled.remove(&handle);
    }

    pub fn reload(&mut self, handle: ShaderHandle) -> anyhow::Result<()> {
        self.unload(handle);
        self.load_spv(handle)?;
        *self.version.entry(handle).or_insert(0) += 1;
        Ok(())
    }

    pub fn version(&self, handle: ShaderHandle) -> u32 {
        self.version.get(&handle).copied().unwrap_or(0)
    }

    pub fn get(&self, handle: ShaderHandle) -> Option<&ShaderDef> {
        self.shaders.get(handle.0 as usize)
    }

    pub fn by_name(&self, name: &str) -> Option<ShaderHandle> {
        self.by_name.get(name).copied()
    }
}

impl Default for ShaderRegistry {
    fn default() -> Self {
        Self::empty()
    }
}

fn load_source(source: &ShaderSource) -> anyhow::Result<Vec<u8>> {
    match source {
        ShaderSource::Bytes(bytes) => Ok(bytes.clone()),
        ShaderSource::File(path) => {
            std::fs::read(path).map_err(|e| anyhow::anyhow!("Не удалось прочитать {:?}: {}", path, e))
        }
    }
}

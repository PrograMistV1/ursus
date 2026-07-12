use crate::assets::material::MaterialPayload;
use crate::assets::mesh::CpuMesh;
use crate::render::gfx::Format;
use std::path::Path;
use std::sync::Arc;

pub struct LoadedTexture {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: Format,
}

pub struct LoadedMaterial {
    pub payload: Box<dyn MaterialPayload>,
    pub textures: Vec<(String, LoadedTexture)>,
}

pub struct LoadedPrimitive {
    pub mesh: CpuMesh,
    pub material: Option<LoadedMaterial>,
    pub node_translation: [f32; 3],
    pub node_rotation: [f32; 4],
    pub node_scale: [f32; 3],
}

pub struct LoadedMeshSource {
    pub primitives: Vec<LoadedPrimitive>,
}

pub trait AssetLoader: Send + Sync {
    fn extensions(&self) -> &[&str];
    fn load(&self, path: &Path) -> anyhow::Result<LoadedMeshSource>;
    fn name(&self) -> &str {
        "unnamed loader"
    }
}

#[derive(Default)]
pub struct LoaderRegistry {
    loaders: Vec<Arc<dyn AssetLoader>>,
}

impl LoaderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, loader: impl AssetLoader + 'static) {
        self.register_arc(Arc::new(loader));
    }

    pub fn register_arc(&mut self, loader: Arc<dyn AssetLoader>) {
        for ext in loader.extensions() {
            if let Some(existing) = self.find(ext) {
                if existing.name() == loader.name() {
                    return;
                }
                log::warn!(
                    "LoaderRegistry: расширение '{}' уже обрабатывается '{}', переопределяется '{}'",
                    ext,
                    existing.name(),
                    loader.name()
                );
            }
        }
        self.loaders.push(loader);
    }

    fn find(&self, ext: &str) -> Option<&Arc<dyn AssetLoader>> {
        self.loaders.iter().rev().find(|l| l.extensions().contains(&ext))
    }

    pub fn load(&self, path: &Path) -> anyhow::Result<LoadedMeshSource> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let loader = self.find(&ext).ok_or_else(|| {
            anyhow::anyhow!("нет зарегистрированного загрузчика для расширения '.{}': {:?}", ext, path)
        })?;
        loader.load(path)
    }

    pub fn into_loaders(self) -> Vec<Arc<dyn AssetLoader>> {
        self.loaders
    }
}

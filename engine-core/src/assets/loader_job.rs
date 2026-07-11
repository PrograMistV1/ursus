use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

pub use crate::assets::loader_registry::LoadedMeshSource as MeshSource;
use crate::assets::loader_registry::{AssetLoader, LoaderRegistry};

pub struct TextureSource {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub enum LoaderMessage {
    MeshReady { path: PathBuf, source: MeshSource },
    TextureReady { path: PathBuf, source: TextureSource },
    Error { path: PathBuf, error: String },
}

enum LoaderCommand {
    LoadMesh(PathBuf),
    LoadTexture(PathBuf),
    RegisterLoader(Arc<dyn AssetLoader>),
    Shutdown,
}

pub struct BackgroundLoader {
    cmd_tx: Sender<LoaderCommand>,
    pub msg_rx: Receiver<LoaderMessage>,
}

impl BackgroundLoader {
    pub fn new(initial_registry: LoaderRegistry) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<LoaderCommand>();
        let (msg_tx, msg_rx) = mpsc::channel::<LoaderMessage>();

        thread::Builder::new()
            .name("asset-loader".into())
            .spawn(move || loader_thread(cmd_rx, msg_tx, initial_registry))
            .expect("failed to spawn asset-loader thread");

        Self { cmd_tx, msg_rx }
    }

    pub fn request_mesh(&self, path: PathBuf) {
        let _ = self.cmd_tx.send(LoaderCommand::LoadMesh(path));
    }

    pub fn request_texture(&self, path: PathBuf) {
        let _ = self.cmd_tx.send(LoaderCommand::LoadTexture(path));
    }

    pub fn register_loader(&self, loader: Arc<dyn AssetLoader>) {
        let _ = self.cmd_tx.send(LoaderCommand::RegisterLoader(loader));
    }
}

impl Drop for BackgroundLoader {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(LoaderCommand::Shutdown);
    }
}

fn loader_thread(cmd_rx: Receiver<LoaderCommand>, msg_tx: Sender<LoaderMessage>, mut registry: LoaderRegistry) {
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            LoaderCommand::Shutdown => break,

            LoaderCommand::RegisterLoader(loader) => registry.register_arc(loader),

            LoaderCommand::LoadMesh(path) => {
                let result = registry.load(&path);
                let msg = match result {
                    Ok(source) => LoaderMessage::MeshReady { path, source },
                    Err(e) => LoaderMessage::Error { path, error: e.to_string() },
                };
                if msg_tx.send(msg).is_err() {
                    break;
                }
            }

            LoaderCommand::LoadTexture(path) => {
                let result = load_texture_cpu(&path);
                let msg = match result {
                    Ok(source) => LoaderMessage::TextureReady { path, source },
                    Err(e) => LoaderMessage::Error { path, error: e.to_string() },
                };
                if msg_tx.send(msg).is_err() {
                    break;
                }
            }
        }
    }
}

fn load_texture_cpu(path: &std::path::Path) -> anyhow::Result<TextureSource> {
    let img =
        image::open(path).map_err(|e| anyhow::anyhow!("не удалось загрузить текстуру {:?}: {}", path, e))?.into_rgba8();

    let (width, height) = img.dimensions();
    let pixels = img.into_raw();

    Ok(TextureSource { pixels, width, height })
}

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::assets::loaders;

pub struct MeshSource {
    pub primitives: Vec<loaders::GltfPrimitive>,
}

pub struct TextureSource {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub enum LoaderMessage {
    MeshReady {
        path: PathBuf,
        source: MeshSource,
    },
    TextureReady {
        path: PathBuf,
        source: TextureSource,
    },
    Error {
        path: PathBuf,
        error: String,
    },
}

enum LoaderCommand {
    LoadMesh(PathBuf),
    LoadTexture(PathBuf),
    Shutdown,
}

pub struct BackgroundLoader {
    cmd_tx: Sender<LoaderCommand>,
    pub msg_rx: Receiver<LoaderMessage>,
}

impl BackgroundLoader {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<LoaderCommand>();
        let (msg_tx, msg_rx) = mpsc::channel::<LoaderMessage>();

        thread::Builder::new()
            .name("asset-loader".into())
            .spawn(move || loader_thread(cmd_rx, msg_tx))
            .expect("failed to spawn asset-loader thread");

        Self { cmd_tx, msg_rx }
    }

    pub fn request_mesh(&self, path: PathBuf) {
        let _ = self.cmd_tx.send(LoaderCommand::LoadMesh(path));
    }

    pub fn request_texture(&self, path: PathBuf) {
        let _ = self.cmd_tx.send(LoaderCommand::LoadTexture(path));
    }
}

impl Drop for BackgroundLoader {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(LoaderCommand::Shutdown);
    }
}

fn loader_thread(cmd_rx: Receiver<LoaderCommand>, msg_tx: Sender<LoaderMessage>) {
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            LoaderCommand::Shutdown => break,

            LoaderCommand::LoadMesh(path) => {
                let result = load_mesh_cpu(&path);
                let msg = match result {
                    Ok(source) => LoaderMessage::MeshReady { path, source },
                    Err(e) => LoaderMessage::Error {
                        path,
                        error: e.to_string(),
                    },
                };
                if msg_tx.send(msg).is_err() {
                    break;
                }
            }

            LoaderCommand::LoadTexture(path) => {
                let result = load_texture_cpu(&path);
                let msg = match result {
                    Ok(source) => LoaderMessage::TextureReady { path, source },
                    Err(e) => LoaderMessage::Error {
                        path,
                        error: e.to_string(),
                    },
                };
                if msg_tx.send(msg).is_err() {
                    break;
                }
            }
        }
    }
}

fn load_mesh_cpu(path: &std::path::Path) -> anyhow::Result<MeshSource> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "obj" => {
            let mesh = loaders::load_obj(path)?;
            let primitive = loaders::GltfPrimitive {
                mesh,
                textures: Vec::new(),
                material: None,
                node_translation: [0.0; 3],
                node_rotation: [0.0, 0.0, 0.0, 1.0],
                node_scale: [1.0; 3],
            };
            Ok(MeshSource {
                primitives: vec![primitive],
            })
        }
        "gltf" | "glb" => {
            let primitives = loaders::load_gltf(path)?;
            Ok(MeshSource { primitives })
        }
        _ => anyhow::bail!("неизвестный формат меша: {:?}", path),
    }
}

fn load_texture_cpu(path: &std::path::Path) -> anyhow::Result<TextureSource> {
    let img = image::open(path)
        .map_err(|e| anyhow::anyhow!("не удалось загрузить текстуру {:?}: {}", path, e))?
        .into_rgba8();

    let (width, height) = img.dimensions();
    let pixels = img.into_raw();

    Ok(TextureSource {
        pixels,
        width,
        height,
    })
}

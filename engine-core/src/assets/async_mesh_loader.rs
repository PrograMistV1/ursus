use crate::assets::asset_registry::MeshInstance;
use crate::assets::loader_job::{BackgroundLoader, LoaderMessage};
use crate::assets::loader_registry::AssetLoader;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};

pub(crate) type MeshPathCache = Arc<Mutex<HashMap<PathBuf, Vec<MeshInstance>>>>;

#[derive(Debug, Clone, Default)]
pub struct LoadProgress {
    pub total: usize,
    pub completed: usize,
    pub current: String,
}

impl LoadProgress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            self.completed as f32 / self.total as f32
        }
    }
    pub fn is_done(&self) -> bool {
        self.total == 0 || self.completed >= self.total
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AsyncMeshHandle(pub(crate) PathBuf);

impl AsyncMeshHandle {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Manages background mesh loading: query submission, deduplication
/// along the way, progress tracking, and a cache of the finished results.
pub(crate) struct AsyncMeshLoader {
    loader: BackgroundLoader,
    pending_paths: HashMap<PathBuf, ()>,
    mesh_path_cache: MeshPathCache,
    load_progress: LoadProgress,
}

impl AsyncMeshLoader {
    pub(crate) fn new() -> Self {
        Self {
            loader: BackgroundLoader::new(),
            pending_paths: HashMap::new(),
            mesh_path_cache: Arc::new(Mutex::new(HashMap::new())),
            load_progress: LoadProgress::default(),
        }
    }

    pub(crate) fn request(&mut self, path: impl AsRef<Path>) -> AsyncMeshHandle {
        let path = path.as_ref().to_path_buf();
        if self.pending_paths.contains_key(&path) || self.mesh_path_cache.lock().unwrap().contains_key(&path) {
            return AsyncMeshHandle(path);
        }
        log::trace!("load_mesh_async: {:?}", path);
        self.loader.request_mesh(path.clone());
        self.pending_paths.insert(path.clone(), ());
        self.load_progress.total += 1;
        self.load_progress.current = path.to_string_lossy().to_string();
        AsyncMeshHandle(path)
    }

    pub(crate) fn get_instances(&self, handle: &AsyncMeshHandle) -> Option<Vec<MeshInstance>> {
        self.mesh_path_cache.lock().unwrap().get(&handle.0).cloned()
    }

    pub(crate) fn is_loading(&self) -> bool {
        !self.load_progress.is_done()
    }

    pub(crate) fn progress(&self) -> &LoadProgress {
        &self.load_progress
    }

    pub(crate) fn register_loader(&self, loader: Arc<dyn AssetLoader>) {
        self.loader.register_loader(loader);
    }

    /// Retrieves all accumulated messages from the background thread. Does not block.
    pub(crate) fn poll(&mut self) -> Vec<LoaderMessage> {
        let mut messages = Vec::new();
        loop {
            match self.loader.msg_rx.try_recv() {
                Ok(msg) => messages.push(msg),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    log::warn!("asset-loader thread disconnected");
                    break;
                }
            }
        }
        messages
    }

    /// Marks the mesh loading at `path` as complete: caches the completed
    /// instances (or nothing if the loading failed) and updates the progress.
    pub(crate) fn mark_completed(&mut self, path: &Path, instances: Option<Vec<MeshInstance>>) {
        if let Some(instances) = instances {
            self.mesh_path_cache.lock().unwrap().insert(path.to_path_buf(), instances);
        }
        self.pending_paths.remove(path);
        self.load_progress.completed += 1;
    }

    pub(crate) fn set_current_progress_path(&mut self, path: &Path) {
        self.load_progress.current = path.to_string_lossy().to_string();
    }
}

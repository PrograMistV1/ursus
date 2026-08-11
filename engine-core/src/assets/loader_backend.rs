use crate::assets::loader_job::LoaderMessage;
use crate::assets::loader_registry::AssetLoader;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) trait LoaderBackend: Send {
    fn request_mesh(&self, path: PathBuf);
    fn register_loader(&self, loader: Arc<dyn AssetLoader>);
    /// Non-blocking collection of all cumulative messages.
    fn poll(&mut self) -> Vec<LoaderMessage>;
}

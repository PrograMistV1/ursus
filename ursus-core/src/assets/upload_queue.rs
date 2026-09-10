use crate::assets::upload::GpuUploadRequest;
use std::sync::mpsc::Sender;

/// GPU upload queue accumulated on the game thread during a frame.
///
/// Knows nothing about meshes, textures, or materials - it simply buffers
/// requests until `drain_to`, which empties the queue into a channel where
/// they are picked up by the render thread
/// ([`crate::render::thread::flush_uploads_gpu`]).
#[derive(Default)]
pub(crate) struct UploadQueue {
    pending: Vec<GpuUploadRequest>,
}

impl UploadQueue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, req: GpuUploadRequest) {
        self.pending.push(req);
    }

    pub(crate) fn drain_to(&mut self, tx: &Sender<GpuUploadRequest>) {
        for req in self.pending.drain(..) {
            let _ = tx.send(req);
        }
    }
}

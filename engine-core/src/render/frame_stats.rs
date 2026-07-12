use crate::vulkan::timestamps::GpuFrameTimes;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const FRAME_HISTORY_LEN: usize = 120;

#[derive(Debug, Clone, Default)]
pub struct FrameSample {
    pub cpu_ms: f32,
    pub fps: f32,
}

#[derive(Debug, Default)]
pub struct FrameStatsInner {
    history: VecDeque<FrameSample>,
    last_gpu_times: Option<GpuFrameTimes>,
    smoothed_ms: f32,
}

impl FrameStatsInner {
    fn push(&mut self, sample: FrameSample) {
        if self.history.len() >= FRAME_HISTORY_LEN {
            self.history.pop_front();
        }
        self.history.push_back(sample);
    }
}

#[derive(Clone)]
pub struct FrameStats {
    inner: Arc<Mutex<FrameStatsInner>>,
}

impl FrameStats {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(FrameStatsInner::default())) }
    }

    pub fn record_cpu_frame(&self, cpu_ms: f32) {
        let fps = if cpu_ms > 0.0 { 1000.0 / cpu_ms } else { 0.0 };

        let mut inner = self.inner.lock().unwrap();

        const SMOOTHING: f32 = 0.9;
        if inner.smoothed_ms <= 0.0 {
            inner.smoothed_ms = cpu_ms;
        } else {
            inner.smoothed_ms = inner.smoothed_ms * SMOOTHING + cpu_ms * (1.0 - SMOOTHING);
        }

        inner.push(FrameSample { cpu_ms, fps });
    }

    pub fn record_gpu_times(&self, times: GpuFrameTimes) {
        let mut inner = self.inner.lock().unwrap();
        inner.last_gpu_times = Some(times);
    }

    pub fn current_fps(&self) -> f32 {
        let inner = self.inner.lock().unwrap();
        if inner.smoothed_ms > 0.0 {
            1000.0 / inner.smoothed_ms
        } else {
            0.0
        }
    }

    pub fn history_snapshot(&self) -> Vec<FrameSample> {
        self.inner.lock().unwrap().history.iter().cloned().collect()
    }

    pub fn last_gpu_times(&self) -> Option<GpuFrameTimes> {
        self.inner.lock().unwrap().last_gpu_times.clone()
    }
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new()
    }
}

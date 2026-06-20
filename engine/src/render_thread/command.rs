#[derive(Debug)]
pub enum PipelineKind {
    //todo это бред, нужно это удалить и нормально переписать
    Loading,
    Default,
}

#[derive(Debug)]
pub enum RenderCommand {
    Resize { width: u32, height: u32 },
    SetInternalScale(f32),
    SetExposure(f32),
    SetFsrSharpness(f32),
    SetPipeline(PipelineKind),
    Shutdown,
}

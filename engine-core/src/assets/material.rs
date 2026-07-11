use std::any::Any;

pub trait MaterialPayload: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any + Send + Sync> MaterialPayload for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

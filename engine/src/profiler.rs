pub use puffin::{profile_function, profile_scope};

pub fn init() {
    puffin::set_scopes_on(true);
}

#[inline]
pub fn new_frame() {
    puffin::GlobalProfiler::lock().new_frame();
}

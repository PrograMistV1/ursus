use crate::app::EngineContext;
use crate::components::mesh::MeshHandle;
use crate::components::transform::Transform;
use std::ffi::c_void;

pub struct EngineHandle {
    pub(crate) title: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) validation: bool,
    pub(crate) callbacks: Option<AppCallbacksOwned>,
    pub(crate) clear_color: [f32; 4],
    pub(crate) ctx: *mut EngineContext,
}

#[repr(C)]
pub struct EngineCallbacks {
    pub on_start: Option<unsafe extern "C" fn(handle: *mut EngineHandle, userdata: *mut c_void)>,
    pub on_update: Option<unsafe extern "C" fn(handle: *mut EngineHandle, userdata: *mut c_void, dt: f32)>,
    pub on_stop: Option<unsafe extern "C" fn(handle: *mut EngineHandle, userdata: *mut c_void)>,
    pub userdata: *mut c_void,
}

pub(crate) struct AppCallbacksOwned {
    pub on_start: Option<unsafe extern "C" fn(*mut EngineHandle, *mut c_void)>,
    pub on_update: Option<unsafe extern "C" fn(*mut EngineHandle, *mut c_void, f32)>,
    pub on_stop: Option<unsafe extern "C" fn(*mut EngineHandle, *mut c_void)>,
    pub userdata: *mut c_void,
}

unsafe impl Send for AppCallbacksOwned {}
unsafe impl Sync for AppCallbacksOwned {}

#[unsafe(no_mangle)]
pub extern "C" fn engine_create() -> *mut EngineHandle {
    Box::into_raw(Box::new(EngineHandle {
        title: "engine-core".to_string(),
        width: 1280,
        height: 720,
        validation: cfg!(debug_assertions),
        callbacks: None,
        clear_color: [0.05, 0.05, 0.1, 1.0],
        ctx: std::ptr::null_mut(),
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_destroy(handle: *mut EngineHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_title(handle: *mut EngineHandle, title: *const std::ffi::c_char) {
    if handle.is_null() || title.is_null() {
        return;
    }
    if let Ok(s) = unsafe { std::ffi::CStr::from_ptr(title) }.to_str() {
        unsafe { (*handle).title = s.to_string() };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_size(handle: *mut EngineHandle, width: u32, height: u32) {
    if handle.is_null() {
        return;
    }
    unsafe {
        (*handle).width = width;
        (*handle).height = height;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_validation(handle: *mut EngineHandle, enabled: bool) {
    if handle.is_null() {
        return;
    }
    unsafe { (*handle).validation = enabled };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_clear_color(handle: *mut EngineHandle, r: f32, g: f32, b: f32, a: f32) {
    if handle.is_null() {
        return;
    }
    unsafe { (*handle).clear_color = [r, g, b, a] };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_run(handle: *mut EngineHandle, callbacks: *const EngineCallbacks) -> i32 {
    if handle.is_null() {
        return -1;
    }

    if !callbacks.is_null() {
        let cb = unsafe { &*callbacks };
        unsafe {
            (*handle).callbacks = Some(AppCallbacksOwned {
                on_start: cb.on_start,
                on_update: cb.on_update,
                on_stop: cb.on_stop,
                userdata: cb.userdata,
            });
        }
    }

    let app = FfiApp { handle };
    match crate::app::Engine::run(app) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("[engine-core] engine_run failed: {e}");
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_spawn_mesh(handle: *mut EngineHandle, mesh_id: u32, x: f32, y: f32, z: f32) -> u64 {
    let ctx = ctx_mut(handle);
    if ctx.is_null() {
        return u64::MAX;
    }
    let entity = unsafe { (*ctx).world.spawn().insert(MeshHandle(mesh_id)).insert(Transform::at(x, y, z)).build() };
    entity.id() as u64
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_despawn(handle: *mut EngineHandle, entity_id: u64) {
    let ctx = ctx_mut(handle);
    if ctx.is_null() {
        return;
    }
    let entity = hecs::Entity::from_bits(entity_id).unwrap();
    unsafe {
        let _ = (*ctx).world.despawn(entity);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_transform(
    handle: *mut EngineHandle,
    entity_id: u64,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
) {
    let ctx = ctx_mut(handle);
    if ctx.is_null() {
        return;
    }
    let entity = hecs::Entity::from_bits(entity_id).unwrap();
    unsafe {
        if let Ok(mut t) = (*ctx).world.inner.get::<&mut Transform>(entity) {
            t.position = glam::Vec3::new(x, y, z);
            t.scale = glam::Vec3::splat(scale);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_mesh_cube() -> u32 {
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_mesh_triangle() -> u32 {
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_mesh_plane() -> u32 {
    2
}

struct FfiApp {
    handle: *mut EngineHandle,
}
unsafe impl Send for FfiApp {}

impl FfiApp {
    fn callbacks(&self) -> Option<&AppCallbacksOwned> {
        if self.handle.is_null() {
            return None;
        }
        unsafe { (*self.handle).callbacks.as_ref() }
    }
}

impl crate::app::App for FfiApp {
    fn on_start(&mut self, ctx: &mut EngineContext) {
        unsafe { (*self.handle).ctx = ctx as *mut EngineContext };

        if let Some(cb) = self.callbacks() {
            if let Some(f) = cb.on_start {
                unsafe { f(self.handle, cb.userdata) };
            }
        }
    }

    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32) {
        unsafe { (*self.handle).ctx = ctx as *mut EngineContext };

        if let Some(cb) = self.callbacks() {
            if let Some(f) = cb.on_update {
                unsafe { f(self.handle, cb.userdata, dt) };
            }
        }
    }

    fn on_render(&mut self, _ctx: &mut EngineContext) {}

    fn on_stop(&mut self, ctx: &mut EngineContext) {
        let _ = ctx;
        unsafe { (*self.handle).ctx = std::ptr::null_mut() };

        if let Some(cb) = self.callbacks() {
            if let Some(f) = cb.on_stop {
                unsafe { f(self.handle, cb.userdata) };
            }
        }
    }
}

fn ctx_mut(handle: *mut EngineHandle) -> *mut EngineContext {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let ctx = unsafe { (*handle).ctx };
    if ctx.is_null() {
        eprintln!("[engine-core] API called outside of on_start/on_update");
    }
    ctx
}

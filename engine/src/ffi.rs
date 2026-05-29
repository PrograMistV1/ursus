use std::ffi::c_void;

pub struct EngineHandle {
    pub(crate) title: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) validation: bool,
    pub(crate) callbacks: Option<AppCallbacksOwned>,
}

#[repr(C)]
pub struct EngineCallbacks {
    pub on_start: Option<unsafe extern "C" fn(handle: *mut EngineHandle, userdata: *mut c_void)>,
    pub on_update: Option<unsafe extern "C" fn(handle: *mut EngineHandle, userdata: *mut c_void, dt: f32)>,
    pub on_render: Option<unsafe extern "C" fn(handle: *mut EngineHandle, userdata: *mut c_void)>,
    pub on_stop: Option<unsafe extern "C" fn(handle: *mut EngineHandle, userdata: *mut c_void)>,
    pub userdata: *mut c_void,
}

pub(crate) struct AppCallbacksOwned {
    pub on_start: Option<unsafe extern "C" fn(*mut EngineHandle, *mut c_void)>,
    pub on_update: Option<unsafe extern "C" fn(*mut EngineHandle, *mut c_void, f32)>,
    pub on_render: Option<unsafe extern "C" fn(*mut EngineHandle, *mut c_void)>,
    pub on_stop: Option<unsafe extern "C" fn(*mut EngineHandle, *mut c_void)>,
    pub userdata: *mut c_void,
}

unsafe impl Send for AppCallbacksOwned {}
unsafe impl Sync for AppCallbacksOwned {}

#[unsafe(no_mangle)]
pub extern "C" fn engine_create() -> *mut EngineHandle {
    let handle = Box::new(EngineHandle {
        title: "engine".to_string(),
        width: 1280,
        height: 720,
        validation: cfg!(debug_assertions),
        callbacks: None,
    });
    Box::into_raw(handle)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_destroy(handle: *mut EngineHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_title(handle: *mut EngineHandle, title: *const std::ffi::c_char) {
    if handle.is_null() || title.is_null() { return; }
    let s = unsafe { std::ffi::CStr::from_ptr(title) };
    if let Ok(s) = s.to_str() {
        unsafe { (*handle).title = s.to_string() };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_size(handle: *mut EngineHandle, width: u32, height: u32) {
    if handle.is_null() { return; }
    unsafe {
        (*handle).width = width;
        (*handle).height = height;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_set_validation(handle: *mut EngineHandle, enabled: bool) {
    if handle.is_null() { return; }
    unsafe { (*handle).validation = enabled };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_run(handle: *mut EngineHandle, callbacks: *const EngineCallbacks) -> i32 {
    if handle.is_null() { return -1; }

    if !callbacks.is_null() {
        let cb = unsafe { &*callbacks };
        unsafe {
            (*handle).callbacks = Some(AppCallbacksOwned {
                on_start: cb.on_start,
                on_update: cb.on_update,
                on_render: cb.on_render,
                on_stop: cb.on_stop,
                userdata: cb.userdata,
            });
        }
    }

    let app = FfiApp { handle };
    match crate::app::Engine::run(app) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("[engine] engine_run failed: {e}");
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn engine_draw_frame(handle: *mut EngineHandle, r: f32, g: f32, b: f32, a: f32) -> i32 {
    let _ = (handle, r, g, b, a);
    0
}

struct FfiApp {
    handle: *mut EngineHandle,
}

unsafe impl Send for FfiApp {}

impl crate::app::App for FfiApp {
    fn on_start(&mut self, ctx: &mut crate::app::EngineContext) {
        let _ = ctx;
        if let Some(cb) = self.callbacks() {
            if let Some(f) = cb.on_start {
                unsafe { f(self.handle, cb.userdata) };
            }
        }
    }

    fn on_update(&mut self, ctx: &mut crate::app::EngineContext, dt: f32) {
        let _ = ctx;
        if let Some(cb) = self.callbacks() {
            if let Some(f) = cb.on_update {
                unsafe { f(self.handle, cb.userdata, dt) };
            }
        }
    }

    fn on_render(&mut self, ctx: &mut crate::app::EngineContext) {
        ctx.renderer
            .draw_frame(&ctx.vk, [0.0, 0.0, 0.0, 1.0])
            .expect("draw_frame failed");

        if let Some(cb) = self.callbacks() {
            if let Some(f) = cb.on_render {
                unsafe { f(self.handle, cb.userdata) };
            }
        }
    }

    fn on_stop(&mut self, ctx: &mut crate::app::EngineContext) {
        let _ = ctx;
        if let Some(cb) = self.callbacks() {
            if let Some(f) = cb.on_stop {
                unsafe { f(self.handle, cb.userdata) };
            }
        }
    }
}

impl FfiApp {
    fn callbacks(&self) -> Option<&AppCallbacksOwned> {
        if self.handle.is_null() { return None; }
        unsafe { (*self.handle).callbacks.as_ref() }
    }
}
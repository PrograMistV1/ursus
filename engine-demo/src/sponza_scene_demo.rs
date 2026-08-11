use engine_core::app::window_config::WindowConfig;
use engine_core::app::{App, Engine, EngineContext};
use engine_core::components::camera::{ActiveCamera, CameraComponent};
use engine_core::components::light::DirectionalLightComponent;
use engine_core::components::mesh::MaterialHandle;
use engine_core::components::ui::{UiLayout, UiText};
use engine_core::render::gfx::Format;
use engine_core::render::thread::command::PipelineFactory;
use engine_pipelines::DefaultPipeline;
use glam::{Quat, Vec2, Vec3};

struct MyApp {
    tick: u64,
}

impl MyApp {
    fn new() -> Self {
        Self { tick: 0 }
    }
}

impl App for MyApp {
    fn initial_pipeline() -> PipelineFactory
    where
        Self: Sized,
    {
        PipelineFactory::of::<DefaultPipeline>()
    }

    fn window_config() -> WindowConfig {
        WindowConfig::new().with_title("Sponza Demo").with_size(1600, 900).with_resizable(true)
    }

    fn on_start(&mut self, ctx: &mut EngineContext) {
        let sponza_path = assets_dir().join("sponza/glTF/Sponza.gltf");
        let primitives = engine_gltf_loader::load_gltf(&sponza_path).expect("failed to load Sponza");

        for prim in primitives {
            let mesh_handle = ctx.asset_registry.upload_mesh(prim.mesh);

            let material_handle: Option<MaterialHandle> = prim.material.map(|payload| {
                let texture_slots = prim
                    .textures
                    .into_iter()
                    .map(|(role, pixels, w, h, name, _image_index)| {
                        let format = match role.as_str() {
                            "base_color" | "emissive" => Format::Rgba8Srgb,
                            _ => Format::Rgba8Unorm,
                        };
                        let tex = ctx.asset_registry.upload_texture_rgba8(pixels, w, h, format, name);
                        (role, tex)
                    })
                    .collect();

                ctx.asset_registry.register_material(payload, texture_slots)
            });

            let transform = engine_core::components::transform::Transform {
                position: Vec3::from(prim.node_translation),
                rotation: Quat::from_array(prim.node_rotation),
                scale: Vec3::from(prim.node_scale),
            };

            let mut builder = ctx.world.spawn().insert(mesh_handle).insert(transform);
            if let Some(m) = material_handle {
                builder = builder.insert(m);
            }
            builder.build();
        }

        log::info!("Sponza spawned");

        ctx.world
            .spawn()
            .insert(UiLayout::top_left(Vec2::new(16.0, 16.0)))
            .insert(UiText::new("FPS: 60").with_size(18.0).with_color([1.0; 4]))
            .build();

        ctx.world
            .spawn()
            .insert(CameraComponent {
                eye: Vec3::new(8.0, 4.0, 0.0),
                target: Vec3::new(0.0, 4.0, 0.0),
                z_near: 0.01,
                z_far: 50.0,
                ..Default::default()
            })
            .insert(ActiveCamera)
            .build();

        ctx.world.spawn().insert(DirectionalLightComponent::default()).build();
    }

    fn on_update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        self.tick += 1;

        if self.tick.is_multiple_of(60) {
            let fps = ctx.frame_stats().current_fps();
            for text in ctx.world.inner.query_mut::<&mut UiText>() {
                text.text = format!("FPS: {:.0}", fps);
            }
        }

        let t = self.tick as f32 * 0.003;
        for (cam, _) in ctx.world.inner.query_mut::<(&mut CameraComponent, &ActiveCamera)>() {
            cam.eye = Vec3::new(t.sin() * 9.0, 2.0, t.cos() * 4.0);
            cam.target = Vec3::new(0.0, 2.0, 0.0);
        }
    }

    fn on_render(&mut self, _ctx: &mut EngineContext) {}

    fn on_stop(&mut self, _ctx: &mut EngineContext) {
        log::info!("Stopped after {} ticks", self.tick);
    }
}

fn assets_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .expect("failed to get path to executable file")
        .parent()
        .expect("the executable file must have a parent directory")
        .join("assets")
}

fn main() -> anyhow::Result<()> {
    Engine::run(MyApp::new())
}

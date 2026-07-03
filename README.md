<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/ursus-light.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/ursus.svg">
  <img src="assets/ursus.svg" width="240" alt="ursus logo">
</picture>

# URSUS

### A Vulkan game engine written in Rust

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange?logo=rust)](https://www.rust-lang.org)
[![License: MPL--2.0](https://img.shields.io/badge/License-MPL--2.0-blue)](LICENSE)
[![Vulkan](https://img.shields.io/badge/Vulkan-1.3%2B-red?logo=vulkan)](https://www.vulkan.org)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows-lightgrey)]()
[![Lines of Code](https://img.shields.io/endpoint?url=https%3A%2F%2Ftokei.kojix2.net%2Fbadge%2Fgithub%2FPrograMistV1%2Fursus%2Flines)](https://tokei.kojix2.net/github/PrograMistV1/ursus)
</div>

---

## 📦 engine-core

A custom Vulkan game engine written in Rust, built around a render-graph architecture with automatic barrier tracking, a
deferred rendering pipeline, and a threaded game/render split.

## 📁 Workspace layout

```text
engine-core/       the engine itself: ECS, asset pipeline, Vulkan abstraction, render graph
engine-default/    a batteries-included deferred renderer + built-in shaders, built on engine-core
engine-template/   minimal example application showing how to use the engine
```

`engine-core` has no opinion about *how* you render things — it gives you the plumbing (device/swapchain setup, render
graph, resource pool, asset loading, ECS). `engine-default` is one opinionated pipeline built on top of that plumbing.
You could write your own pipeline crate instead and skip `engine-default` entirely.

## ⭐ Features

- **Render graph** (`engine-core/src/render/graph.rs`) — passes declare read/write access to resources; the graph
  topologically sorts them and inserts image layout barriers automatically. See [Render graph](#-render-graph) below.
- **Deferred pipeline** (`engine-default`) — shadow pass → depth prepass → GBuffer (albedo/normal) → lighting →
  tonemap/post-process → FSR1 (EASU + RCAS) upscale → UI overlay.
- **Bindless textures** — a single descriptor set with `update_after_bind` + `PARTIALLY_BOUND`, indexed via
  `nonuniformEXT` in shaders. No per-draw descriptor churn.
- **Threaded renderer** — game logic and rendering run on separate threads, synchronized via a lock-free triple buffer (
  `render/triple_buffer.rs`). The render thread never blocks on game logic.
- **Async asset loading** — a background thread loads `.obj`/`.gltf`/`.glb` off the hot path; results flow back through
  a channel and get GPU-uploaded when ready.
- **Text rendering** — `cosmic-text` for shaping + a custom glyph atlas (`etagere`-backed packer) for rasterized text.
- **GPU profiling** — per-pass timestamp queries (`vulkan/timestamps.rs`) feeding into `puffin` for frame-time
  breakdowns.

## 🕸️ Render graph

Passes are built declaratively and don't know about each other directly — dependencies are inferred from resource
access:

```rust
pass("lighting")
.read(h_gbuffer_albedo, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
.read(h_gbuffer_normal, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
.read(h_shadow_map, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
.write(h_hdr, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
.bind_sampled(h_gbuffer_albedo, lighting_pass.descriptor_set, 0, sampler)
.record( move | cmd, pool, rw, gpu| { /* ... */ })
.build(graph);
```

`RenderGraph::compile()` builds a dependency graph from these read/write accesses, topologically sorts the passes, and
precomputes the minimal set of `VkImageMemoryBarrier2`s needed between them. Resources are either:

- **Transient** — sized relative to internal or output resolution, allocated/resized by the graph (`ResourcePool`)
- **External** — swapchain images, whose layout is managed at frame boundaries

Current pipeline order in `engine-default`:

```text
shadow ---------+
                |
                +--> geometry --> lighting --> post_process
                |                                  |
depth_prepass --+                                  v
                           fsr_easu --> fsr_rcas --> blit_to_swapchain --> ui
```

(`depth_prepass` and `shadow` have no dependency on each other and could run in parallel on hardware/APIs that support
it — the graph doesn't currently exploit that, it just orders them consistently.)

## 🛠️ Building

Requires the **Vulkan SDK** installed with `glslc` on your `PATH` — shaders are compiled to SPIR-V at build time via
`build.rs` (`engine-default/build.rs`), there's no runtime shader compilation.

```bash
cargo build --release
cargo run -p engine-template
```

If `glslc` isn't found, the build fails immediately with a clear panic rather than silently skipping shader compilation.

## 🎮 Example usage

See `engine-template/src/main.rs` for a full example. Minimal shape:

```rust
struct MyApp;

impl App for MyApp {
    fn initial_pipeline() -> PipelineFactory { PipelineFactory::of::<LoadingPipeline>() }

    fn on_start(&mut self, ctx: &mut EngineContext) {
        ctx.world.spawn().insert(CameraComponent::default()).insert(ActiveCamera).build();
        ctx.world.spawn().insert(DirectionalLightComponent::default()).build();
    }

    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32) { /* game logic */ }
    fn on_render(&mut self, _ctx: &mut EngineContext) {}
    fn on_stop(&mut self, _ctx: &mut EngineContext) {}
}

fn main() -> anyhow::Result<()> {
    Engine::run(MyApp)
}
```

Entities are plain `hecs` ECS entities (`GameWorld` wraps `hecs::World`). Each fixed tick, an `ExtractSchedule` copies
relevant ECS state into a `RenderWorld` snapshot, which is published to the render thread via the triple buffer.

## 🚧 Status / known rough edges

- Single discrete-GPU assumption in device selection (falls back to first suitable device if none found).
- No parallelism exploited in the render graph yet — passes execute sequentially even when independent.
- Some internal log/error strings are in Russian; not yet standardized to one language throughout.

## 📄 License

This project is licensed under the Mozilla Public License 2.0 (MPL-2.0). See the [LICENSE](LICENSE) file for details.

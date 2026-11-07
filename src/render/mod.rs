//! The 3D view: bodies drawn with wgpu through eframe's renderer. It only reads the simulation's
//! snapshot (through [`scene::build`]) and never changes it.
//!
//! The scene is rendered into our own multisampled color and depth textures during egui's
//! `prepare` step, then copied into egui's frame where the central panel is. egui's own render
//! pass has no depth buffer, and this way the scene's depth format and multisampling are ours.

pub mod scene;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui_wgpu::{self, wgpu, CallbackResources, CallbackTrait, ScreenDescriptor};
use egui::{Painter, Rect};
use glam::Mat4;

use scene::{Instance, Scene, Uniforms};

const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Creates the renderer's GPU resources. Call once at startup.
pub fn init(render_state: &egui_wgpu::RenderState) {
    let renderer = SceneRenderer::new(&render_state.device, render_state.target_format);
    render_state
        .renderer
        .write()
        .callback_resources
        .insert(renderer);
}

/// A request to save the 3D view to a PNG file. Used to check the view without capturing the
/// screen, see `SOLAR_CAPTURE` in main.rs.
#[derive(Clone)]
pub struct Capture {
    pub path: PathBuf,
    /// Set once the file is written
    pub done: Arc<AtomicBool>,
}

/// Draws `scene` into `rect`, and saves it if `capture` is given
pub fn paint(painter: &Painter, rect: Rect, scene: &Scene, capture: Option<Capture>) {
    let callback = SceneCallback {
        instances: scene.instances.clone(),
        projection: scene.projection,
        rect,
        capture,
    };
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, callback));
}

struct SceneCallback {
    instances: Vec<Instance>,
    projection: Mat4,
    rect: Rect,
    capture: Option<Capture>,
}

impl CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let renderer: &mut SceneRenderer = resources.get_mut().expect("Renderer not initialized");
        let pixels = |points: f32| ((points * screen.pixels_per_point).round() as u32).max(1);
        let size = [pixels(self.rect.width()), pixels(self.rect.height())];
        let uniforms = scene::uniforms(self.projection, size[1]);

        // A copy recorded last frame has been submitted by now
        if let Some(pending) = renderer.pending_capture.take() {
            pending.save(device);
        }
        renderer.render(device, queue, encoder, size, &self.instances, &uniforms);
        if let Some(capture) = &self.capture {
            if !capture.done.load(Ordering::Relaxed) {
                renderer.pending_capture = Some(renderer.copy_out(device, encoder, capture));
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let renderer: &SceneRenderer = resources.get().expect("Renderer not initialized");
        renderer.blit(render_pass);
    }
}

/// The offscreen textures, recreated when the viewport changes size
struct Targets {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// The multisampled color resolved into one sample per pixel, for copying out
    resolved_texture: wgpu::Texture,
    resolved: wgpu::TextureView,
    blit_bind_group: wgpu::BindGroup,
}

/// A frame copied into a buffer, waiting for the GPU to finish before it's saved
struct PendingCapture {
    buffer: wgpu::Buffer,
    size: [u32; 2],
    bytes_per_row: u32,
    capture: Capture,
}

impl PendingCapture {
    fn save(self, device: &wgpu::Device) {
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| {
                result.expect("Failed to read capture")
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("Failed to wait for the GPU");

        let [width, height] = self.size;
        let mapped = self
            .buffer
            .slice(..)
            .get_mapped_range()
            .expect("Capture buffer not mapped");
        let pixels: Vec<u8> = mapped
            .chunks(self.bytes_per_row as usize)
            .flat_map(|row| &row[..width as usize * 4])
            .copied()
            .collect();
        // The texture holds sRGB encoded RGBA, which is what PNG expects
        if let Err(error) = image::save_buffer(
            &self.capture.path,
            &pixels,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        ) {
            eprintln!("Failed to save {}: {error}", self.capture.path.display());
        }
        self.capture.done.store(true, Ordering::Relaxed);
    }
}

struct SceneRenderer {
    sphere_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
    pending_capture: Option<PendingCapture>,
}

impl SceneRenderer {
    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let spheres = device.create_shader_module(wgpu::include_wgsl!("spheres.wgsl"));
        let sphere_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("spheres"),
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let instance_attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Float32x3
        ];
        let sphere_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("spheres"),
            layout: Some(&sphere_layout),
            vertex: wgpu::VertexState {
                module: &spheres,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &instance_attributes,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                // Reversed-Z: nearer is larger
                depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                mask: !0,
                alpha_to_coverage_enabled: true,
            },
            fragment: Some(wgpu::FragmentState {
                module: &spheres,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blit = device.create_shader_module(wgpu::include_wgsl!("blit.wgsl"));
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &blit,
                // egui draws into a plain (not sRGB) target and encodes colors itself
                entry_point: Some(if target_format.is_srgb() {
                    "fs_linear"
                } else {
                    "fs_encode_srgb"
                }),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit"),
            ..Default::default()
        });

        let instance_capacity = 256;
        SceneRenderer {
            sphere_pipeline,
            blit_pipeline,
            uniform_buffer,
            uniform_bind_group,
            instance_buffer: Self::instance_buffer(device, instance_capacity),
            instance_capacity,
            blit_layout,
            sampler,
            targets: None,
            pending_capture: None,
        }
    }

    fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn targets(&mut self, device: &wgpu::Device, size: [u32; 2]) -> &Targets {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let create = |label, format, samples, usage| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
            let view = |texture: wgpu::Texture| texture.create_view(&Default::default());
            let color = view(create("scene color", COLOR_FORMAT, SAMPLES, attachment));
            let depth = view(create("scene depth", DEPTH_FORMAT, SAMPLES, attachment));
            let resolved_texture = create(
                "scene resolved",
                COLOR_FORMAT,
                1,
                attachment | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            );
            let resolved = resolved_texture.create_view(&Default::default());
            let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit"),
                layout: &self.blit_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&resolved),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.targets = Some(Targets {
                size,
                color,
                depth,
                resolved_texture,
                resolved,
                blit_bind_group,
            });
        }
        self.targets.as_ref().unwrap()
    }

    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        instances: &[Instance],
        uniforms: &Uniforms,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = Self::instance_buffer(device, self.instance_capacity);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        self.targets(device, size);
        let targets = self.targets.as_ref().unwrap();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.color,
                depth_slice: None,
                resolve_target: Some(&targets.resolved),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        if !instances.is_empty() {
            pass.set_pipeline(&self.sphere_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..6, 0..instances.len() as u32);
        }
    }

    /// Records a copy of the last rendered frame into a buffer that can be read back
    fn copy_out(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        capture: &Capture,
    ) -> PendingCapture {
        let targets = self.targets.as_ref().expect("Nothing rendered yet");
        let [width, height] = targets.size;
        let bytes_per_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            targets.resolved_texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        PendingCapture {
            buffer,
            size: targets.size,
            bytes_per_row,
            capture: capture.clone(),
        }
    }

    fn blit(&self, render_pass: &mut wgpu::RenderPass<'static>) {
        let Some(targets) = &self.targets else {
            return;
        };
        render_pass.set_pipeline(&self.blit_pipeline);
        render_pass.set_bind_group(0, &targets.blit_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

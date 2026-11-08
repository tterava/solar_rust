//! The 3D view: bodies drawn with wgpu through eframe's renderer. It only reads the simulation's
//! snapshot (through [`scene::build`]) and never changes it.
//!
//! The scene is rendered into our own multisampled color and depth textures during egui's
//! `prepare` step, then copied into egui's frame where the central panel is. egui's own render
//! pass has no depth buffer, and this way the scene's depth format and multisampling are ours.
//!
//! Drawing order: the background stars, the bodies (opaque, writing depth), then the see-through
//! things that don't hide what's behind them: traces and orbits, rings, and the glow of stars.

pub mod curves;
pub mod light;
pub mod orbits;
pub mod rotation;
pub mod scene;
pub mod sky;
pub mod textures;
pub mod traces;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use eframe::egui_wgpu::{self, wgpu, CallbackResources, CallbackTrait, ScreenDescriptor};
use egui::{Painter, Rect};
use glam::Mat4;

use scene::{Glow, Instance, LineVertex, RingInstance, Scene};
use textures::Surface;

const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Textures uploaded per frame at most, so a burst of finished loads doesn't stall a frame
const UPLOADS_PER_FRAME: usize = 2;
/// Light adds up
const ADDITIVE: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Creates the renderer's GPU resources and starts loading the textures of `bodies`. Call once at
/// startup.
pub fn init(render_state: &egui_wgpu::RenderState, bodies: Vec<Arc<str>>) {
    let mut renderer = SceneRenderer::new(
        &render_state.device,
        &render_state.queue,
        render_state.target_format,
    );
    renderer.loader = Some(Mutex::new(textures::spawn_loader(bodies)));
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
        names: scene.names.clone(),
        rings: scene.rings.clone(),
        ring_names: scene.ring_names.clone(),
        glows: scene.glows.clone(),
        lines: scene.lines.clone(),
        projection: scene.projection,
        rotation: scene.rotation,
        rect,
        capture,
    };
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, callback));
}

/// A frame's worth of the scene, handed to the GPU side
struct SceneCallback {
    instances: Vec<Instance>,
    names: Vec<Arc<str>>,
    rings: Vec<RingInstance>,
    ring_names: Vec<Arc<str>>,
    glows: Vec<Glow>,
    lines: Vec<LineVertex>,
    projection: Mat4,
    rotation: Mat4,
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

        // A copy recorded last frame has been submitted by now
        if let Some(pending) = renderer.pending_capture.take() {
            pending.save(device);
        }
        renderer.upload_textures(device, queue);
        renderer.render(device, queue, encoder, size, self);
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

/// A vertex buffer rewritten every frame, grown when it's too small
struct FrameBuffer {
    label: &'static str,
    buffer: wgpu::Buffer,
    capacity: u64,
    /// Elements written this frame
    len: u32,
}

impl FrameBuffer {
    fn new(device: &wgpu::Device, label: &'static str) -> Self {
        let capacity = 4096;
        FrameBuffer {
            label,
            buffer: Self::create(device, label, capacity),
            capacity,
            len: 0,
        }
    }

    fn create(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn write<T: bytemuck::Pod>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[T]) {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        if bytes.len() as u64 > self.capacity {
            self.capacity = (bytes.len() as u64).next_power_of_two();
            self.buffer = Self::create(device, self.label, self.capacity);
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytes);
        }
        self.len = data.len() as u32;
    }
}

/// How a pipeline draws into the scene's targets
struct Drawing<'a> {
    label: &'a str,
    module: &'a wgpu::ShaderModule,
    layout: &'a wgpu::PipelineLayout,
    vertex: wgpu::VertexBufferLayout<'a>,
    topology: wgpu::PrimitiveTopology,
    blend: Option<wgpu::BlendState>,
    /// Opaque things write depth and smooth their outlines through alpha to coverage
    opaque: bool,
    depth_test: bool,
}

fn scene_pipeline(device: &wgpu::Device, drawing: Drawing) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(drawing.label),
        layout: Some(drawing.layout),
        vertex: wgpu::VertexState {
            module: drawing.module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(drawing.vertex)],
        },
        primitive: wgpu::PrimitiveState {
            topology: drawing.topology,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(drawing.opaque),
            // Reversed-Z: nearer is larger
            depth_compare: Some(if drawing.depth_test {
                wgpu::CompareFunction::Greater
            } else {
                wgpu::CompareFunction::Always
            }),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: drawing.opaque,
        },
        fragment: Some(wgpu::FragmentState {
            module: drawing.module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: drawing.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn texture_layout_entries() -> [wgpu::BindGroupLayoutEntry; 2] {
    [
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
    ]
}

struct SceneRenderer {
    sky_pipeline: wgpu::RenderPipeline,
    sphere_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    glow_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    sky: wgpu::Buffer,
    sky_count: u32,
    instances: FrameBuffer,
    rings: FrameBuffer,
    glows: FrameBuffer,
    lines: FrameBuffer,
    /// This frame's instances and rings, with `textured` filled in
    instance_upload: Vec<Instance>,
    ring_upload: Vec<RingInstance>,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture_layout: wgpu::BindGroupLayout,
    surface_sampler: wgpu::Sampler,
    ring_sampler: wgpu::Sampler,
    /// The bind group of every texture that has loaded
    textures: HashMap<Surface, wgpu::BindGroup>,
    /// For what has no texture: one white pixel, which the shaders ignore
    plain: wgpu::BindGroup,
    loader: Option<Mutex<Receiver<textures::Loaded>>>,
    targets: Option<Targets>,
    pending_capture: Option<PendingCapture>,
}

impl SceneRenderer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene uniforms"),
            size: std::mem::size_of::<scene::Uniforms>() as u64,
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

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("surface"),
            entries: &texture_layout_entries(),
        });
        // Longitude wraps around, latitude stops at the poles. Anisotropic filtering keeps the
        // surface sharp towards the limb, where it's seen at a slant.
        let surface_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("surface"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });
        // Rings stop at their edges
        let ring_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rings"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 16,
            ..Default::default()
        });

        let layout = |label, textured: bool| {
            let uniforms = Some(&uniform_layout);
            let groups = if textured {
                vec![uniforms, Some(&texture_layout)]
            } else {
                vec![uniforms]
            };
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &groups,
                immediate_size: 0,
            })
        };
        let plain_layout = layout("scene", false);
        let textured_layout = layout("textured", true);
        let instanced = |stride: usize, attributes| wgpu::VertexBufferLayout {
            array_stride: stride as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes,
        };

        let sky_module = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
        let sky_attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x3];
        let sky_pipeline = scene_pipeline(
            device,
            Drawing {
                label: "sky",
                module: &sky_module,
                layout: &plain_layout,
                vertex: instanced(std::mem::size_of::<sky::SkyStar>(), &sky_attributes),
                topology: wgpu::PrimitiveTopology::TriangleList,
                blend: Some(ADDITIVE),
                opaque: false,
                depth_test: false,
            },
        );

        let spheres = device.create_shader_module(wgpu::include_wgsl!("spheres.wgsl"));
        let sphere_attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Float32x3,
            5 => Float32, 6 => Float32x3, 7 => Float32, 8 => Float32x3, 9 => Float32x3,
            10 => Float32x3, 11 => Float32x3, 12 => Float32x3
        ];
        let sphere_pipeline = scene_pipeline(
            device,
            Drawing {
                label: "spheres",
                module: &spheres,
                layout: &textured_layout,
                vertex: instanced(std::mem::size_of::<Instance>(), &sphere_attributes),
                topology: wgpu::PrimitiveTopology::TriangleList,
                blend: None,
                opaque: true,
                depth_test: true,
            },
        );

        let line_module = device.create_shader_module(wgpu::include_wgsl!("lines.wgsl"));
        let line_attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let line_pipeline = scene_pipeline(
            device,
            Drawing {
                label: "lines",
                module: &line_module,
                layout: &plain_layout,
                vertex: wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &line_attributes,
                },
                topology: wgpu::PrimitiveTopology::LineList,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                opaque: false,
                depth_test: true,
            },
        );

        let ring_module = device.create_shader_module(wgpu::include_wgsl!("rings.wgsl"));
        let ring_attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Float32x3,
            5 => Float32, 6 => Float32x3, 7 => Float32, 8 => Float32x3, 9 => Float32x3,
            10 => Float32x3
        ];
        let ring_pipeline = scene_pipeline(
            device,
            Drawing {
                label: "rings",
                module: &ring_module,
                layout: &textured_layout,
                vertex: instanced(std::mem::size_of::<RingInstance>(), &ring_attributes),
                topology: wgpu::PrimitiveTopology::TriangleList,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                opaque: false,
                depth_test: true,
            },
        );

        let glow_module = device.create_shader_module(wgpu::include_wgsl!("glow.wgsl"));
        let glow_attributes =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x3];
        let glow_pipeline = scene_pipeline(
            device,
            Drawing {
                label: "glow",
                module: &glow_module,
                layout: &plain_layout,
                vertex: instanced(std::mem::size_of::<Glow>(), &glow_attributes),
                topology: wgpu::PrimitiveTopology::TriangleList,
                blend: Some(ADDITIVE),
                opaque: false,
                depth_test: true,
            },
        );

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &texture_layout_entries(),
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

        let stars = sky::stars();
        let sky = FrameBuffer::create(
            device,
            "sky",
            std::mem::size_of_val(stars.as_slice()) as u64,
        );
        queue.write_buffer(&sky, 0, bytemuck::cast_slice(&stars));

        let white = textures::Image {
            width: 1,
            height: 1,
            pixels: vec![255; 4],
        };
        let (plain_texture, plain) = Self::create_texture(
            device,
            &texture_layout,
            &surface_sampler,
            "plain",
            std::slice::from_ref(&white),
        );
        write_mips(queue, &plain_texture, std::slice::from_ref(&white));

        SceneRenderer {
            sky_pipeline,
            sphere_pipeline,
            line_pipeline,
            ring_pipeline,
            glow_pipeline,
            blit_pipeline,
            uniform_buffer,
            uniform_bind_group,
            sky,
            sky_count: stars.len() as u32,
            instances: FrameBuffer::new(device, "instances"),
            rings: FrameBuffer::new(device, "rings"),
            glows: FrameBuffer::new(device, "glows"),
            lines: FrameBuffer::new(device, "lines"),
            instance_upload: Vec::new(),
            ring_upload: Vec::new(),
            blit_layout,
            sampler,
            texture_layout,
            surface_sampler,
            ring_sampler,
            textures: HashMap::new(),
            plain,
            loader: None,
            targets: None,
            pending_capture: None,
        }
    }

    /// Creates a texture from its mip levels, and the bind group to draw with it
    fn create_texture(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        label: &str,
        mips: &[textures::Image],
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: mips[0].width,
                height: mips[0].height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        (texture, bind_group)
    }

    /// Uploads textures the loader has finished
    fn upload_textures(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(loader) = &mut self.loader else {
            return;
        };
        let loader = loader.get_mut().unwrap();
        for loaded in loader.try_iter().take(UPLOADS_PER_FRAME) {
            let (sampler, label) = match &loaded.surface {
                Surface::Body(name) => (&self.surface_sampler, name),
                Surface::Rings(name) => (&self.ring_sampler, name),
            };
            let (texture, bind_group) =
                Self::create_texture(device, &self.texture_layout, sampler, label, &loaded.mips);
            write_mips(queue, &texture, &loaded.mips);
            self.textures.insert(loaded.surface, bind_group);
            textures::uploaded();
        }
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
        frame: &SceneCallback,
    ) {
        let uniforms = scene::uniforms(frame.projection, frame.rotation, size);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let textures = &self.textures;
        let textured = |surface: Surface| textures.contains_key(&surface) as u32 as f32;
        self.instance_upload.clear();
        self.instance_upload
            .extend(
                frame
                    .instances
                    .iter()
                    .zip(&frame.names)
                    .map(|(instance, name)| Instance {
                        textured: textured(Surface::Body(name.clone())),
                        ..*instance
                    }),
            );
        self.ring_upload.clear();
        self.ring_upload.extend(
            frame
                .rings
                .iter()
                .zip(&frame.ring_names)
                .map(|(ring, name)| RingInstance {
                    textured: textured(Surface::Rings(name.clone())),
                    ..*ring
                }),
        );
        self.instances.write(device, queue, &self.instance_upload);
        self.rings.write(device, queue, &self.ring_upload);
        self.glows.write(device, queue, &frame.glows);
        self.lines.write(device, queue, &frame.lines);

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
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);

        pass.set_pipeline(&self.sky_pipeline);
        pass.set_vertex_buffer(0, self.sky.slice(..));
        pass.draw(0..6, 0..self.sky_count);

        // One draw per body, each with its own texture
        if self.instances.len > 0 {
            pass.set_pipeline(&self.sphere_pipeline);
            pass.set_vertex_buffer(0, self.instances.buffer.slice(..));
            for (i, name) in frame.names.iter().enumerate() {
                let surface = self.textures.get(&Surface::Body(name.clone()));
                pass.set_bind_group(1, surface.unwrap_or(&self.plain), &[]);
                pass.draw(0..6, i as u32..i as u32 + 1);
            }
        }

        if self.lines.len > 0 {
            pass.set_pipeline(&self.line_pipeline);
            pass.set_vertex_buffer(0, self.lines.buffer.slice(..));
            pass.draw(0..self.lines.len, 0..1);
        }

        if self.rings.len > 0 {
            pass.set_pipeline(&self.ring_pipeline);
            pass.set_vertex_buffer(0, self.rings.buffer.slice(..));
            for (i, name) in frame.ring_names.iter().enumerate() {
                let strip = self.textures.get(&Surface::Rings(name.clone()));
                pass.set_bind_group(1, strip.unwrap_or(&self.plain), &[]);
                pass.draw(0..6, i as u32..i as u32 + 1);
            }
        }

        if self.glows.len > 0 {
            pass.set_pipeline(&self.glow_pipeline);
            pass.set_vertex_buffer(0, self.glows.buffer.slice(..));
            pass.draw(0..6, 0..self.glows.len);
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

/// Writes every mip level of a texture
fn write_mips(queue: &wgpu::Queue, texture: &wgpu::Texture, mips: &[textures::Image]) {
    for (level, mip) in mips.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &mip.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip.width * 4),
                rows_per_image: Some(mip.height),
            },
            wgpu::Extent3d {
                width: mip.width,
                height: mip.height,
                depth_or_array_layers: 1,
            },
        );
    }
}

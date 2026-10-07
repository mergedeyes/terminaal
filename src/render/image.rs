//! Images in the terminal (`terminal::graphics`): one textured quad per
//! placement on screen, clipped to its pane, drawn between the cell
//! backgrounds and the text.
//!
//! Textures are uploaded the first time an image is drawn and kept while
//! it keeps being drawn; one off screen for [`KEEP_FRAMES`] frames goes,
//! and comes back from the terminal's copy when it's scrolled into view
//! again.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Frames a texture stays without being drawn.
const KEEP_FRAMES: u64 = 300;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UnitVertex {
    pos: [f32; 2],
}

const UNIT_QUAD: [UnitVertex; 6] = [
    UnitVertex { pos: [0.0, 0.0] },
    UnitVertex { pos: [1.0, 0.0] },
    UnitVertex { pos: [0.0, 1.0] },
    UnitVertex { pos: [0.0, 1.0] },
    UnitVertex { pos: [1.0, 0.0] },
    UnitVertex { pos: [1.0, 1.0] },
];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    offset: [f32; 2],
    size: [f32; 2],
    uv_offset: [f32; 2],
    uv_size: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
    _pad: [f32; 2],
}

/// Which image: the pane, its id there, the transmission.
pub type TextureKey = (usize, u32, u64);

/// One image to draw this frame.
#[derive(Clone)]
pub struct ImageDraw {
    pub key: TextureKey,
    /// Width, height and straight RGBA, in case it's not uploaded yet.
    pub pixels: (u32, u32, Arc<Vec<u8>>),
    /// Where, in physical pixels: x, y, width, height.
    pub rect: [f32; 4],
    /// The part of the image, in texture coordinates: x, y, width, height.
    pub uv: [f32; 4],
    /// The pane's rectangle in physical pixels, x, y, width, height.
    pub clip: [u32; 4],
}

struct Cached {
    bind_group: wgpu::BindGroup,
    _texture: wgpu::Texture,
    last_used: u64,
}

pub struct ImageRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    screen_uniform_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    cache: HashMap<TextureKey, Cached>,
    frame: u64,
    /// This frame's draws: texture and clip rectangle, in instance order.
    prepared: Vec<(TextureKey, [u32; 4])>,
    screen: (u32, u32),
}

impl ImageRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("image.wgsl").into()),
        });
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("image_unit_vertices"),
            contents: bytemuck::cast_slice(&UNIT_QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instance_capacity = 16;
        let instance_buffer = instance_buffer(device, instance_capacity);
        let screen_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("image_screen_uniform"),
            contents: bytemuck::cast_slice(&[ScreenUniform { size: [0.0, 0.0], _pad: [0.0, 0.0] }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image_screen_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image_screen_bind_group"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: screen_uniform_buffer.as_entire_binding() }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image_texture_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
        // Scaled images are smoothed, not blocky.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image_pipeline_layout"),
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<UnitVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![1 => Float32x2, 2 => Float32x2, 3 => Float32x2, 4 => Float32x2],
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            vertex_buffer,
            instance_buffer,
            instance_capacity,
            screen_uniform_buffer,
            screen_bind_group,
            texture_layout,
            sampler,
            cache: HashMap::new(),
            frame: 0,
            prepared: Vec::new(),
            screen: (0, 0),
        }
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: f32, height: f32) {
        self.screen = (width as u32, height as u32);
        queue.write_buffer(
            &self.screen_uniform_buffer,
            0,
            bytemuck::cast_slice(&[ScreenUniform { size: [width, height], _pad: [0.0, 0.0] }]),
        );
    }

    /// Upload what this frame draws: new textures, and the quads.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, draws: &[ImageDraw]) {
        self.frame += 1;
        for draw in draws {
            let frame = self.frame;
            if let Some(cached) = self.cache.get_mut(&draw.key) {
                cached.last_used = frame;
                continue;
            }
            let (bind_group, texture) = self.upload(device, queue, &draw.pixels);
            self.cache.insert(draw.key, Cached { bind_group, _texture: texture, last_used: frame });
        }
        let frame = self.frame;
        self.cache.retain(|_, cached| cached.last_used + KEEP_FRAMES >= frame);

        if draws.len() > self.instance_capacity {
            self.instance_capacity = draws.len().next_power_of_two();
            self.instance_buffer = instance_buffer(device, self.instance_capacity);
        }
        let instances: Vec<Instance> = draws
            .iter()
            .map(|draw| Instance {
                offset: [draw.rect[0], draw.rect[1]],
                size: [draw.rect[2], draw.rect[3]],
                uv_offset: [draw.uv[0], draw.uv[1]],
                uv_size: [draw.uv[2], draw.uv[3]],
            })
            .collect();
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }
        self.prepared = draws.iter().map(|draw| (draw.key, draw.clip)).collect();
    }

    fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, (width, height, rgba): &(u32, u32, Arc<Vec<u8>>)) -> (wgpu::BindGroup, wgpu::Texture) {
        let size = wgpu::Extent3d { width: *width, height: *height, depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terminal_image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: None },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terminal_image_bind_group"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        (bind_group, texture)
    }

    /// Draw the prepared images in `range`, each clipped to its pane.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.screen_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        for index in range {
            let Some((key, [x, y, w, h])) = self.prepared.get(index) else { break };
            let Some(cached) = self.cache.get(key) else { continue };
            // The scissor must stay on the surface.
            let (x, y) = (*x.min(&self.screen.0), *y.min(&self.screen.1));
            let (w, h) = ((*w).min(self.screen.0 - x), (*h).min(self.screen.1 - y));
            if w == 0 || h == 0 {
                continue;
            }
            pass.set_scissor_rect(x, y, w, h);
            pass.set_bind_group(1, &cached.bind_group, &[]);
            pass.draw(0..6, index as u32..index as u32 + 1);
        }
        pass.set_scissor_rect(0, 0, self.screen.0.max(1), self.screen.1.max(1));
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("image_instances"),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

//! Backdrop blur for the floating glass (composer field and its cards, the
//! destinations bar) — Linux's counterpart of the Android RenderEffect blur
//! and the macOS material.
//!
//! iced has no blur, and a custom primitive normally can't read what was
//! drawn before it. Our vendored `iced_wgpu` (third_party/iced_wgpu,
//! CURSUS_PATCH.md) makes the window surface sampleable when the GPU allows
//! it. A [`backdrop`] then sits *under* a glass surface (`Stack::push_under`,
//! so the surface's tint and contents are drawn on a later layer, over it):
//! its primitive reads the frame drawn so far around its bounds, blurs it
//! on the GPU (downsample + two separable Gaussian passes, twice) and draws
//! the result back clipped to the surface's rounded rectangle.
//!
//! With the software renderer, or a surface that can't be sampled, nothing
//! is drawn and the glass keeps its dense frosted fill ([`available`]).

use std::sync::atomic::Ordering;
use std::sync::Mutex;

use iced::widget::shader::{self, Viewport};
use iced::{mouse, Element, Length, Rectangle};
use iced_wgpu::wgpu;

use crate::Message;

/// Downsampling of the blurred copy: cheaper, and widens the blur.
const DOWNSAMPLE: u32 = 4;
/// Backdrop read around the surface (logical px) so its edges blur in what
/// lies just outside it. Small: the same layer may already hold siblings
/// drawn beside the surface (the send button 8 px from the field), which
/// must not bleed into it.
const MARGIN: f32 = 6.0;
/// Blur passes after the downsampling (horizontal) one: V, H, V.
const PASSES: usize = 3;

/// `true` when the frame can be sampled, so glass blurs its backdrop.
pub fn available() -> bool {
    iced_wgpu::BACKDROP_READABLE.load(Ordering::Relaxed)
}

/// The blurred backdrop under a glass surface of corner `radius`. Use with
/// `Stack::push_under`; an empty space when blur isn't [`available`].
pub fn backdrop<'a>(radius: f32) -> Element<'a, Message> {
    if available() {
        iced::widget::shader(Glass { radius })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        iced::widget::Space::new().width(Length::Fill).height(Length::Fill).into()
    }
}

/// `content` as a glass surface of corner `radius`: the blurred backdrop
/// under it. The tree shape is the same with or without blur, so focus in
/// `content` (the composer editor) survives the renderer settling.
pub fn surface<'a>(content: impl Into<Element<'a, Message>>, radius: f32) -> Element<'a, Message> {
    let content: Element<'a, Message> = content.into();
    // Same sizing as the surface, so rows / columns lay it out unchanged.
    let size = content.as_widget().size();
    iced::widget::Stack::new()
        .width(size.width)
        .height(size.height)
        .push(content)
        .push_under(backdrop(radius))
        .into()
}

struct Glass {
    radius: f32,
}

impl shader::Program<Message> for Glass {
    type State = ();
    type Primitive = Primitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> Primitive {
        Primitive { radius: self.radius, slot: Mutex::new(None) }
    }
}

#[derive(Debug)]
pub struct Primitive {
    radius: f32,
    /// Which pipeline slot `prepare` filled for this frame.
    slot: Mutex<Option<usize>>,
}

/// Physical-pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Px {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    /// Source rectangle in the input texture, as UV x, y, w, h.
    rect: [f32; 4],
    /// Blur step in input UV (x, y); unused for the composite.
    step: [f32; 4],
    /// Composite: surface size (px) and corner radius (px).
    shape: [f32; 4],
    _pad: [f32; 4],
}

impl Uniforms {
    fn bytes(&self) -> &[u8] {
        // SAFETY: plain `#[repr(C)]` f32 arrays, no padding bytes.
        unsafe { std::slice::from_raw_parts((self as *const Self).cast::<u8>(), std::mem::size_of::<Self>()) }
    }
}

/// GPU resources for one glass surface, reused while its size holds.
struct Slot {
    size: (u32, u32),
    /// Ping-pong targets of the blur, downsampled.
    views: [wgpu::TextureView; 2],
    /// One uniform buffer per pass: downsample, PASSES blurs, composite.
    uniforms: Vec<wgpu::Buffer>,
    /// Bind groups reading views[0] / views[1] with each pass's uniforms.
    reads: Vec<[wgpu::BindGroup; 2]>,
    /// Surface and the backdrop area read around it, this frame.
    surface: Px,
    area: Px,
    frame: (u32, u32),
}

pub struct Pipeline {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    slots: Vec<Slot>,
    next: usize,
}

const SHADER: &str = r#"
struct U { rect: vec4<f32>, step: vec4<f32>, shape: vec4<f32>, pad: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var t: texture_2d<f32>;
@group(0) @binding(2) var s: sampler;

struct VO { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VO {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VO;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

@fragment
fn blur(in: VO) -> @location(0) vec4<f32> {
    var w = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    let uv = u.rect.xy + in.uv * u.rect.zw;
    var c = textureSample(t, s, uv) * w[0];
    for (var k = 1; k < 5; k++) {
        let o = u.step.xy * f32(k);
        c += (textureSample(t, s, uv + o) + textureSample(t, s, uv - o)) * w[k];
    }
    return vec4<f32>(c.rgb, 1.0);
}

@fragment
fn composite(in: VO) -> @location(0) vec4<f32> {
    let c = textureSample(t, s, u.rect.xy + in.uv * u.rect.zw);
    let size = u.shape.xy;
    let r = min(u.shape.z, min(size.x, size.y) * 0.5);
    let p = in.uv * size - size * 0.5;
    let q = abs(p) - (size * 0.5 - vec2<f32>(r, r));
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
    return vec4<f32>(c.rgb, clamp(0.5 - d, 0.0, 1.0));
}
"#;

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cursus glass"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cursus glass"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cursus glass"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let make = |entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("cursus glass"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let blur = make("blur", None);
        let composite = make("composite", Some(wgpu::BlendState::ALPHA_BLENDING));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cursus glass"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { device: device.clone(), format, layout, sampler, blur, composite, slots: Vec::new(), next: 0 }
    }

    fn trim(&mut self) {
        // End of frame: the next frame's surfaces reuse slots from 0.
        self.next = 0;
    }
}

impl Pipeline {
    fn bind(&self, buffer: &wgpu::Buffer, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cursus glass"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        })
    }

    fn slot(&mut self, index: usize, size: (u32, u32)) -> &mut Slot {
        let fits = self.slots.get(index).is_some_and(|s| s.size == size);
        if !fits {
            let texture = |device: &wgpu::Device| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("cursus glass"),
                        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default())
            };
            let views = [texture(&self.device), texture(&self.device)];
            let uniforms: Vec<wgpu::Buffer> = (0..PASSES + 2)
                .map(|_| {
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("cursus glass"),
                        size: std::mem::size_of::<Uniforms>() as u64,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                })
                .collect();
            let reads = uniforms.iter().map(|b| [self.bind(b, &views[0]), self.bind(b, &views[1])]).collect();
            let slot = Slot {
                size,
                views,
                uniforms,
                reads,
                surface: Px { x: 0, y: 0, w: 0, h: 0 },
                area: Px { x: 0, y: 0, w: 0, h: 0 },
                frame: (1, 1),
            };
            if index < self.slots.len() {
                self.slots[index] = slot;
            } else {
                self.slots.push(slot);
            }
        }
        &mut self.slots[index]
    }
}

impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Pipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale = viewport.scale_factor() as f32;
        let frame = viewport.physical_size();
        let (fw, fh) = (frame.width.max(1), frame.height.max(1));
        let clamp_rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
            let x0 = x0.floor().clamp(0.0, fw as f32) as u32;
            let y0 = y0.floor().clamp(0.0, fh as f32) as u32;
            let x1 = x1.ceil().clamp(0.0, fw as f32) as u32;
            let y1 = y1.ceil().clamp(0.0, fh as f32) as u32;
            Px { x: x0, y: y0, w: x1.saturating_sub(x0), h: y1.saturating_sub(y0) }
        };
        let b = *bounds * scale;
        let surface = clamp_rect(b.x, b.y, b.x + b.width, b.y + b.height);
        let m = MARGIN * scale;
        let area = clamp_rect(b.x - m, b.y - m, b.x + b.width + m, b.y + b.height + m);
        if surface.w == 0 || surface.h == 0 || area.w == 0 || area.h == 0 {
            *self.slot.lock().unwrap() = None;
            return;
        }
        let size = (area.w.div_ceil(DOWNSAMPLE).max(1), area.h.div_ceil(DOWNSAMPLE).max(1));
        let index = pipeline.next;
        pipeline.next += 1;
        let slot = pipeline.slot(index, size);
        slot.surface = surface;
        slot.area = area;
        slot.frame = (fw, fh);

        let (tw, th) = (size.0 as f32, size.1 as f32);
        let full = [0.0, 0.0, 1.0, 1.0];
        let mut passes = Vec::with_capacity(PASSES + 2);
        // Downsample + horizontal blur straight out of the frame.
        passes.push(Uniforms {
            rect: [area.x as f32 / fw as f32, area.y as f32 / fh as f32, area.w as f32 / fw as f32, area.h as f32 / fh as f32],
            step: [DOWNSAMPLE as f32 / fw as f32, 0.0, 0.0, 0.0],
            shape: [0.0; 4],
            _pad: [0.0; 4],
        });
        for i in 0..PASSES {
            let step = if i % 2 == 0 { [0.0, 1.0 / th, 0.0, 0.0] } else { [1.0 / tw, 0.0, 0.0, 0.0] };
            passes.push(Uniforms { rect: full, step, shape: [0.0; 4], _pad: [0.0; 4] });
        }
        // Composite: the surface's part of the blurred area, rounded.
        passes.push(Uniforms {
            rect: [
                (surface.x - area.x) as f32 / area.w as f32,
                (surface.y - area.y) as f32 / area.h as f32,
                surface.w as f32 / area.w as f32,
                surface.h as f32 / area.h as f32,
            ],
            step: [0.0; 4],
            shape: [surface.w as f32, surface.h as f32, self.radius * scale, 0.0],
            _pad: [0.0; 4],
        });
        for (buffer, u) in slot.uniforms.iter().zip(&passes) {
            queue.write_buffer(buffer, 0, u.bytes());
        }
        *self.slot.lock().unwrap() = Some(index);
    }

    fn render(
        &self,
        pipeline: &Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(index) = *self.slot.lock().unwrap() else { return };
        let Some(slot) = pipeline.slots.get(index) else { return };

        let pass = |encoder: &mut wgpu::CommandEncoder,
                    view: &wgpu::TextureView,
                    pipe: &wgpu::RenderPipeline,
                    group: &wgpu::BindGroup,
                    load: wgpu::LoadOp<wgpu::Color>|
         -> wgpu::RenderPass<'static> {
            let mut rp = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cursus glass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            rp.set_pipeline(pipe);
            rp.set_bind_group(0, group, &[]);
            rp
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);

        // 1. Frame → views[0]: downsample + horizontal blur.
        let from_frame = pipeline.bind(&slot.uniforms[0], target);
        pass(encoder, &slot.views[0], &pipeline.blur, &from_frame, clear).draw(0..3, 0..1);
        // 2. Ping-pong: V, H, V, … between views[0] and views[1].
        let mut src = 0;
        for i in 0..PASSES {
            let dst = 1 - src;
            pass(encoder, &slot.views[dst], &pipeline.blur, &slot.reads[i + 1][src], clear).draw(0..3, 0..1);
            src = dst;
        }
        // 3. Blurred area → the surface on the frame, rounded.
        let s = slot.surface;
        let (x0, y0) = (s.x.max(clip_bounds.x), s.y.max(clip_bounds.y));
        let x1 = (s.x + s.w).min(clip_bounds.x + clip_bounds.width).min(slot.frame.0);
        let y1 = (s.y + s.h).min(clip_bounds.y + clip_bounds.height).min(slot.frame.1);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let mut rp = pass(encoder, target, &pipeline.composite, &slot.reads[PASSES + 1][src], wgpu::LoadOp::Load);
        rp.set_viewport(s.x as f32, s.y as f32, s.w as f32, s.h as f32, 0.0, 1.0);
        rp.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
        rp.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::executor::block_on;
    use iced::widget::shader::{Pipeline as _, Primitive as _};
    use iced::Size;

    /// Runs the real passes on a headless GPU: a striped frame must come
    /// out smoothly grey inside the surface, untouched outside it and in
    /// its rounded corners. Skips when the machine has no GPU adapter.
    #[test]
    fn blurs_only_inside_the_rounded_surface() {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("no GPU adapter: skipped");
            return;
        };
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
        device.push_error_scope(wgpu::ErrorFilter::Validation);

        let (w, h) = (256u32, 128u32);
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // 4 px black / white stripes.
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = if x % 8 < 4 { 255 } else { 0 };
                pixels.extend_from_slice(&[v, v, v, 255]);
            }
        }
        queue.write_texture(
            texture.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut pipeline = Pipeline::new(&device, &queue, format);
        let primitive = Primitive { radius: 16.0, slot: Mutex::new(None) };
        let bounds = Rectangle { x: 64.0, y: 32.0, width: 128.0, height: 64.0 };
        primitive.prepare(&mut pipeline, &device, &queue, &bounds, &Viewport::with_physical_size(Size::new(w, h), 1.0));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        primitive.render(&pipeline, &mut encoder, &view, &Rectangle { x: 0, y: 0, width: w, height: h });

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map"));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        assert!(block_on(device.pop_error_scope()).is_none(), "wgpu validation error");

        let data = buffer.slice(..).get_mapped_range();
        let at = |x: u32, y: u32| data[((y * w + x) * 4) as usize];
        // Inside: stripes blurred to grey, and smooth from pixel to pixel.
        for x in 100..140 {
            let v = at(x, 64);
            assert!((70..=190).contains(&v), "({x},64) = {v}: not blurred");
            assert!(v.abs_diff(at(x + 1, 64)) < 40, "({x},64): still striped");
        }
        // Outside the surface: untouched.
        assert!(matches!(at(20, 64), 0 | 255) && matches!(at(128, 10), 0 | 255));
        // The rounded corner of the surface's box: untouched.
        assert!(matches!(at(65, 33), 0 | 255), "corner was drawn: {}", at(65, 33));
    }
}

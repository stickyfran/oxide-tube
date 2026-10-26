use crate::player::VideoFrameBuffer;
use iced::widget::shader::{self, Viewport};
use iced::{mouse, Element, Length, Rectangle};
use iced_wgpu::wgpu;

pub struct VideoPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    current_size: (u32, u32),
    uploaded_frame_id: u64,
    win_w: f32,
    win_h: f32,
}

impl VideoPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mitub_video_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("video_shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mitub_video_bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mitub_video_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mitub_video_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mitub_video_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mitub_video_uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            uniform_buffer,
            texture: None,
            bind_group: None,
            current_size: (0, 0),
            uploaded_frame_id: 0,
            win_w: 1.0,
            win_h: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VideoPrimitive {
    pub frame: VideoFrameBuffer,
}

impl shader::Primitive for VideoPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        if !storage.has::<VideoPipeline>() {
            storage.store(VideoPipeline::new(device, format));
        }

        let video_pipe = storage.get_mut::<VideoPipeline>().unwrap();

        if self.frame.width == 0 || self.frame.height == 0 {
            return;
        }

        let win_w = (viewport.physical_width() as f32).max(1.0);
        let win_h = (viewport.physical_height() as f32).max(1.0);
        let scale = viewport.scale_factor() as f32;
        video_pipe.win_w = win_w;
        video_pipe.win_h = win_h;

        // 1. Recreate texture and bind group ONLY if resolution changes
        if video_pipe.current_size != (self.frame.width, self.frame.height) || video_pipe.texture.is_none() {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("mitub_persistent_video_tex"),
                size: wgpu::Extent3d {
                    width: self.frame.width,
                    height: self.frame.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mitub_video_bg"),
                layout: &video_pipe.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&video_pipe.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: video_pipe.uniform_buffer.as_entire_binding(),
                    },
                ],
            });

            video_pipe.texture = Some(tex);
            video_pipe.bind_group = Some(bg);
            video_pipe.current_size = (self.frame.width, self.frame.height);
            video_pipe.uploaded_frame_id = 0;
        }

        // 2. Upload frame data to the persistent texture ONLY if it's a new frame
        if self.frame.frame_id != video_pipe.uploaded_frame_id {
            if let Some(ref tex) = video_pipe.texture {
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &self.frame.data,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(self.frame.width * 4),
                        rows_per_image: Some(self.frame.height),
                    },
                    wgpu::Extent3d {
                        width: self.frame.width,
                        height: self.frame.height,
                        depth_or_array_layers: 1,
                    },
                );
                video_pipe.uploaded_frame_id = self.frame.frame_id;
            }
        }

        // 3. Compute aspect-ratio preserving bounds in physical pixels based on the widget's on-screen bounds
        let phys_w = (bounds.width * scale).max(1.0);
        let phys_h = (bounds.height * scale).max(1.0);
        let phys_x = bounds.x * scale;
        let phys_y = bounds.y * scale;

        let video_aspect = self.frame.width as f32 / self.frame.height as f32;
        let bounds_aspect = phys_w / phys_h;

        let (vid_w, vid_h) = if bounds_aspect > video_aspect {
            (phys_h * video_aspect, phys_h)
        } else {
            (phys_w, phys_w / video_aspect)
        };

        let vid_x = phys_x + (phys_w - vid_w) / 2.0;
        let vid_y = phys_y + (phys_h - vid_h) / 2.0;

        // Convert physical pixel coordinates to NDC coordinates [-1.0, 1.0]
        let left = (vid_x / win_w) * 2.0 - 1.0;
        let top = 1.0 - (vid_y / win_h) * 2.0;
        let right = ((vid_x + vid_w) / win_w) * 2.0 - 1.0;
        let bottom = 1.0 - ((vid_y + vid_h) / win_h) * 2.0;

        let ndc_rect = [left, top, right, bottom];
        let mut raw = [0u8; 16];
        for (i, &val) in ndc_rect.iter().enumerate() {
            raw[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
        }
        queue.write_buffer(&video_pipe.uniform_buffer, 0, &raw);
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        if self.frame.width == 0 || self.frame.height == 0 || clip_bounds.width == 0 || clip_bounds.height == 0 {
            return;
        }

        if let Some(video_pipe) = storage.get::<VideoPipeline>() {
            if let Some(ref bg) = video_pipe.bind_group {
                if video_pipe.win_w <= 0.0 || video_pipe.win_h <= 0.0 {
                    return;
                }

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mitub_video_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_viewport(
                    0.0,
                    0.0,
                    video_pipe.win_w,
                    video_pipe.win_h,
                    0.0,
                    1.0,
                );
                render_pass.set_scissor_rect(
                    clip_bounds.x,
                    clip_bounds.y,
                    clip_bounds.width,
                    clip_bounds.height,
                );
                render_pass.set_pipeline(&video_pipe.pipeline);
                render_pass.set_bind_group(0, bg, &[]);
                render_pass.draw(0..6, 0..1);
            }
        }
    }
}

pub struct VideoProgram {
    frame: VideoFrameBuffer,
}

impl<Message> shader::Program<Message> for VideoProgram {
    type State = ();
    type Primitive = VideoPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        VideoPrimitive {
            frame: self.frame.clone(),
        }
    }
}

pub fn video_shader<'a, Message: 'static>(frame: VideoFrameBuffer) -> Element<'a, Message> {
    iced::widget::shader(VideoProgram { frame })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}


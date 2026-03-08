use std::sync::Arc;

use egui_wgpu::preferred_framebuffer_format;
use glam::{Vec3, Vec3Swizzles};
use image::GenericImageView;
use wgpu::{self, InstanceFlags, TextureFormat, util::DeviceExt};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::{camera::Camera, gui::Gui};

#[derive(Debug, Clone, Copy)]
pub struct RendererConfig {
    vsync: bool,
    validate: bool,
    validate_gpu: bool,
    surface_format: wgpu::TextureFormat,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            vsync: true,
            validate: true,
            validate_gpu: false,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
        }
    }
}

pub struct Renderer {
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,

    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,

    pub gui: Gui,

    pub pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    diffuse_bind_group: wgpu::BindGroup,
    pub camera: Camera,
    camera_bind_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
    framebuffer_color: wgpu::Texture,
    framebuffer_depth: wgpu::Texture,
    ldr_texture: wgpu::Texture,
    tone_mapping: wgpu::RenderPipeline,
    sampler_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: glam::Vec3,
    uv: glam::Vec2,
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: glam::Vec3::new(-0.5, 0.5, 0.0),
        uv: glam::Vec2::new(0.0, 1.0),
    },
    Vertex {
        position: glam::Vec3::new(-0.5, -0.5, 0.0),
        uv: glam::Vec2::new(0.0, 0.0),
    },
    Vertex {
        position: glam::Vec3::new(0.5, -0.5, 0.0),
        uv: glam::Vec2::new(1.0, 0.0),
    },
    Vertex {
        position: glam::Vec3::new(-0.5, 0.5, 0.0),
        uv: glam::Vec2::new(0.0, 1.0),
    },
    Vertex {
        position: glam::Vec3::new(0.5, -0.5, 0.0),
        uv: glam::Vec2::new(1.0, 0.0),
    },
    Vertex {
        position: glam::Vec3::new(0.5, 0.5, 0.0),
        uv: glam::Vec2::new(1.0, 1.0),
    },
];

impl Renderer {
    pub async fn new(window: Arc<Window>, config: RendererConfig) -> Self {
        let mut flags = InstanceFlags::empty();
        if config.validate {
            flags |= InstanceFlags::debugging();
            if config.validate_gpu {
                flags |= InstanceFlags::advanced_debugging();
            }
        }

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: flags,
            ..Default::default()
        });

        // wgpu::util::new_instance_with_webgpu_detection(instance_desc)

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to get adapter");

        log::info!("Adapter info:\n{:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("RendererDevice"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::defaults(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("Failed to acquire device");

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);

        let mut surface_format = surface_caps.formats[0];
        if surface_caps.formats.contains(&config.surface_format) {
            surface_format = config.surface_format;
        } else {
            surface_format = surface_caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(surface_caps.formats[0]);
        }

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &surface_config);

        let (
            framebuffer_color,
            framebuffer_color_format,
            framebuffer_depth,
            framebuffer_depth_format,
        ) = Self::create_framebuffer(&device, &surface_config);
        let ldr_size = wgpu::Extent3d {
            width: surface_config.width,
            height: surface_config.height,
            depth_or_array_layers: 1,
        };
        let ldr_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: ldr_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: Some("framebuffer_color"),
            view_formats: &[],
        });

        let gui = Gui::new(
            &device,
            ldr_texture.format(),
            window.clone(),
            device.limits().max_texture_dimension_2d as usize,
        );

        let mut camera = Camera::look_at(Vec3::new(0., 0.1, 10.0), Vec3::ZERO);
        camera.aspect = surface_config.width as f32 / surface_config.height as f32;

        let (view, proj) = camera.compute_view_projection();
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[view, proj]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                label: Some("camera_bind_group_layout"),
            });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("camera_bind_group"),
        });

        //setup samplers
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("LinearSampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,

            ..Default::default()
        });
        let point_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("PointSampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,

            ..Default::default()
        });

        let sampler_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    //Linear
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        // This should match the filterable field of the
                        // corresponding Texture entry above.
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    //Point
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("sampler_bind_layout"),
            });

        let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &sampler_bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&point_sampler),
                },
            ],
            label: Some("sampler_bind_group"),
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                }],
                label: Some("texture_bind_group_layout"),
            });

        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../../assets/shaders/model_lit.wgsl"));
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &sampler_bind_layout,
                    &texture_bind_group_layout,
                    &camera_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PIPI"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: framebuffer_color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: framebuffer_depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multiview: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let diffuse_bytes = include_bytes!("../../assets/textures/texture_01.png");
        let diffuse_image = image::load_from_memory(diffuse_bytes).unwrap();
        let diffuse_rgba = diffuse_image.to_rgba8();

        let dimensions = diffuse_image.dimensions();

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            // All textures are stored as 3D, we represent our 2D texture
            // by setting depth to 1.
            depth_or_array_layers: 1,
        };
        let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("diffuse_texture"),
            view_formats: &[],
        });

        queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::TexelCopyTextureInfo {
                texture: &diffuse_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            &diffuse_rgba,
            // The layout of the texture
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            texture_size,
        );

        let diffuse_texture_view =
            diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
            }],
            label: Some("diffuse_bind_group"),
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../../assets/shaders/tone_mapping.wgsl"
        ));
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&sampler_bind_layout, &texture_bind_group_layout],
                push_constant_ranges: &[],
            });
        let tone_mapping = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tone_mapping"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ldr_texture.format(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: None,
            multiview: None,
            cache: None,
        });

        Self {
            instance,
            device,
            queue,
            surface,
            surface_config,
            gui,
            camera,
            camera_buffer,
            camera_bind_group,
            pipeline,
            vertex_buffer,

            texture_bind_group_layout,

            sampler_bind_group,
            diffuse_bind_group,

            tone_mapping,

            framebuffer_color,
            framebuffer_depth,
            ldr_texture,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == self.surface_config.width
            && new_size.height == self.surface_config.height
        {
            return;
        }
        assert_ne!(new_size.width, 0);
        assert_ne!(new_size.height, 0);

        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);

        let (
            framebuffer_color,
            framebuffer_color_format,
            framebuffer_depth,
            framebuffer_depth_format,
        ) = Self::create_framebuffer(&self.device, &self.surface_config);
        let ldr_size = wgpu::Extent3d {
            width: self.surface_config.width,
            height: self.surface_config.height,
            depth_or_array_layers: 1,
        };
        let ldr_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            size: ldr_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: Some("ldr_texture"),
            view_formats: &[],
        });

        self.framebuffer_color.destroy();
        self.framebuffer_depth.destroy();
        self.ldr_texture.destroy();

        self.framebuffer_color = framebuffer_color;
        self.framebuffer_depth = framebuffer_depth;
        self.ldr_texture = ldr_texture;
    }

    pub fn render(&mut self) {
        let output = match self.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                return;
            }
            Err(e) => {
                log::error!("Surface error: {e}");
                return;
            }
        };

        self.camera.position.z -= 0.001;
        let (view, proj) = self.camera.compute_view_projection();
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[view, proj]));

        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            });

        let color_view = self
            .framebuffer_color
            .create_view(&wgpu::TextureViewDescriptor::default());

        let depth_view = self
            .framebuffer_depth
            .create_view(&wgpu::TextureViewDescriptor::default());

        let ldr_view = self
            .ldr_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let framebuffer_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&color_view),
            }],
            label: Some("diffuse_bind_group"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.sampler_bind_group, &[]);
            render_pass.set_bind_group(1, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(2, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..6, 0..1);
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tone mapping"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &ldr_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.tone_mapping);
            render_pass.set_bind_group(0, &self.sampler_bind_group, &[]);
            render_pass.set_bind_group(1, &framebuffer_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        // encoder.copy_texture_to_texture(
        //     wgpu::TexelCopyTextureInfo {
        //         texture: &self.framebuffer_color,
        //         mip_level: 0,
        //         origin: wgpu::Origin3d::ZERO,
        //         aspect: wgpu::TextureAspect::All,
        //     },
        //     wgpu::TexelCopyTextureInfo {
        //         texture: &self.ldr_texture,
        //         mip_level: 0,
        //         origin: wgpu::Origin3d::ZERO,
        //         aspect: wgpu::TextureAspect::All,
        //     },
        //     wgpu::Extent3d {
        //         width: self.surface_config.width,
        //         height: self.surface_config.height,
        //         depth_or_array_layers: 1,
        //     },
        // );

        {
            self.gui
                .end_frame(&self.device, &self.queue, &mut encoder, &ldr_view);
        }

        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.ldr_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &output.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.surface_config.width,
                height: self.surface_config.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    fn create_framebuffer(
        device: &wgpu::Device,
        surface_config: &wgpu::SurfaceConfiguration,
    ) -> (
        wgpu::Texture,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureFormat,
    ) {
        //framebuffer

        let framebuffer_color_format = wgpu::TextureFormat::Rgba16Float;
        let framebuffer_depth_format = wgpu::TextureFormat::Depth24PlusStencil8;
        let framebuffer_size = wgpu::Extent3d {
            width: surface_config.width,
            height: surface_config.height,
            depth_or_array_layers: 1,
        };
        let framebuffer_color = device.create_texture(&wgpu::TextureDescriptor {
            size: framebuffer_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: framebuffer_color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("framebuffer_color"),
            view_formats: &[],
        });
        let framebuffer_depth = device.create_texture(&wgpu::TextureDescriptor {
            size: framebuffer_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: framebuffer_depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("framebuffer_depth"),
            view_formats: &[],
        });

        //framebuffer end

        (
            framebuffer_color,
            framebuffer_color_format,
            framebuffer_depth,
            framebuffer_depth_format,
        )
    }
}

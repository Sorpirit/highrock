use std::sync::Arc;

use winit::window::{Window, WindowAttributes, WindowId};
use wgpu::{self, InstanceFlags, TextureFormat};

use crate::gui::Gui;

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
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb
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
}

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
            .request_device(
                &wgpu::DeviceDescriptor {
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &surface_config);

        let gui = Gui::new(&device, surface_format, window.clone(), device.limits().max_texture_dimension_2d as usize);

        Self { instance, device, queue, surface, surface_config, gui }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == self.surface_config.width && new_size.height == self.surface_config.height {
            return;
        }
        assert_ne!(new_size.width, 0);
        assert_ne!(new_size.height, 0);
        
        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    pub fn render(&mut self) {
        let output = match self.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => { return; },
            Err(e) => { log::error!("Surface error: {e}"); return; }
        };

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Frame Encoder"),
        });

        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
        }

        {
            self.gui.end_frame(&self.device, &self.queue, &mut encoder, &view);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
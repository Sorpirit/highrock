use std::sync::Arc;

use egui_wgpu::ScreenDescriptor;
use wgpu::Face;

pub struct Gui {
    pub ctx: egui::Context,
    pub state: egui_winit::State,
    pub renderer: egui_wgpu::Renderer,
    pub window: Arc<winit::window::Window>,
    pub screen_size: winit::dpi::PhysicalSize<u32>,
    pub is_drawing_gui: bool,
}

impl Gui {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        window: Arc<winit::window::Window>,
        max_texture_side: usize,
    ) -> Self {
        let ctx = egui::Context::default();

        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(max_texture_side),
        );

        let renderer = egui_wgpu::Renderer::new(
            device,
            surface_format,
            egui_wgpu::RendererOptions::PREDICTABLE
        );

        Self {
            ctx,
            state,
            renderer,
            screen_size: window.inner_size(),
            window,
            is_drawing_gui: false,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
    ) -> bool {
        self.state.on_window_event(&self.window, event).consumed
    }

    pub fn begin_frame(&mut self) {
        self.is_drawing_gui = true;
        let raw_input = self.state.take_egui_input(&self.window);
        self.ctx.begin_pass(raw_input);
    }

    pub fn get_gui_context(&self) -> &egui::Context  {
        assert!(self.is_drawing_gui);
        &self.ctx
    }

    pub fn end_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        self.is_drawing_gui = false;
        let full_output = self.ctx.end_pass();

        self.state
            .handle_platform_output(&self.window, full_output.platform_output);

        let paint_jobs = self
            .ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        self.screen_size = self.window.inner_size();
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.screen_size.width, self.screen_size.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }

        self.renderer
            .update_buffers(device, queue, encoder, &paint_jobs, &screen_descriptor);

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Gui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            }).forget_lifetime();

            self.renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}

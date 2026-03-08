pub mod camera;
pub mod gui;
pub mod renderer;
pub mod texture_loader;

use std::sync::Arc;

use env_logger::Env;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::renderer::{Renderer, RendererConfig};

pub struct WindowConfig {
    titel: String,
    width: u32,
    height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            titel: "Highrock".to_string(),
            width: 1920,
            height: 1080,
        }
    }
}

struct Engine {
    window: Arc<Window>,
    renderer: Renderer,
}

struct EngineRunner {
    wconfig: WindowConfig,
    rconfig: RendererConfig,

    engine: Option<Engine>,
}

fn init_window(event_loop: &ActiveEventLoop, config: &WindowConfig) -> Window {
    let window_attributes = WindowAttributes::default()
        .with_title(config.titel.to_string())
        .with_resizable(false)
        .with_maximized(true)
        // .with_fullscreen(Some(true))
        .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height));

    event_loop
        .create_window(window_attributes)
        .expect("Failed to initialize window")
}

impl EngineRunner {
    fn new() -> Self {
        Self {
            wconfig: Default::default(),
            rconfig: Default::default(),
            engine: None,
        }
    }
}

impl ApplicationHandler for EngineRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.engine.is_some() {
            log::warn!("Resumed is called and engine is already initied?");
        }

        let window = init_window(event_loop, &self.wconfig);
        let window = Arc::new(window);
        let renderer = pollster::block_on(Renderer::new(window.clone(), self.rconfig.clone()));

        self.engine = Some(Engine { window, renderer });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };

        let consumed = engine.renderer.gui.handle_event(&event);

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested.");
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                log::info!("Resized: {}:{}", new_size.width, new_size.height);
                engine.renderer.resize(new_size);
            }

            WindowEvent::KeyboardInput { event, .. } if !consumed => {
                if event.physical_key == KeyCode::Escape && event.state.is_pressed() {
                    event_loop.exit();
                }
            }

            // WindowEvent::RedrawRequested => {
            //     engine.renderer.render();
            //     log::info!("render");
            // }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(engine) = self.engine.as_mut() else {
            log::warn!("About to wait without an engine");
            return;
        };

        engine.renderer.gui.begin_frame();

        egui::Window::new("Props").show(engine.renderer.gui.get_gui_context(), |ui| {
            ui.label("Some props");
        });

        engine.renderer.render();
        // engine.window.request_redraw();
    }
}

pub fn setup() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    log::info!("LOG START");

    let game_loop = EventLoop::new().expect("Failed to create event loop");
    game_loop.set_control_flow(ControlFlow::Poll);

    let mut app = EngineRunner::new();
    game_loop.run_app(&mut app).expect("Event loop crashed");
}

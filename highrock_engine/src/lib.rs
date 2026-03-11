pub mod camera;
pub mod gui;
pub mod renderer;
pub mod texture_loader;

use std::sync::Arc;

use env_logger::Env;
use instant::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::renderer::{Renderer, RendererConfig};

// engine/task.rs
use std::future::Future;

pub fn spawn<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(fut);

    // #[cfg(not(target_arch = "wasm32"))]
    // tokio::task::spawn(fut);
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(fut);
    // std::thread::spawn(|| pollster::block_on(fut));
}

pub struct HighRockEngine {
    window: Arc<Window>,
    renderer: Renderer,

    time: Instant,
    frame_count: u32,
    time_arr: [f32; 64],
    time_arr_cursor: usize,
    time_loop: f32,
}

enum EngineEvent {
    EngineReady(HighRockEngine),
    AssetsRead(String),
    AsyncError(String),
}

struct EngineRunner {
    proxy: EventLoopProxy<EngineEvent>,
    engine: Option<HighRockEngine>,
    scene: Option<Box<dyn UserScene>>,
}

impl HighRockEngine {
    pub async fn new(scene: Box<dyn UserScene>, window: Window) -> Result<Self, String> {
        let window = Arc::new(window);
        let renderer = Renderer::new(window.clone(), RendererConfig::default()).await;
        Ok(Self {
            window,
            renderer,
            time: Instant::now(),
            frame_count: 0,
            time_arr: [0.; 64],
            time_arr_cursor: 0,
            time_loop: 0.,
        })
    }

    fn init(&mut self) {}

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.renderer.resize(size);
    }

    fn update_loop(&mut self) {
        if self.frame_count == 0 {
            self.time = Instant::now();
        } else {
            let round_trip = self.time.elapsed().as_secs_f64() * 1000.0;
            self.time_arr[self.time_arr_cursor] = round_trip as f32;
            self.time_arr_cursor = (self.time_arr_cursor + 1) % 64;
            self.time = Instant::now();
        }
        let loop_time = Instant::now();

        self.renderer.gui.begin_frame();

        egui::Window::new("My window").show(&self.renderer.gui.ctx, |ui| {
            ui.label("Hello world");
            ui.collapsing("📊 Performance", |ui| {
                ui.label(format!(
                    "Update loop: {:.1}({:.1} ms)",
                    1000.0 / self.time_loop,
                    self.time_loop
                ));
                let avg = self.time_arr.iter().sum::<f32>();
                ui.label(format!(
                    "Full loop: {:.1}({:.1} ms)",
                    (64. * 1000.0) / avg,
                    avg / 64.
                ));
                ui.label(format!("Frame: {}", self.frame_count));
            });
        });

        self.renderer.render();
        self.frame_count += 1;
        self.time_loop = loop_time.elapsed().as_secs_f32() * 1000.0;
    }

    fn handle_input(&mut self, event_loop: &ActiveEventLoop, event: &WindowEvent) {
        if self.renderer.gui.handle_event(&event) {
            return;
        }

        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => {
                if key_state.is_pressed() && *code == KeyCode::Escape {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

impl EngineRunner {
    fn new(scene: Box<dyn UserScene>, proxy: EventLoopProxy<EngineEvent>) -> Self {
        Self {
            proxy,
            engine: None,
            scene: Some(scene),
        }
    }

    fn engine_loop(&mut self) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };

        engine.update_loop();
    }
}

impl ApplicationHandler<EngineEvent> for EngineRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.engine.is_some() {
            return;
        }

        let proxy = self.proxy.clone();
        let scene = self.scene.take();
        let window = create_window(event_loop);
        spawn(async move {
            match HighRockEngine::new(scene.unwrap(), window).await {
                Ok(ctx) => proxy.send_event(EngineEvent::EngineReady(ctx)).ok(),
                Err(e) => proxy
                    .send_event(EngineEvent::AsyncError(e.to_string()))
                    .ok(),
            };
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                let Some(engine) = self.engine.as_mut() else {
                    return;
                };
                engine.resize(new_size);
            }

            #[cfg(target_arch = "wasm32")]
            WindowEvent::RedrawRequested => {
                self.engine_loop();
            }

            _ => {
                let Some(engine) = self.engine.as_mut() else {
                    return;
                };
                engine.handle_input(event_loop, &event);
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(engine) = self.engine.as_mut() else {
                return;
            };
            engine.window.request_redraw();
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.engine_loop();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: EngineEvent) {
        match event {
            EngineEvent::EngineReady(engine) => self.engine = Some(engine),
            EngineEvent::AssetsRead(_) => todo!(),
            EngineEvent::AsyncError(_) => todo!(),
        }
    }
}

pub trait UserScene: Send {
    fn setup(&mut self);
}

#[cfg(target_arch = "wasm32")]
pub fn log_setup() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("Failed to connect to console...");
    log::info!("LOG SETUP");
}
#[cfg(not(target_arch = "wasm32"))]
pub fn log_setup() {
    // env_logger::init();
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    log::info!("LOG SETUP");
}

pub fn engine_entry(scene: Box<dyn UserScene>) {
    start_game_loop(scene);
}

#[cfg(target_arch = "wasm32")]
fn start_game_loop(scene: Box<dyn UserScene>) {
    let game_loop = EventLoop::with_user_event()
        .build()
        .expect("Failed to create event loop");
    game_loop.set_control_flow(ControlFlow::Wait);

    let mut app = EngineRunner::new(scene, game_loop.create_proxy());
    use winit::platform::web::EventLoopExtWebSys;
    game_loop.spawn_app(app);
}
#[cfg(not(target_arch = "wasm32"))]
fn start_game_loop(scene: Box<dyn UserScene>) {
    let game_loop = EventLoop::with_user_event()
        .build()
        .expect("Failed to create event loop");
    game_loop.set_control_flow(ControlFlow::Poll);

    let mut app = EngineRunner::new(scene, game_loop.create_proxy());
    game_loop.run_app(&mut app).expect("Event loop crashed");
}

#[cfg(target_arch = "wasm32")]
fn create_window(event_loop: &ActiveEventLoop) -> Window {
    use web_sys::wasm_bindgen::JsCast;
    use web_sys::wasm_bindgen::UnwrapThrowExt;
    use winit::platform::web::WindowAttributesExtWebSys;
    let document = web_sys::window()
        .expect("No window")
        .document()
        .expect("No document");

    const CANVAS_ID: &str = "canvas";
    let window = wgpu::web_sys::window().unwrap_throw();
    let document = window.document().unwrap_throw();
    let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
    let html_canvas_element = canvas.unchecked_into();
    // window_attributes = window_attributes.with_canvas(Some(html_canvas_element));

    // let canvas = document
    //     .get_element_by_id("canvas")
    //     .expect("Failed to find the_canvas_id")
    //     .dyn_into::<web_sys::HtmlCanvasElement>()
    //     .expect("the_canvas_id was not a HtmlCanvasElement");

    let window_attributes = WindowAttributes::default().with_canvas(Some(html_canvas_element));

    event_loop
        .create_window(window_attributes)
        .expect("Failed to initialize window")
}

#[cfg(not(target_arch = "wasm32"))]
fn create_window(event_loop: &ActiveEventLoop) -> Window {
    let window_attributes = WindowAttributes::default()
        .with_title("highrock".to_string())
        .with_resizable(false)
        .with_maximized(true)
        // .with_fullscreen(Some(true))
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

    event_loop
        .create_window(window_attributes)
        .expect("Failed to initialize window")
}

use highrock_engine::UserScene;

struct MyScene;

impl UserScene for MyScene {
    fn setup(&mut self) {
        // log::info!("LOG SETUP");
    }
}

pub fn entry() {
    //pre init
    //init
    //load assets
    //load scene
    //play
    highrock_engine::log_setup(); //inits logs
    highrock_engine::engine_entry(Box::new(MyScene)); //start engine with a target scene
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() -> Result<(), String> {
    entry();
    Ok(())
}

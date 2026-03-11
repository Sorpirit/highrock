use thegame::entry;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    entry();
}

#[cfg(target_arch = "wasm32")]
fn main() {}

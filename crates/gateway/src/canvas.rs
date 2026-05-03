use std::sync::Mutex;
use pharmakon_common::visual_primitives::CanvasState;
use pharmakon_common::Event;

pub struct CanvasHost {
    state: Mutex<CanvasState>,
}

impl CanvasHost {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CanvasState { elements: Vec::new() }),
        }
    }

    pub fn handle_event(&self, event: &Event) {
        match event {
            Event::CanvasUpdate { primitive } => {
                let mut state = self.state.lock().unwrap();
                state.elements.push(primitive.clone());
            }
            Event::CanvasClear => {
                let mut state = self.state.lock().unwrap();
                state.elements.clear();
            }
            _ => {}
        }
    }

    pub fn get_state(&self) -> CanvasState {
        self.state.lock().unwrap().clone()
    }
}

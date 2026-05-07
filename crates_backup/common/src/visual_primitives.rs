use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum CanvasPrimitive {
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: String,
    },
    Circle {
        x: f32,
        y: f32,
        radius: f32,
        color: String,
    },
    Text {
        x: f32,
        y: f32,
        content: String,
        size: f32,
        color: String,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CanvasState {
    pub elements: Vec<CanvasPrimitive>,
}

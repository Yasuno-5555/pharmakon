use accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, PaintCtx, PropertiesMut, PropertiesRef,
    RegisterCtx, UpdateCtx, Widget, WidgetId,
};
use peniko::{Color, Fill};
use vello::Scene;
use vello::kurbo::{Affine, Circle, Point, Size, Stroke};

pub struct SwarmVisualizerWidget {
    pub swarms: Vec<crate::app::SwarmStatus>,
    pub time: f32,
}

impl SwarmVisualizerWidget {
    pub fn new(swarms: Vec<crate::app::SwarmStatus>) -> Self {
        Self { swarms, time: 0.0 }
    }
}

impl Widget for SwarmVisualizerWidget {
    type Action = ();

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        self.time += interval as f32 / 1_000_000_000.0;
        ctx.request_paint_only();
        ctx.request_anim_frame(); // Keep animating
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        bc.max()
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        let size = ctx.size();
        let center = Point::new(size.width / 2.0, size.height / 2.0);

        // Draw background glow
        let bg_glow = Circle::new(center, (self.time.sin() * 10.0 + 100.0) as f64);
        scene.fill(
            Fill::EvenOdd,
            Affine::IDENTITY,
            Color::from_rgb8(100, 50, 255),
            None,
            &bg_glow,
        );

        // Draw Swarm Nodes
        let radius = 120.0;
        let count = self.swarms.len();
        if count == 0 {
            return;
        }

        for (i, swarm) in self.swarms.iter().enumerate() {
            let angle = (i as f32 / count as f32) * 2.0 * std::f32::consts::PI + self.time * 0.2;
            let x = center.x + (angle.cos() * radius) as f64;
            let y = center.y + (angle.sin() * radius) as f64;
            let node_pos = Point::new(x, y);

            // Orbiting node
            let node_circle = Circle::new(node_pos, 15.0);
            let glow_color = if swarm.status.contains("Active") {
                Color::from_rgb8(0, 255, 200)
            } else {
                Color::from_rgb8(150, 150, 150)
            };

            scene.fill(
                Fill::EvenOdd,
                Affine::IDENTITY,
                glow_color,
                None,
                &node_circle,
            );

            // Connecting line to center
            scene.stroke(
                &Stroke::new(1.0),
                Affine::IDENTITY,
                Color::from_rgb8(255, 255, 255),
                None,
                &vello::kurbo::Line::new(center, node_pos),
            );
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Unknown
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("SwarmVisualizerWidget", id = id.to_raw())
    }
}

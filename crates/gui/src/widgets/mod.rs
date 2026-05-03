pub mod swarm_visualizer;

use swarm_visualizer::SwarmVisualizerWidget;
use xilem::{Pod, ViewCtx};
use xilem_core::{View, MessageResult, ViewMarker, MessageContext, Mut};

pub struct SwarmVisualizer {
    swarms: Vec<crate::app::SwarmStatus>,
}

pub fn swarm_visualizer(swarms: Vec<crate::app::SwarmStatus>) -> SwarmVisualizer {
    SwarmVisualizer { swarms }
}

impl ViewMarker for SwarmVisualizer {}

impl<State, Action> View<State, Action, ViewCtx> for SwarmVisualizer {
    type Element = Pod<SwarmVisualizerWidget>;
    type ViewState = ();

    fn build(&self, _ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let pod = Pod::new(SwarmVisualizerWidget::new(self.swarms.clone()));
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) {
        if self.swarms != prev.swarms {
            element.widget.swarms = self.swarms.clone();
        }
    }

    fn teardown(&self, _view_state: &mut Self::ViewState, _ctx: &mut ViewCtx, _element: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        _message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

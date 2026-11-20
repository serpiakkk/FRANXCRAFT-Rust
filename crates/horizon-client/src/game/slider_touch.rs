//! A finger on an options slider: the game keeps running with the costly values (render and
//! simulation distance, biome blend, LOD distance) while the slider shows the one under the
//! finger, and takes it up once it settled or on release (`touch_slider`).
use super::*;
use horizon_client::touch_slider::Costly;
use winit::event::TouchPhase;

impl Game<'_> {
    /// A slider grabbed by a finger: what the game runs with now is what a held value falls back to.
    pub(super) fn slider_begin(&mut self, id: u64, phys: [f32; 2]) {
        // The scroll gesture that started with the touch is not this drag's.
        self.mobile.ui.gesture(id, phys, TouchPhase::Cancelled);
        let applied = Costly::of(&self.settings);
        self.mobile.slider.settle().begin(applied);
    }
    /// `Event::SliderDrag`: true while a finger holds a costly value back from `apply_settings`.
    pub(super) fn slider_drag_held(&mut self) -> bool {
        if !self.mobile.slider.dragging() {
            return false;
        }
        let live = Costly::of(&self.settings);
        let settle = self.mobile.slider.settle();
        let held = settle.step(live, Instant::now());
        if let Some(applied) = settle.applied().filter(|_| held) {
            applied.put(&mut self.settings);
        }
        held
    }
    /// The touch will not come back (screen closed, touch lost): the value the player saw is
    /// applied and saved once, as a release would.
    pub(super) fn slider_abort(&mut self) {
        if let Some(live) = self.mobile.slider.reset() {
            live.put(&mut self.settings);
            self.settings.save(&Self::options_path());
            self.apply_settings();
        }
    }
    /// Every frame: a slider touch outlived the screen it began on.
    pub(super) fn slider_screen_guard(&mut self) {
        let gone = if self.mobile.slider_in_options {
            self.screen != Screen::Options
        } else {
            self.screen == Screen::Playing
        };
        if gone && self.mobile.slider.busy() {
            self.slider_abort();
        }
    }
    /// Every frame of the options: a value that stayed unchanged long enough applies.
    pub(super) fn slider_tick(&mut self) {
        if let Some(settled) = self.mobile.slider.settle().due(Instant::now()) {
            settled.put(&mut self.settings);
            self.apply_settings();
        }
    }
    /// The finger lifts: the value still held goes back into the settings, and the release's
    /// `Event::Changed` applies and saves it.
    pub(super) fn slider_flush(&mut self) {
        if let Some(live) = self.mobile.slider.settle().finish() {
            live.put(&mut self.settings);
        }
    }
    /// The costly values the slider shows, in the settings for as long as `draw` takes.
    pub(super) fn with_shown_sliders<R>(&mut self, draw: impl FnOnce(&mut Self) -> R) -> R {
        let Some(shown) = self.mobile.slider.settle().shown() else {
            return draw(self);
        };
        let held = Costly::of(&self.settings);
        shown.put(&mut self.settings);
        let result = draw(self);
        held.put(&mut self.settings);
        result
    }
}

//! Touch sliders in menus: a finger that lands on a slider only grabs it once it moves sideways
//! (a tap or a vertical swipe leaves the value alone and scrolls), and the values whose change
//! reloads chunks apply once they have settled instead of at every step of the drag.
use std::time::{Duration, Instant};
use winit::event::TouchPhase;

/// Physical pixels a touch may move before it is a drag (the menu gesture's own slop).
pub const SLOP: f32 = 12.0;
/// How long a costly value stays unchanged before the drag applies it.
pub const SETTLE: Duration = Duration::from_millis(300);

/// What the menu does with a touch event of a slider touch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// Not a slider touch: the usual menu handling.
    Ignore,
    /// Landed on a slider, nothing decided yet (or lifted before moving): no press, no value.
    Hold,
    /// The touch became a slider drag: press now, at the finger.
    Begin,
    /// A slider drag moves with the finger.
    Drag,
    /// A slider drag ends.
    Release,
    /// Mostly vertical: the menu's scroll gesture owns it.
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Pending([f32; 2]),
    Drag,
    Scroll,
}

#[derive(Default)]
pub struct SliderTouch {
    owner: Option<(u64, Mode)>,
    settle: Settle,
}

/// Whether a GUI-space point is over a slider widget of the screen that was drawn last.
pub fn over_slider(p: [f32; 2]) -> bool {
    crate::widgets::last_targets().iter().any(|t| {
        let [x, y, w, h] = t.rect;
        matches!(t.kind, crate::widgets::Kind::Slider { .. })
            && (x..x + w).contains(&p[0])
            && (y..y + h).contains(&p[1])
    })
}

impl SliderTouch {
    /// One touch event; `phys` is in physical pixels, `on_slider` is whether a start lands on one.
    pub fn touch(&mut self, id: u64, phys: [f32; 2], phase: TouchPhase, on_slider: bool) -> Grab {
        let ended = matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled);
        let Some((owner, mode)) = self.owner else {
            if phase == TouchPhase::Started && on_slider {
                self.owner = Some((id, Mode::Pending(phys)));
                return Grab::Hold;
            }
            return Grab::Ignore;
        };
        if owner != id || phase == TouchPhase::Started {
            return Grab::Ignore;
        }
        if ended {
            self.owner = None;
        }
        match mode {
            Mode::Drag if ended => Grab::Release,
            Mode::Drag => Grab::Drag,
            Mode::Scroll => Grab::Scroll,
            Mode::Pending(_) if ended => Grab::Hold,
            Mode::Pending(start) => self.decide(id, start, phys),
        }
    }
    fn decide(&mut self, id: u64, start: [f32; 2], p: [f32; 2]) -> Grab {
        let (dx, dy) = ((p[0] - start[0]).abs(), (p[1] - start[1]).abs());
        if dx.hypot(dy) <= SLOP {
            return Grab::Hold;
        }
        let horizontal = dx > dy;
        self.owner = Some((id, if horizontal { Mode::Drag } else { Mode::Scroll }));
        if horizontal {
            Grab::Begin
        } else {
            Grab::Scroll
        }
    }
    /// Whether a slider is being dragged by a finger.
    pub fn dragging(&self) -> bool {
        matches!(self.owner, Some((_, Mode::Drag)))
    }
    /// A finger owns a slider touch: other fingers must leave the cursor and focus alone.
    pub fn busy(&self) -> bool {
        self.owner.is_some()
    }
    /// The touch will not come back (screen left, touch lost): forget it, and hand back the
    /// costly value that was still waiting.
    pub fn reset(&mut self) -> Option<Costly> {
        self.owner = None;
        self.settle.finish()
    }
    pub fn settle(&mut self) -> &mut Settle {
        &mut self.settle
    }
}

/// The options that make the game reload or rebuild chunks when they change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Costly {
    render: i32,
    simulation: i32,
    biome_blend: i32,
    lod: u32,
}

impl Costly {
    pub fn of(s: &crate::settings::Settings) -> Self {
        Self {
            render: s.render_distance,
            simulation: s.simulation_distance,
            biome_blend: s.biome_blend,
            lod: s.horizon.lod_distance,
        }
    }
    pub fn put(self, s: &mut crate::settings::Settings) {
        s.render_distance = self.render;
        s.simulation_distance = self.simulation;
        s.biome_blend = self.biome_blend;
        s.horizon.lod_distance = self.lod;
    }
}

/// While a finger drags a slider: the costly values the game runs with, and the one the slider
/// shows, which the game takes up after it stayed unchanged for [`SETTLE`] or on release.
#[derive(Default)]
pub struct Settle {
    applied: Option<Costly>,
    shown: Option<(Costly, Instant)>,
}

impl Settle {
    /// A drag starts, with the values the game runs with now.
    pub fn begin(&mut self, applied: Costly) {
        self.applied = Some(applied);
        self.shown = None;
    }
    /// A slider step led to `live`. True when it must not be applied yet; the game then runs
    /// with [`Settle::applied`] and the slider shows [`Settle::shown`].
    pub fn step(&mut self, live: Costly, now: Instant) -> bool {
        let Some(applied) = self.applied else {
            return false;
        };
        if live == applied {
            self.shown = None;
            return false;
        }
        if self.shown.is_none_or(|(shown, _)| shown != live) {
            self.shown = Some((live, now));
        }
        true
    }
    pub fn applied(&self) -> Option<Costly> {
        self.applied
    }
    pub fn shown(&self) -> Option<Costly> {
        self.shown.map(|(live, _)| live)
    }
    /// The value to apply now, once it stayed unchanged for [`SETTLE`]; never twice.
    pub fn due(&mut self, now: Instant) -> Option<Costly> {
        let (live, since) = self.shown?;
        if now.saturating_duration_since(since) < SETTLE {
            return None;
        }
        self.applied = Some(live);
        self.shown = None;
        Some(live)
    }
    /// The drag ends: the value still waiting, which the release applies.
    pub fn finish(&mut self) -> Option<Costly> {
        self.applied = None;
        self.shown.take().map(|(live, _)| live)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(moves: &[[f32; 2]], end: TouchPhase) -> Vec<Grab> {
        let mut t = SliderTouch::default();
        let mut out = vec![t.touch(1, [100., 100.], TouchPhase::Started, true)];
        out.extend(
            moves
                .iter()
                .map(|&p| t.touch(1, p, TouchPhase::Moved, true)),
        );
        out.push(t.touch(1, *moves.last().unwrap_or(&[100., 100.]), end, true));
        out
    }

    #[test]
    fn a_touch_off_a_slider_is_ignored() {
        let mut t = SliderTouch::default();
        assert_eq!(
            t.touch(1, [0., 0.], TouchPhase::Started, false),
            Grab::Ignore
        );
        assert_eq!(
            t.touch(1, [50., 0.], TouchPhase::Moved, false),
            Grab::Ignore
        );
    }

    #[test]
    fn a_tap_holds_and_never_presses() {
        let grabs = run(&[[104., 103.]], TouchPhase::Ended);
        assert_eq!(grabs, [Grab::Hold, Grab::Hold, Grab::Hold]);
    }

    #[test]
    fn sideways_movement_past_the_slop_grabs_then_drags_and_releases() {
        let grabs = run(
            &[[108., 100.], [113., 101.], [150., 105.]],
            TouchPhase::Ended,
        );
        let want = [
            Grab::Hold,
            Grab::Hold,
            Grab::Begin,
            Grab::Drag,
            Grab::Release,
        ];
        assert_eq!(grabs, want);
    }

    #[test]
    fn vertical_movement_scrolls_and_never_grabs() {
        let grabs = run(&[[102., 120.], [140., 160.]], TouchPhase::Ended);
        assert_eq!(
            grabs,
            [Grab::Hold, Grab::Scroll, Grab::Scroll, Grab::Scroll]
        );
    }

    #[test]
    fn a_cancelled_drag_releases_and_frees_the_slider() {
        let mut t = SliderTouch::default();
        t.touch(1, [100., 100.], TouchPhase::Started, true);
        assert_eq!(
            t.touch(1, [130., 100.], TouchPhase::Moved, true),
            Grab::Begin
        );
        assert!(t.dragging());
        assert_eq!(
            t.touch(1, [130., 100.], TouchPhase::Cancelled, true),
            Grab::Release
        );
        assert!(!t.dragging());
        assert_eq!(t.touch(2, [5., 5.], TouchPhase::Started, true), Grab::Hold);
    }

    #[test]
    fn a_second_finger_is_not_the_sliders() {
        let mut t = SliderTouch::default();
        t.touch(1, [100., 100.], TouchPhase::Started, true);
        assert_eq!(
            t.touch(2, [5., 5.], TouchPhase::Started, true),
            Grab::Ignore
        );
        assert_eq!(t.touch(2, [50., 5.], TouchPhase::Moved, true), Grab::Ignore);
    }

    #[test]
    fn another_finger_changes_nothing_of_a_drag_in_progress() {
        let mut t = SliderTouch::default();
        t.touch(1, [100., 100.], TouchPhase::Started, true);
        t.touch(1, [130., 100.], TouchPhase::Moved, true);
        for phase in [TouchPhase::Started, TouchPhase::Moved, TouchPhase::Ended] {
            assert_eq!(t.touch(2, [5., 5.], phase, true), Grab::Ignore);
            assert!(t.busy() && t.dragging());
        }
        assert_eq!(
            t.touch(1, [140., 100.], TouchPhase::Moved, true),
            Grab::Drag
        );
    }

    #[test]
    fn reset_frees_the_slider_and_hands_back_the_waiting_value() {
        let mut t = SliderTouch::default();
        t.touch(1, [100., 100.], TouchPhase::Started, true);
        t.touch(1, [130., 100.], TouchPhase::Moved, true);
        t.settle().begin(costly(12));
        assert!(t.settle().step(costly(30), Instant::now()));
        assert_eq!(t.reset(), Some(costly(30)));
        assert!(!t.busy() && !t.dragging());
        assert_eq!(t.settle().shown(), None);
        assert_eq!(t.reset(), None);
        assert_eq!(t.touch(2, [5., 5.], TouchPhase::Started, true), Grab::Hold);
    }

    fn costly(render: i32) -> Costly {
        let s = crate::settings::Settings {
            render_distance: render,
            ..Default::default()
        };
        Costly::of(&s)
    }

    #[test]
    fn a_costly_value_applies_once_after_300_ms_unchanged() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut s = Settle::default();
        s.begin(costly(12));
        assert!(s.step(costly(16), ms(0)));
        assert!(s.step(costly(20), ms(200)));
        assert_eq!(s.due(ms(450)), None, "the value changed at 200 ms");
        assert!(
            s.step(costly(20), ms(480)),
            "same value again keeps the timer"
        );
        assert_eq!(s.shown(), Some(costly(20)));
        assert_eq!(s.due(ms(500)), Some(costly(20)));
        assert_eq!(s.due(ms(900)), None, "never twice for the same value");
        assert_eq!(s.applied(), Some(costly(20)));
        assert_eq!(s.finish(), None, "release has nothing left to apply");
    }

    #[test]
    fn release_hands_back_the_value_that_did_not_settle() {
        let t0 = Instant::now();
        let mut s = Settle::default();
        s.begin(costly(12));
        assert!(s.step(costly(30), t0));
        assert_eq!(s.finish(), Some(costly(30)));
        assert!(!s.step(costly(31), t0), "no drag, nothing is held");
    }

    #[test]
    fn dragging_back_to_the_applied_value_applies_it_directly() {
        let t0 = Instant::now();
        let mut s = Settle::default();
        s.begin(costly(12));
        assert!(s.step(costly(30), t0));
        assert!(!s.step(costly(12), t0));
        assert_eq!(s.shown(), None);
        assert_eq!(s.due(t0 + SETTLE * 2), None);
    }

    #[test]
    fn costly_values_round_trip_through_settings() {
        let mut s = crate::settings::Settings::default();
        let before = Costly::of(&s);
        s.render_distance += 5;
        s.horizon.lod_distance += 7;
        before.put(&mut s);
        assert_eq!(Costly::of(&s), before);
    }
}

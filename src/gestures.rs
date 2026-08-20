//! Trackpad swipes, read out of the scroll stream.
//!
//! winit gives macOS no gesture events at all: `PanGesture` is emitted only by
//! the iOS backend, and the macOS view implements magnify, smartMagnify and
//! rotate and nothing else. So a swipe has to be recognised from the ordinary
//! `MouseWheel` events — the same ones the web page and the sidebar scroll
//! from — and the whole problem is telling the two apart without breaking
//! either.
//!
//! Three rules do it.
//!
//! Recognition happens when the fingers lift, not partway through the stroke.
//! Firing the moment travel crosses a threshold means the duration and
//! straightness tests never get to run — 45 points of ordinary scrolling take
//! a fraction of the time a flick is allowed, and the stroke has barely begun
//! to wander — so every deliberate horizontal scroll would trip it. Waiting
//! for the lift means the test is applied to the whole gesture.
//!
//! Momentum is rejected by when it arrives. macOS reports inertia with its own
//! phase run, and winit gives `momentumPhase` priority over `phase`
//! (macos/view.rs:683-689), so an inertia tail looks exactly like a second
//! flick. It is not distinguishable by shape — only by the fact that it begins
//! immediately after a stroke that moved.
//!
//! And a direction is only offered where nothing scrolls that way. Horizontal
//! is free: no part of the chrome scrolls sideways, and on a page it is the
//! gesture every browser already uses for back and forward. Vertical is
//! scrolling almost everywhere, so it is recognised only over the strip above
//! the web page, where there is nothing to scroll.

use std::time::{Duration, Instant};

use egui::Vec2;
use serde::{Deserialize, Serialize};
use winit::event::TouchPhase;

/// How far a flick has to travel along its own axis, in points.
const MIN_TRAVEL: f32 = 45.0;
/// Past this it is a pan, however far it went.
const MAX_DURATION: Duration = Duration::from_millis(350);
/// How far it may wander off its axis, as a fraction of the travel along it.
const CROSS_RATIO: f32 = 0.5;
/// Points per second. A scroll can be long or straight; it is rarely both and
/// fast as well.
const MIN_SPEED: f32 = 320.0;
/// Inertia begins this soon after the fingers lift, and nothing else does.
const MOMENTUM_GAP: Duration = Duration::from_millis(160);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// What a swipe can be bound to.
///
/// Nothing here destroys anything. A mis-swipe on a page that scrolls
/// sideways is not a remote possibility, and there is no undo for a closed
/// tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GestureAction {
    None,
    Back,
    Forward,
    ToggleSidebar,
    /// Pull the navigation bar open over its widget shelf, or put it away.
    ToggleShelf,
    NextWorkspace,
    PreviousWorkspace,
    NewTab,
}

impl GestureAction {
    pub const ALL: [GestureAction; 8] = [
        GestureAction::None,
        GestureAction::Back,
        GestureAction::Forward,
        GestureAction::ToggleSidebar,
        GestureAction::ToggleShelf,
        GestureAction::NextWorkspace,
        GestureAction::PreviousWorkspace,
        GestureAction::NewTab,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            GestureAction::None => "Nothing",
            GestureAction::Back => "Back",
            GestureAction::Forward => "Forward",
            GestureAction::ToggleSidebar => "Toggle sidebar",
            GestureAction::ToggleShelf => "Widget shelf",
            GestureAction::NextWorkspace => "Next workspace",
            GestureAction::PreviousWorkspace => "Previous workspace",
            GestureAction::NewTab => "New tab",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gestures {
    pub enabled: bool,
    /// Two fingers moving left and right, anywhere in the window.
    pub left: GestureAction,
    pub right: GestureAction,
    /// Two fingers moving up and down, over the bar above the page only —
    /// everywhere else that is scrolling.
    pub up: GestureAction,
    pub down: GestureAction,
}

impl Default for Gestures {
    fn default() -> Self {
        Self {
            enabled: true,
            // Fingers right shows what is to the left, which is where you
            // came from. The same way round as every other browser.
            right: GestureAction::Back,
            left: GestureAction::Forward,
            up: GestureAction::None,
            down: GestureAction::ToggleShelf,
        }
    }
}

impl Gestures {
    pub fn action(&self, direction: Direction) -> GestureAction {
        match direction {
            Direction::Left => self.left,
            Direction::Right => self.right,
            Direction::Up => self.up,
            Direction::Down => self.down,
        }
    }
}

/// One stroke of the fingers, accumulated until they lift.
#[derive(Default)]
pub struct Recognizer {
    travel: Vec2,
    started: Option<Instant>,
    /// The stroke began inside the momentum window, so it is inertia from the
    /// previous one and cannot be a gesture.
    inertia: bool,
    /// When the last stroke that actually moved ended.
    settled: Option<Instant>,
}

impl Recognizer {
    /// Feed one scroll event. Returns a direction only on the frame the
    /// fingers lift, and only if the whole stroke was a flick.
    pub fn feed(&mut self, delta: Vec2, phase: TouchPhase, now: Instant) -> Option<Direction> {
        match phase {
            TouchPhase::Started => {
                // A Started while a stroke is already open is AppKit's
                // MayBegin followed by Began — the fingers landing and then
                // moving. One stroke, not two.
                if self.started.is_none() {
                    self.started = Some(now);
                    self.travel = Vec2::ZERO;
                    self.inertia = self
                        .settled
                        .is_some_and(|end| now.saturating_duration_since(end) < MOMENTUM_GAP);
                }
                None
            },
            TouchPhase::Moved => {
                self.travel += delta;
                None
            },
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let started = self.started.take();
                let moved = self.travel.length() > 1.0;
                // Only a stroke that went somewhere leaves inertia behind, so
                // resting two fingers and lifting them again — which AppKit
                // also reports as a phase run — must not arm the window.
                self.settled = moved.then_some(now);
                let direction = started
                    .filter(|_| !self.inertia)
                    .and_then(|start| self.recognise(now.saturating_duration_since(start)));
                self.travel = Vec2::ZERO;
                direction
            },
        }
    }

    fn recognise(&self, elapsed: Duration) -> Option<Direction> {
        if elapsed > MAX_DURATION {
            return None;
        }
        let (along, across, horizontal) = if self.travel.x.abs() >= self.travel.y.abs() {
            (self.travel.x, self.travel.y.abs(), true)
        } else {
            (self.travel.y, self.travel.x.abs(), false)
        };
        if along.abs() < MIN_TRAVEL
            || across > along.abs() * CROSS_RATIO
            || along.abs() / elapsed.as_secs_f32().max(0.001) < MIN_SPEED
        {
            return None;
        }
        Some(match (horizontal, along > 0.0) {
            (true, true) => Direction::Right,
            (true, false) => Direction::Left,
            (false, true) => Direction::Down,
            (false, false) => Direction::Up,
        })
    }
}

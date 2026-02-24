use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tachyonfx::fx::{self, Direction};
use tachyonfx::{Duration, Effect, Shader};

/// Manages active animation effects and advances them each tick.
pub struct AnimationManager {
    last_tick: Instant,
    effects: Vec<ActiveEffect>,
}

struct ActiveEffect {
    effect: Effect,
    area: Rect,
}

impl AnimationManager {
    pub fn new() -> Self {
        Self {
            last_tick: Instant::now(),
            effects: Vec::new(),
        }
    }

    /// Queue a new effect to play in the given area.
    pub fn add_effect(&mut self, effect: Effect, area: Rect) {
        self.effects.push(ActiveEffect { effect, area });
    }

    /// Called each frame. Advances all effects and applies them to the buffer.
    /// Returns true if any effects are still active.
    pub fn tick(&mut self, buf: &mut Buffer) -> bool {
        let now = Instant::now();
        let elapsed_std = now.duration_since(self.last_tick);
        let elapsed = Duration::from_millis(elapsed_std.as_millis() as u32);
        self.last_tick = now;

        self.effects.retain_mut(|active| {
            active.effect.process(elapsed, buf, active.area);
            !active.effect.done()
        });

        !self.effects.is_empty()
    }

    /// Whether any effects are currently running.
    pub fn has_active_effects(&self) -> bool {
        !self.effects.is_empty()
    }

    /// Create a slide effect for scroll boundary bounce.
    pub fn bounce_effect(direction: BounceDirection) -> Effect {
        let dir = match direction {
            BounceDirection::Up => Direction::UpToDown,
            BounceDirection::Down => Direction::DownToUp,
        };

        fx::sequence(&[
            fx::slide_in(dir, 0, 0, Color::Reset, Duration::from_millis(100)),
        ])
    }

    /// Create a dissolve-then-coalesce transition for switching emails.
    pub fn email_transition_effect() -> Effect {
        fx::sequence(&[
            fx::dissolve(Duration::from_millis(80)),
            fx::coalesce(Duration::from_millis(120)),
        ])
    }

    /// Create a sweep effect for pull-to-refresh.
    pub fn refresh_effect() -> Effect {
        fx::sweep_in(
            Direction::LeftToRight,
            2,
            0,
            Color::Cyan,
            Duration::from_millis(300),
        )
    }

    /// Fade in effect for initial load.
    pub fn fade_in_effect() -> Effect {
        fx::coalesce(Duration::from_millis(200))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BounceDirection {
    Up,
    Down,
}

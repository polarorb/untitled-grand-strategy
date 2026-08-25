//! Global Tension: how close the world is to the brink.
//! Spec: docs/design/systems/tension.md

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::SimClock;

pub mod tuning {
    /// Scenario start value, 1950-01-01 (30.0 displayed).
    pub const START_1950: i32 = 300;
    /// Structural minimum for the era; decay stops here.
    pub const ERA_FLOOR: i32 = 150;
    /// Internal tenths shed per day when above the floor...
    pub const BASE_DECAY_PER_DAY: i32 = 2;
    /// ...plus value/this, so high tension cools faster in absolute terms.
    pub const DECAY_SCALE_DIVISOR: i32 = 250;
    pub const MAX: i32 = 1000;
}

/// World tension in internal tenths (0..=1000). Displayed as 0.0–100.0.
/// Integer on purpose: tension gates discrete outcomes and must be exactly
/// reproducible.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalTension {
    value: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensionBand {
    Calm,
    Wary,
    Crisis,
    Brink,
}

impl GlobalTension {
    pub fn new(value: i32) -> Self {
        Self {
            value: value.clamp(0, tuning::MAX),
        }
    }

    pub fn value(&self) -> i32 {
        self.value
    }

    pub fn displayed(&self) -> f32 {
        self.value as f32 / 10.0
    }

    pub fn band(&self) -> TensionBand {
        match self.value {
            ..250 => TensionBand::Calm,
            250..500 => TensionBand::Wary,
            500..750 => TensionBand::Crisis,
            _ => TensionBand::Brink,
        }
    }

    /// The only mutation path besides decay. Clamps to [0, MAX].
    pub fn apply(&mut self, delta: i32) {
        self.value = (self.value + delta).clamp(0, tuning::MAX);
    }
}

impl std::fmt::Display for TensionBand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TensionBand::Calm => "Calm",
            TensionBand::Wary => "Wary",
            TensionBand::Crisis => "Crisis",
            TensionBand::Brink => "BRINK",
        };
        f.write_str(s)
    }
}

/// Daily decay toward the era floor. Below the floor, tension is left alone.
pub fn decay_tension(clock: Res<SimClock>, mut tension: ResMut<GlobalTension>) {
    if !clock.new_day {
        return;
    }
    if tension.value > tuning::ERA_FLOOR {
        let decay = tuning::BASE_DECAY_PER_DAY + tension.value / tuning::DECAY_SCALE_DIVISOR;
        tension.value = (tension.value - decay).max(tuning::ERA_FLOOR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{calendar::GameDate, run_ticks, SimPlugin};
    use bevy_app::App;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1,
        });
        app
    }

    #[test]
    fn decays_once_per_day() {
        let mut app = app();
        run_ticks(&mut app, 23);
        assert_eq!(
            app.world().resource::<GlobalTension>().value(),
            tuning::START_1950,
            "no decay before midnight"
        );
        run_ticks(&mut app, 1);
        // 300 -> decay 2 + 300/250 = 3
        assert_eq!(app.world().resource::<GlobalTension>().value(), 297);
    }

    #[test]
    fn decay_stops_at_era_floor() {
        let mut app = app();
        run_ticks(&mut app, 24 * 200);
        assert_eq!(
            app.world().resource::<GlobalTension>().value(),
            tuning::ERA_FLOOR
        );
    }

    #[test]
    fn bands_match_spec_boundaries() {
        assert_eq!(GlobalTension::new(0).band(), TensionBand::Calm);
        assert_eq!(GlobalTension::new(249).band(), TensionBand::Calm);
        assert_eq!(GlobalTension::new(250).band(), TensionBand::Wary);
        assert_eq!(GlobalTension::new(499).band(), TensionBand::Wary);
        assert_eq!(GlobalTension::new(500).band(), TensionBand::Crisis);
        assert_eq!(GlobalTension::new(750).band(), TensionBand::Brink);
        assert_eq!(GlobalTension::new(1000).band(), TensionBand::Brink);
    }

    #[test]
    fn apply_clamps_to_range() {
        let mut t = GlobalTension::new(990);
        t.apply(500);
        assert_eq!(t.value(), tuning::MAX);
        t.apply(-5000);
        assert_eq!(t.value(), 0);
    }
}

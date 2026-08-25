//! The deterministic simulation core.
//!
//! Hard rules for everything in this crate (see CLAUDE.md "Determinism"):
//! - No wall-clock time, no `std::time` in gameplay logic.
//! - All randomness flows through [`rng::SimRng`], seeded at campaign start.
//! - No iteration over `HashMap`/`HashSet` where order affects outcomes;
//!   use `BTreeMap` or sort first.
//! - No rendering, windowing, audio, or asset-server dependencies. This
//!   crate must run headless: `cargo test -p ugs-sim` exercises real ticks.
//!
//! One tick = one in-game hour. The presentation layer (ugs-app) decides how
//! many ticks to run per real second based on game speed; the sim only knows
//! "advance one tick".

pub mod agriculture;
pub mod calendar;
pub mod command;
pub mod demography;
pub mod economy;
pub mod events;
pub mod military;
pub mod planning;
pub mod rng;
pub mod savegame;
pub mod tension;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;

use calendar::GameDate;
use command::PendingCommands;
use rng::SimRng;
use tension::GlobalTension;

/// The schedule that advances the simulation exactly one hour.
/// Presentation/tests call [`run_ticks`]; nothing in this schedule may read
/// real time or unseeded entropy.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimTick;

/// Ordered stages within a tick. Cross-system ordering goes through these
/// sets, never through ad-hoc `.after(some_fn)` chains.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum TickSet {
    /// Advance clock/date, roll daily & monthly flags.
    Time,
    /// Player/AI commands queued since last tick are applied.
    Commands,
    /// Economy, research, production (mostly daily cadence).
    Economy,
    /// Diplomacy, influence, espionage, escalation.
    Politics,
    /// Movement, combat, supply.
    Military,
    /// Derived state, victory checks, event emission for the UI.
    Resolve,
}

/// Global simulation clock. `tick` is total hours elapsed since campaign
/// start; everything else derives from it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimClock {
    pub tick: u64,
    pub date: GameDate,
    /// True on the tick that begins a new day (midnight) / month.
    pub new_day: bool,
    pub new_month: bool,
}

impl SimClock {
    pub fn at_start(date: GameDate) -> Self {
        Self {
            tick: 0,
            date,
            new_day: false,
            new_month: false,
        }
    }
}

pub struct SimPlugin {
    pub start_date: GameDate,
    pub seed: u64,
}

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(SimTick);
        app.insert_resource(SimClock::at_start(self.start_date));
        app.insert_resource(SimRng::seeded(self.seed));
        app.insert_resource(savegame::SimSeed(self.seed));
        app.insert_resource(savegame::StartDate(self.start_date));
        app.init_resource::<savegame::CommandLog>();
        app.insert_resource(GlobalTension::new(tension::tuning::START_1950));
        app.init_resource::<PendingCommands>();
        app.init_resource::<demography::Demographics>();
        app.init_resource::<demography::LivingStandards>();
        app.init_resource::<economy::EconomyStatic>();
        app.init_resource::<economy::NationalBalances>();
        app.init_resource::<economy::RegionalPower>();
        app.init_resource::<planning::Economies>();
        app.init_resource::<agriculture::Agriculture>();
        app.init_resource::<military::Military>();
        app.init_resource::<military::PlayerCountry>();
        app.init_resource::<events::FiredEvents>();
        app.configure_sets(
            SimTick,
            (
                TickSet::Time,
                TickSet::Commands,
                TickSet::Economy,
                TickSet::Politics,
                TickSet::Military,
                TickSet::Resolve,
            )
                .chain(),
        );
        app.add_systems(SimTick, advance_clock.in_set(TickSet::Time));
        app.add_systems(SimTick, command::apply_commands.in_set(TickSet::Commands));
        app.add_systems(
            SimTick,
            (tension::decay_tension, events::update_events)
                .chain()
                .in_set(TickSet::Politics),
        );
        app.add_systems(SimTick, military::update_military.in_set(TickSet::Military));
        app.add_systems(
            SimTick,
            (
                demography::update_demographics,
                economy::update_economy,
                planning::update_production,
                agriculture::update_agriculture,
            )
                .chain()
                .in_set(TickSet::Economy),
        );
    }
}

fn advance_clock(mut clock: ResMut<SimClock>) {
    clock.tick += 1;
    let prev = clock.date;
    clock.date = prev.plus_hours(1);
    clock.new_day = clock.date.day != prev.day;
    clock.new_month = clock.date.month != prev.month;
}

/// Advance the simulation by `n` hours. The only entry point for moving
/// game time forward, shared by the app, tests, and future AI harnesses.
pub fn run_ticks(app: &mut App, n: u64) {
    for _ in 0..n {
        app.world_mut().run_schedule(SimTick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 42,
        });
        app
    }

    #[test]
    fn clock_advances_one_hour_per_tick() {
        let mut app = test_app();
        run_ticks(&mut app, 25);
        let clock = app.world().resource::<SimClock>();
        assert_eq!(clock.tick, 25);
        assert_eq!(clock.date, GameDate::new(1950, 1, 2, 1));
    }

    #[test]
    fn new_month_flag_fires_at_february() {
        let mut app = test_app();
        run_ticks(&mut app, 31 * 24);
        let clock = app.world().resource::<SimClock>();
        assert_eq!(clock.date, GameDate::new(1950, 2, 1, 0));
        assert!(clock.new_month);
        assert!(clock.new_day);
    }

    #[test]
    fn identical_seeds_produce_identical_streams() {
        let mut a = SimRng::seeded(7);
        let mut b = SimRng::seeded(7);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}

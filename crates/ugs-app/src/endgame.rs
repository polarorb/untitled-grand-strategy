//! The failure state. Crawford's rule: we do not reward failure — no
//! mushroom cloud, no score screen. The teletype prints the cities and
//! falls silent; the ledger is exact because there is no one left to
//! estimate; the attribution comes from the command log; the epitaph
//! is the date the peace failed. Load menu only.

use bevy::prelude::*;
use ugs_sim::crisis::GameOver;
use ugs_sim::SimClock;

use crate::{font, AppState, Fonts, GameSpeed, World1950};

#[derive(Component)]
struct FuneralScreen;

pub struct EndgamePlugin;

impl Plugin for EndgamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, the_end.run_if(in_state(AppState::InGame)));
    }
}

fn the_end(
    mut commands: Commands,
    game_over: Option<Res<GameOver>>,
    world: Res<World1950>,
    clock: Res<SimClock>,
    fonts: Res<Fonts>,
    mut speed: ResMut<GameSpeed>,
    existing: Query<(), With<FuneralScreen>>,
) {
    let Some(go) = game_over else {
        return;
    };
    // The sim never ticks again.
    speed.paused = true;
    if !existing.is_empty() {
        return;
    }
    let name = |tag: &ugs_data::CountryTag| {
        world
            .0
            .nations_meta
            .get(tag)
            .map(|m| m.display_name.to_uppercase())
            .unwrap_or_else(|| tag.0.clone())
    };
    let years = go.tick / (24 * 365);
    let months = (go.tick % (24 * 365)) / (24 * 30);
    commands
        .spawn((
            FuneralScreen,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.01, 0.02, 0.985)),
            GlobalZIndex(100),
        ))
        .with_children(|s| {
            s.spawn((
                Text::new("*** FLASH *** FLASH *** FLASH ***"),
                font(&fonts.mono_bold, 15.0),
                TextColor(Color::srgb(0.75, 0.25, 0.2)),
            ));
            s.spawn((
                Text::new("STRATEGIC WARNING CONFIRMED. MULTIPLE LAUNCHES."),
                font(&fonts.mono, 14.0),
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
            ));
            s.spawn((
                Text::new(""),
                font(&fonts.mono, 10.0),
                TextColor(Color::BLACK),
            ));
            for (city, pop) in go.cities.iter().take(8) {
                s.spawn((
                    Text::new(format!(
                        "{} (POP {:.1}M) -- NO FURTHER TRANSMISSIONS",
                        city.to_uppercase(),
                        *pop as f64 / 1e6
                    )),
                    font(&fonts.mono, 13.0),
                    TextColor(Color::srgb(0.62, 0.62, 0.62)),
                ));
            }
            s.spawn((
                Text::new("..."),
                font(&fonts.mono, 13.0),
                TextColor(Color::srgb(0.45, 0.45, 0.45)),
            ));
            s.spawn((
                Text::new("TRANSMISSION ENDS"),
                font(&fonts.mono_bold, 16.0),
                TextColor(Color::srgb(0.55, 0.55, 0.55)),
            ));
            s.spawn((
                Text::new(""),
                font(&fonts.mono, 14.0),
                TextColor(Color::BLACK),
            ));
            s.spawn((
                Text::new(format!("ESTIMATED DEAD: {}", go.dead)),
                font(&fonts.mono, 13.0),
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
            ));
            s.spawn((
                Text::new(format!(
                    "ESCALATION INITIATED BY: {} -- AGAINST {}",
                    name(&go.initiator),
                    name(&go.against)
                )),
                font(&fonts.mono, 13.0),
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
            ));
            s.spawn((
                Text::new(format!("THE PEACE HELD {years} YEARS, {months} MONTHS.")),
                font(&fonts.display, 18.0),
                TextColor(Color::srgb(0.83, 0.69, 0.36)),
            ));
            s.spawn((
                Text::new(format!("{}", clock.date)),
                font(&fonts.mono, 12.0),
                TextColor(Color::srgb(0.5, 0.5, 0.5)),
            ));
            s.spawn((
                Text::new(""),
                font(&fonts.mono, 14.0),
                TextColor(Color::BLACK),
            ));
            s.spawn((
                Text::new("WE DO NOT REWARD FAILURE. [F9: LOAD]"),
                font(&fonts.mono, 11.0),
                TextColor(Color::srgb(0.4, 0.4, 0.4)),
            ));
        });
}

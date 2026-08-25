//! Scripted historical events: fire by date, apply effects, log for the
//! presentation layer (teletype popups).

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::EventEffect;

use crate::demography::SimScenario;
use crate::military::{Military, Posture};
use crate::tension::GlobalTension;
use crate::SimClock;

/// Events that have fired, in order. The UI reads this to show popups;
/// `seen` is presentation-side state (not part of sim determinism).
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct FiredEvents {
    pub fired: Vec<String>,
}

pub fn update_events(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    mut fired: ResMut<FiredEvents>,
    mut tension: ResMut<GlobalTension>,
    mut military: ResMut<Military>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    let now = (
        clock.date.year,
        clock.date.month,
        clock.date.day,
        clock.date.hour,
    );
    for event in &data.events {
        if event.date > now || fired.fired.iter().any(|id| id == &event.id) {
            continue;
        }
        fired.fired.push(event.id.clone());
        for effect in &event.effects {
            match effect {
                EventEffect::AdjustTension(delta) => tension.apply(*delta),
                EventEffect::DeclareWar { a, b } => {
                    military.declare_war(a.clone(), b.clone());
                }
                EventEffect::SetPosture {
                    country,
                    enemy,
                    posture,
                } => {
                    let p = if posture == "Advance" {
                        Posture::Advance
                    } else {
                        Posture::Hold
                    };
                    military.postures.insert((country.clone(), enemy.clone()), p);
                }
                EventEffect::TransferProvinces { from, to, names } => {
                    for name in names {
                        if let Ok(id) = data.province_by_name(from, name) {
                            military.occupation.insert(id, to.clone());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;
    use ugs_data::CountryTag;

    fn app_with_scenario() -> App {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1950,
        });
        app.insert_resource(crate::demography::SimScenario(Arc::new(data)));
        app
    }

    #[test]
    #[ignore = "dev helper: writes saves/war-jul-1950.ron"]
    fn make_midwar_save() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 190); // July 10, 1950 — war in progress
        let save = crate::savegame::SaveGame::capture(app.world(), Some("KOR".into()));
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../saves");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("war-jul-1950.ron"), ron::to_string(&save).unwrap()).unwrap();
    }

    #[test]
    fn the_korean_war_begins_and_seoul_falls() {
        let mut app = app_with_scenario();
        // Run to June 24th: peace.
        run_ticks(&mut app, 24 * 174);
        {
            let military = app.world().resource::<Military>();
            assert!(military.wars.is_empty(), "war before June 25");
        }
        let tension_before = app
            .world()
            .resource::<GlobalTension>()
            .value();
        // Through June 25th: invasion fires.
        run_ticks(&mut app, 24 * 2);
        {
            let fired = app.world().resource::<FiredEvents>();
            assert!(fired.fired.iter().any(|id| id == "korea-invasion"));
            let military = app.world().resource::<Military>();
            let prk = CountryTag("PRK".into());
            let kor = CountryTag("KOR".into());
            assert!(military.at_war(&prk, &kor), "PRK at war with KOR");
            let tension_now = app.world().resource::<GlobalTension>().value();
            assert!(tension_now > tension_before + 100, "tension spiked");
        }
        // Three weeks in: the ROK is losing but still fighting (combat
        // must break armies before it annihilates them).
        run_ticks(&mut app, 24 * 21);
        {
            let military = app.world().resource::<Military>();
            let kor_alive = military.formations.values().any(|f| f.owner.0 == "KOR");
            assert!(kor_alive, "ROK annihilated in 3 weeks — combat too lethal");
        }
        // Eight weeks in: without outside intervention, Seoul has fallen
        // and the KPA holds most of the south — the historical
        // counterfactual this slice models (intervention comes later).
        run_ticks(&mut app, 24 * 35);
        let scenario = app.world().resource::<crate::demography::SimScenario>().clone();
        let military = app.world().resource::<Military>();
        let seoul = scenario
            .0
            .province_by_name(&CountryTag("KOR".into()), "Seoul")
            .unwrap();
        let holder = military.owner_of(seoul, &CountryTag("KOR".into()));
        assert_eq!(holder.0, "PRK", "Seoul should have fallen to the KPA");
        let occupied = military
            .occupation
            .values()
            .filter(|t| t.0 == "PRK")
            .count();
        assert!(occupied >= 10, "KPA should hold most of the south ({occupied})");
    }
}

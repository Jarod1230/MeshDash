//! Tests for settings that can be changed while the service runs.

use super::*;
use serde::Deserialize;

/// Stands in for a module's own settings type.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
struct Example {
    neighbours: bool,
    every_minutes: u64,
    silent_after_hours: i64,
}

impl Default for Example {
    fn default() -> Self {
        Self {
            neighbours: false,
            every_minutes: 30,
            silent_after_hours: 24,
        }
    }
}

fn from_file(section: serde_json::Value) -> Settings {
    let mut file = ModuleSettings::default();
    file.set("example", section);

    Settings::from_file(file)
}

async fn stored(section: serde_json::Value) -> Settings {
    let db = Database::open_in_memory().await.unwrap();
    let mut file = ModuleSettings::default();
    file.set("example", section);

    Settings::load(file, db, EventBus::new()).await.unwrap()
}

#[test]
fn a_module_without_a_section_gets_its_own_defaults() {
    let settings = Settings::default();

    assert_eq!(
        settings.get::<Example>("example").unwrap(),
        Example::default()
    );
}

#[test]
fn the_file_is_the_ground() {
    let settings = from_file(serde_json::json!({ "every_minutes": 5 }));

    assert_eq!(settings.get::<Example>("example").unwrap().every_minutes, 5);
}

#[tokio::test]
async fn a_change_wins_over_the_file() {
    let settings = stored(serde_json::json!({ "every_minutes": 5 })).await;

    settings
        .set::<Example>("example", serde_json::json!({ "every_minutes": 10 }))
        .await
        .unwrap();

    assert_eq!(
        settings.get::<Example>("example").unwrap().every_minutes,
        10
    );
}

#[tokio::test]
async fn changing_one_option_leaves_the_others_alone() {
    // Section-wise replacement would silently reset everything beside the
    // option somebody actually touched.
    let settings = stored(serde_json::json!({ "every_minutes": 5, "silent_after_hours": 6 })).await;

    settings
        .set::<Example>("example", serde_json::json!({ "neighbours": true }))
        .await
        .unwrap();

    let now = settings.get::<Example>("example").unwrap();
    assert!(now.neighbours);
    assert_eq!(now.every_minutes, 5, "the file's value survived");
    assert_eq!(now.silent_after_hours, 6);
}

#[tokio::test]
async fn an_option_the_module_does_not_have_is_refused() {
    // Keeping it quietly would leave the operator believing they changed
    // something.
    let settings = stored(serde_json::json!({})).await;

    let refused = settings
        .set::<Example>("example", serde_json::json!({ "evry_minutes": 10 }))
        .await;

    assert!(matches!(refused, Err(SetError::Rejected { .. })));
    assert_eq!(
        settings.get::<Example>("example").unwrap(),
        Example::default()
    );
}

#[tokio::test]
async fn a_value_of_the_wrong_kind_is_refused() {
    let settings = stored(serde_json::json!({})).await;

    let refused = settings
        .set::<Example>("example", serde_json::json!({ "every_minutes": "bald" }))
        .await;

    assert!(matches!(refused, Err(SetError::Rejected { .. })));
}

#[tokio::test]
async fn a_change_is_announced() {
    // A module that captured a value at start has no other way to hear of it.
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let mut listening = events.subscribe();
    let settings = Settings::load(ModuleSettings::default(), db, events)
        .await
        .unwrap();

    settings
        .set::<Example>("example", serde_json::json!({ "neighbours": true }))
        .await
        .unwrap();

    assert_eq!(
        listening.recv().await.unwrap(),
        AppEvent::SettingsChanged {
            module: "example".to_owned()
        }
    );
}

#[tokio::test]
async fn a_change_survives_a_restart() {
    let db = Database::open_in_memory().await.unwrap();
    let first = Settings::load(ModuleSettings::default(), db.clone(), EventBus::new())
        .await
        .unwrap();
    first
        .set::<Example>("example", serde_json::json!({ "neighbours": true }))
        .await
        .unwrap();

    // The same database, opened again the way a restart would.
    let again = Settings::load(ModuleSettings::default(), db, EventBus::new())
        .await
        .unwrap();

    assert!(again.get::<Example>("example").unwrap().neighbours);
    assert_eq!(again.changed_modules(), vec!["example".to_owned()]);
}

#[tokio::test]
async fn every_clone_sees_the_same_change() {
    // Modules each hold their own clone; a change made through one has to be
    // visible to all of them or half the service runs on stale settings.
    let settings = stored(serde_json::json!({})).await;
    let held_elsewhere = settings.clone();

    settings
        .set::<Example>("example", serde_json::json!({ "neighbours": true }))
        .await
        .unwrap();

    assert!(held_elsewhere.get::<Example>("example").unwrap().neighbours);
}

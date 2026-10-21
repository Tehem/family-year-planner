//! Languages (§19.2) and the configurable type catalog (§19.3–§19.8) over the demo store.

mod common;

use annee_server::config::AppConfig;
use annee_server::state::AppState;
use axum::http::StatusCode;
use axum_extra::extract::cookie::Key;
use common::TestApp;
use serde_json::{Value, json};

async fn logged_in() -> TestApp {
    let mut app = TestApp::demo().await;
    app.login().await;
    app
}

fn names(fr: &str, en: &str) -> Value {
    json!({"fr": {"singular": fr, "plural": format!("{fr}s")}, "en": {"singular": en, "plural": format!("{en}s")}})
}

/// The editor's current catalog, as the body of a `PUT`.
async fn catalog(app: &mut TestApp) -> Vec<Value> {
    let j = app.get("/api/admin/types").await.json();
    j["types"].as_array().unwrap().clone()
}

fn set_color(types: &mut [Value], code: &str, color: &str) {
    let t = types.iter_mut().find(|t| t["code"] == code).unwrap();
    t["color_id"] = json!(color);
}

fn custom(slot: u8, color: &str, fr: &str, en: &str) -> Value {
    json!({"code": format!("custom_{slot}"), "color_id": color, "labels": names(fr, en)})
}

// ---- Languages -----------------------------------------------------------------------------

#[tokio::test]
async fn the_language_cookie_localizes_config_errors_and_imported_titles() {
    let mut app = logged_in().await;
    let c = app.get("/api/config").await.json();
    assert_eq!(
        (c["lang"].as_str(), c["default_language"].as_str()),
        (Some("fr"), Some("fr"))
    );

    // `?lang=` is honoured and persisted in the `annee_lang` cookie.
    let r = app.get("/api/config?lang=en").await;
    assert!(
        r.header("set-cookie")
            .unwrap()
            .starts_with("annee_lang=en;")
    );
    let c = r.json();
    assert_eq!(c["lang"], "en");
    let trip = c["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "trip")
        .unwrap()
        .clone();
    assert_eq!(trip["label"], "Trip or vacation");
    assert_eq!(trip["label_plural"], "Trips & vacations");

    // The cookie alone keeps the language afterwards.
    assert_eq!(app.get("/api/config").await.json()["lang"], "en");
    let r = app
        .post(
            "/api/events",
            json!({"type": "trip", "title": " ", "start": "2026-07-01"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json()["error"]["message"], "Give the event a title.");

    // Imported titles are translated at read time, user titles are not.
    let events = app.get("/api/events?year=2026").await.json();
    let titles: Vec<&str> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.contains(&"Christmas Day") || titles.contains(&"Bastille Day"),
        "{titles:?}"
    );
    assert!(!titles.contains(&"Fête nationale"));

    // Back to French.
    app.get("/api/config?lang=fr").await;
    let r = app
        .post(
            "/api/events",
            json!({"type": "trip", "title": " ", "start": "2026-07-01"}),
        )
        .await;
    assert_eq!(
        r.json()["error"]["message"],
        "Donnez un titre à l’événement."
    );
    let events = app.get("/api/events?year=2026").await.json();
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["title"] == "Fête nationale")
    );
}

#[tokio::test]
async fn unknown_languages_are_ignored_and_the_instance_default_applies() {
    let mut config = AppConfig::default();
    config.app.default_language = "en".into();
    let state = AppState::demo(
        config,
        chrono::NaiveDate::from_ymd_opt(2026, 10, 16),
        Key::generate(),
    )
    .await
    .unwrap();
    let mut app = TestApp::with_state(state);
    app.login().await;
    assert_eq!(app.get("/api/config").await.json()["lang"], "en");
    assert_eq!(app.get("/api/config?lang=de").await.json()["lang"], "en");
    assert_eq!(app.get("/api/config?lang=fr").await.json()["lang"], "fr");
}

#[tokio::test]
async fn unauthorized_errors_follow_the_language() {
    let mut app = TestApp::demo().await;
    assert_eq!(
        app.get("/api/me").await.json()["error"]["message"],
        "Connectez-vous pour continuer."
    );
    assert_eq!(
        app.get("/api/me?lang=en").await.json()["error"]["message"],
        "Sign in to continue."
    );
}

// ---- Catalog editor --------------------------------------------------------------------------

#[tokio::test]
async fn the_editor_lists_the_defaults_the_palette_and_the_free_colors() {
    let mut app = logged_in().await;
    let j = app.get("/api/admin/types").await.json();
    assert_eq!(j["max_custom"], 3);
    assert_eq!(j["palette"].as_array().unwrap().len(), 11);
    assert_eq!(
        j["palette"][0],
        json!({"color_id": "1", "name_fr": "Lavande", "name_en": "Lavender"})
    );
    assert_eq!(j["free_colors"], json!(["1", "6", "7"]));
    assert_eq!(j["types"].as_array().unwrap().len(), 9);
    let trip = j["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "trip")
        .unwrap()
        .clone();
    assert_eq!(trip["color_id"], "9");
    assert_eq!(trip["labels"]["en"]["singular"], "Trip or vacation");
    assert!(j["migration"].is_null());
}

#[tokio::test]
async fn renaming_a_type_is_immediate_and_reaches_the_config() {
    let mut app = logged_in().await;
    let mut types = catalog(&mut app).await;
    let t = types.iter_mut().find(|t| t["code"] == "trip").unwrap();
    t["labels"]["fr"]["singular"] = json!("Escapade");
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json()["applied"], true);
    assert_eq!(r.json()["changes"], json!([]));
    let c = app.get("/api/config").await.json();
    let trip = c["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "trip")
        .unwrap()
        .clone();
    assert_eq!(trip["label"], "Escapade");
    assert_eq!(
        trip["label_plural"],
        "Escapades".replace("Escapades", "Voyages & vacances")
    );
}

#[tokio::test]
async fn recoloring_previews_the_event_count_and_keeps_events_typed() {
    let mut app = logged_in().await;
    let before = app.get("/api/events?year=2026").await.json();
    let trips = before["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "trip")
        .count();
    assert!(trips > 0);

    let mut types = catalog(&mut app).await;
    set_color(&mut types, "trip", "6");
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types, "dry_run": true})),
        )
        .await;
    let j = r.json();
    assert_eq!(j["applied"], false);
    assert_eq!(
        j["changes"],
        json!([{"code": "trip", "from_color_id": "9", "to_color_id": "6", "events": trips}])
    );
    // A dry run changes nothing.
    assert_eq!(
        app.get("/api/admin/types").await.json()["free_colors"],
        json!(["1", "6", "7"])
    );

    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types})),
        )
        .await;
    assert_eq!(r.json()["applied"], true);
    let j = app.get("/api/admin/types").await.json();
    assert_eq!(j["free_colors"], json!(["1", "7", "9"]));
    let after = app.get("/api/events?year=2026").await.json();
    assert_eq!(
        after["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["type"] == "trip")
            .count(),
        trips
    );
    let cfg = app.get("/api/config").await.json();
    assert_eq!(
        cfg["types"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["code"] == "trip")
            .unwrap()["color_id"],
        "6"
    );
}

#[tokio::test]
async fn two_types_can_swap_colors_in_one_save() {
    let mut app = logged_in().await;
    let mut types = catalog(&mut app).await;
    set_color(&mut types, "trip", "11");
    set_color(&mut types, "family_gathering", "9");
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types, "dry_run": true})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json()["changes"].as_array().unwrap().len(), 2);
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let cfg = app.get("/api/config").await.json();
    let color = |code: &str| {
        cfg["types"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["code"] == code)
            .unwrap()["color_id"]
            .clone()
    };
    assert_eq!(
        (color("trip"), color("family_gathering")),
        (json!("11"), json!("9"))
    );
}

#[tokio::test]
async fn invalid_catalogs_are_refused_with_a_localized_message() {
    let mut app = logged_in().await;
    let put = |types: Vec<Value>| json!({"types": types});

    let mut types = catalog(&mut app).await;
    set_color(&mut types, "trip", "11"); // already used by family_gathering
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(put(types.clone())),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(
        r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Tomate")
    );
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types?lang=en",
            Some(put(types)),
        )
        .await;
    assert!(
        r.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Tomato")
    );

    let mut types = catalog(&mut app).await;
    types.iter_mut().find(|t| t["code"] == "trip").unwrap()["labels"]["en"]["plural"] = json!("  ");
    assert_eq!(
        app.api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(put(types))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    let mut types = catalog(&mut app).await;
    types.retain(|t| t["code"] != "trip");
    assert_eq!(
        app.api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(put(types))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    let mut types = catalog(&mut app).await;
    types.push(json!({"code": "nonsense", "color_id": "6", "labels": names("A", "A")}));
    assert_eq!(
        app.api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(put(types))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // The 4th custom type does not exist: colors run out.
    let mut types = catalog(&mut app).await;
    types.extend([
        custom(1, "1", "A", "A"),
        custom(2, "6", "B", "B"),
        custom(3, "7", "C", "C"),
        custom(4, "5", "D", "D"),
    ]);
    assert_eq!(
        app.api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(put(types))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn a_custom_type_can_be_added_used_and_deleted() {
    let mut app = logged_in().await;
    let mut types = catalog(&mut app).await;
    types.push(custom(1, "6", "Sport", "Sports"));
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(
        app.get("/api/admin/types").await.json()["free_colors"],
        json!(["1", "7"])
    );
    let cfg = app.get("/api/config").await.json();
    let t = cfg["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "custom_1")
        .unwrap()
        .clone();
    assert_eq!(
        (
            t["label"].as_str(),
            t["custom"].as_bool(),
            t["render"].as_str(),
            t["writable"].as_bool()
        ),
        (Some("Sport"), Some(true), Some("bar"), Some(true))
    );

    let r = app.post("/api/events", json!({"type": "custom_1", "title": "Judo", "start": "2026-11-04", "members": ["Sacha"]})).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let ev = r.json();
    assert_eq!(
        (ev["type"].as_str(), ev["calendar"].as_str()),
        (Some("custom_1"), Some("famille"))
    );

    // Deleting the type reports its events and turns them unclassified.
    let mut types = catalog(&mut app).await;
    types.retain(|t| t["code"] != "custom_1");
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types, "dry_run": true})),
        )
        .await;
    assert_eq!(
        r.json()["changes"],
        json!([{"code": "custom_1", "from_color_id": "6", "to_color_id": null, "events": 1}])
    );
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": types})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let events = app.get("/api/events?year=2026").await.json();
    let judo = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == "Judo")
        .unwrap()
        .clone();
    assert_eq!(judo["type"], "unclassified");
    // The type code is unknown afterwards.
    let r = app
        .post(
            "/api/events",
            json!({"type": "custom_1", "title": "x", "start": "2026-11-04"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn writes_wait_while_colors_are_being_migrated() {
    let mut app = logged_in().await;
    app.state.types.set_migrating(true);
    let r = app
        .post(
            "/api/events",
            json!({"type": "trip", "title": "x", "start": "2026-11-04"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.json()["error"]["code"], "migrating");
    assert_eq!(
        app.get("/api/events?year=2026").await.status,
        StatusCode::OK,
        "reads keep working"
    );
    app.state.types.set_migrating(false);
    let r = app
        .post(
            "/api/events",
            json!({"type": "trip", "title": "x", "start": "2026-11-04"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
}

#[tokio::test]
async fn kiosks_and_anonymous_visitors_cannot_edit_types() {
    let mut app = TestApp::demo().await;
    assert_eq!(
        app.get("/api/admin/types").await.status,
        StatusCode::UNAUTHORIZED
    );
    let (_, token) = app
        .state
        .tokens
        .create("Kiosque", annee_server::tokens::Scope::Read, true, "T")
        .await
        .unwrap();
    let r = app.get(&format!("/kiosk?token={token}")).await;
    assert!(r.status.is_redirection() || r.status == StatusCode::OK);
    assert_eq!(
        app.get("/api/admin/types").await.status,
        StatusCode::FORBIDDEN
    );
    let r = app
        .api(
            axum::http::Method::PUT,
            "/api/admin/types",
            Some(json!({"types": []})),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

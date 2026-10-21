//! Web API (§7) over the demo store.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::TestApp;
use serde_json::{Value, json};

async fn logged_in() -> TestApp {
    let mut app = TestApp::demo().await;
    app.login().await;
    app
}

fn events(v: &Value) -> &Vec<Value> {
    v["events"].as_array().unwrap()
}

fn find<'a>(v: &'a Value, title: &str) -> &'a Value {
    events(v)
        .iter()
        .find(|e| e["title"] == title)
        .unwrap_or_else(|| panic!("no event {title}"))
}

#[tokio::test]
async fn healthz_needs_no_auth() {
    let mut app = TestApp::demo().await;
    let r = app.get("/healthz").await;
    assert_eq!(r.status, StatusCode::OK);
    let j = r.json();
    assert_eq!(j["status"], "ok");
    assert_eq!(j["mode"], "demo");
    assert!(j["version"].is_string());
}

#[tokio::test]
async fn api_requires_a_session() {
    let mut app = TestApp::demo().await;
    for uri in ["/api/me", "/api/config", "/api/events?year=2026"] {
        let r = app.get(uri).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert_eq!(r.json()["error"]["code"], "unauthorized");
    }
    let r = app.post("/api/events", json!({})).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pages_redirect_to_login_and_back() {
    let mut app = TestApp::demo().await;
    let r = app.get("/").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.header("location"), Some("/connexion?next=%2F"));
    let r = app.get("/reglages").await;
    assert_eq!(r.header("location"), Some("/connexion?next=%2Freglages"));
    assert_eq!(app.get("/connexion").await.status, StatusCode::OK);

    app.login().await;
    assert_eq!(app.get("/").await.status, StatusCode::OK);
    assert_eq!(app.get("/reglages").await.status, StatusCode::OK);
    // Already signed in: the login page sends you on.
    let r = app.get("/connexion?next=/reglages").await;
    assert_eq!(r.header("location"), Some("/reglages"));
}

#[tokio::test]
async fn open_redirects_are_refused() {
    let mut app = TestApp::demo().await;
    for next in ["//evil.example", "https://evil.example", "/\\evil"] {
        let r = app
            .send(
                Request::post(format!("/auth/demo?next={}", next.replace('\\', "%5C")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(r.header("location"), Some("/"), "{next}");
        app.clear_cookies();
    }
}

#[tokio::test]
async fn logout_ends_the_session() {
    let mut app = logged_in().await;
    assert_eq!(app.get("/api/me").await.status, StatusCode::OK);
    let r = app
        .send(Request::post("/auth/logout").body(Body::empty()).unwrap())
        .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(app.get("/api/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_and_config() {
    let mut app = logged_in().await;
    let me = app.get("/api/me").await.json();
    assert_eq!(
        me,
        json!({"email": "alex@demo.local", "name": "Alex", "kiosk": false, "demo": true})
    );
    let c = app.get("/api/config").await.json();
    assert_eq!(c["members"], json!(["Alex", "Camille", "Sacha", "Noé"]));
    assert_eq!(c["timezone"], "Europe/Paris");
    assert_eq!(c["school_zone"], "Zone B");
    assert_eq!(c["today"], "2026-10-16");
    let types = c["types"].as_array().unwrap();
    assert_eq!(types.len(), 9);
    let trip = types.iter().find(|t| t["code"] == "trip").unwrap();
    assert_eq!(trip["label"], "Voyage ou vacances");
    assert_eq!(trip["label_plural"], "Voyages & vacances");
    assert_eq!(trip["calendar"], "famille");
    assert_eq!(trip["writable"], true);
    assert_eq!(trip["render"], "bar");
}

#[tokio::test]
async fn list_events_returns_the_sample_year() {
    let mut app = logged_in().await;
    let v = app.get("/api/events?year=2026").await.json();
    assert_eq!(v["year"], 2026);
    let sacha = find(&v, "Anniversaire de Sacha");
    assert_eq!(sacha["type"], "birthday");
    assert_eq!(sacha["start"], "2026-03-12");
    assert_eq!(sacha["calendar"], "reperes");
    assert!(sacha["series_id"].is_string());
    assert_eq!(sacha["members"], json!([]));
    let toussaint = find(&v, "Vacances de la Toussaint");
    assert_eq!(toussaint["read_only"], true);
    assert_eq!(toussaint["source"], "import:education");
    let ski = find(&v, "Semaine de ski aux Arcs");
    assert_eq!(
        (ski["start"].as_str(), ski["end"].as_str()),
        (Some("2026-02-21"), Some("2026-02-28"))
    );
    assert!(
        ski.get("source_id").is_none(),
        "internal field is not exposed"
    );
    assert_eq!(
        app.get("/api/events?year=1800").await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn create_edit_delete_round_trip() {
    let mut app = logged_in().await;
    let r = app
        .post(
            "/api/events",
            json!({"type": "trip", "title": " Bretagne ", "start": "2026-08-10", "end": "2026-08-12",
                   "members": ["Alex", "Camille"], "location": "Quiberon", "notes": "Prendre les vélos"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let created = r.json();
    assert_eq!(created["title"], "Bretagne");
    assert_eq!(created["created_by"], "Alex");
    assert_eq!(created["members"], json!(["Alex", "Camille"]));
    let id = created["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("f:"));

    let v = app.get("/api/events?year=2026").await.json();
    assert_eq!(find(&v, "Bretagne")["location"], "Quiberon");

    let r = app
        .patch(
            &format!("/api/events/{id}"),
            json!({"title": "Bretagne en famille", "location": null, "end": "2026-08-14"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let updated = r.json();
    assert_eq!(updated["title"], "Bretagne en famille");
    assert_eq!(updated["location"], Value::Null);
    assert_eq!(
        updated["notes"], "Prendre les vélos",
        "omitted fields are unchanged"
    );
    assert_eq!(updated["end"], "2026-08-14");

    let r = app
        .patch(
            &format!("/api/events/{id}"),
            json!({"type": "daycare_closure"}),
        )
        .await;
    assert_eq!(
        r.json()["calendar"],
        "reperes",
        "type change moves to the other calendar"
    );

    let id = r.json()["id"].as_str().unwrap().to_string();
    assert_eq!(
        app.delete(&format!("/api/events/{id}")).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.delete(&format!("/api/events/{id}")).await.status,
        StatusCode::NOT_FOUND
    );
    let v = app.get("/api/events?year=2026").await.json();
    assert!(
        events(&v)
            .iter()
            .all(|e| e["title"] != "Bretagne en famille")
    );
}

#[tokio::test]
async fn validation_errors_are_french_400s() {
    let mut app = logged_in().await;
    let cases = [
        (
            json!({"type": "trip", "title": "  ", "start": "2026-08-10", "end": "2026-08-12"}),
            "Donnez un titre à l’événement.",
        ),
        (
            json!({"type": "trip", "title": "x", "start": "2026-08-10", "end": "2026-08-01"}),
            "Le dernier jour ne peut pas être avant le premier.",
        ),
        (
            json!({"type": "trip", "title": "x", "end": "2026-08-01"}),
            "Choisissez un premier et un dernier jour.",
        ),
        (
            json!({"type": "school_holiday", "title": "x", "start": "2026-08-10", "end": "2026-08-12"}),
            "Ce type d’événement ne peut pas être créé à la main.",
        ),
        (
            json!({"type": "birthday", "title": "x", "start": "2026-08-10", "end": "2026-08-12"}),
            "Un anniversaire dure un seul jour.",
        ),
        (
            json!({"type": "trip", "title": "x", "start": "2026-08-10", "end": "2026-08-12", "members": ["Papi"]}),
            "Personne inconnue : Papi.",
        ),
        (
            json!({"type": "birthday", "title": "x", "start": "2026-08-10", "end": "2026-08-10", "birth_year": 1850}),
            "Année de naissance invalide.",
        ),
        (
            json!({"type": "trip", "title": "x", "start": "2026-01-01", "end": "2027-06-01"}),
            "Un événement ne peut pas dépasser un an.",
        ),
    ];
    for (body, message) in cases {
        let r = app.post("/api/events", body.clone()).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}");
        let j = r.json();
        assert_eq!(j["error"]["code"], "validation_failed");
        assert_eq!(j["error"]["message"], message);
    }
    let r = app.api(Method::POST, "/api/events", None).await;
    assert_eq!(
        r.status,
        StatusCode::BAD_REQUEST,
        "missing body is a plain bad request"
    );
}

#[tokio::test]
async fn imported_events_are_read_only() {
    let mut app = logged_in().await;
    let v = app.get("/api/events?year=2026").await.json();
    let id = find(&v, "Vacances de la Toussaint")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = app
        .patch(&format!("/api/events/{id}"), json!({"title": "x"}))
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(
        r.json()["error"]["message"],
        "Cet événement est importé automatiquement et ne peut pas être modifié."
    );
    let r = app.delete(&format!("/api/events/{id}")).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let id = find(&v, "Noël")["id"].as_str().unwrap().to_string();
    assert_eq!(
        app.delete(&format!("/api/events/{id}")).await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn deleting_a_birthday_occurrence_deletes_the_series() {
    let mut app = logged_in().await;
    let v = app.get("/api/events?year=2027").await.json();
    let sacha = find(&v, "Anniversaire de Sacha");
    assert!(sacha["series_id"].is_string());
    let id = sacha["id"].as_str().unwrap().to_string();
    assert_eq!(
        app.delete(&format!("/api/events/{id}")).await.status,
        StatusCode::NO_CONTENT
    );
    for year in [2026, 2027, 2028] {
        let v = app.get(&format!("/api/events?year={year}")).await.json();
        assert!(
            events(&v)
                .iter()
                .all(|e| e["title"] != "Anniversaire de Sacha"),
            "{year}"
        );
    }
}

#[tokio::test]
async fn editing_a_birthday_adds_age() {
    let mut app = logged_in().await;
    let r = app
        .post("/api/events", json!({"type": "birthday", "title": "Anniversaire de Léa", "start": "2026-05-05", "birth_year": 2020}))
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    assert_eq!(r.json()["age"], 6);
    let v = app.get("/api/events?year=2027").await.json();
    assert_eq!(find(&v, "Anniversaire de Léa")["age"], 7);
}

#[tokio::test]
async fn csrf_header_and_origin_are_enforced() {
    let mut app = logged_in().await;
    let body = json!({"type": "trip", "title": "x", "start": "2026-08-10", "end": "2026-08-12"})
        .to_string();
    let mk = |header: Option<(&str, &str)>| {
        let mut b = Request::post("/api/events").header("content-type", "application/json");
        if let Some((k, v)) = header {
            b = b.header(k, v);
        }
        b
    };
    // No X-Requested-With.
    let r = app
        .send(mk(None).body(Body::from(body.clone())).unwrap())
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Wrong value.
    let r = app
        .send(
            mk(Some(("x-requested-with", "other")))
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Right header, cross-site Origin.
    let r = app
        .send(
            mk(Some(("x-requested-with", "annee")))
                .header("origin", "https://evil.example")
                .header("host", "localhost:8080")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Same origin passes.
    let r = app
        .send(
            mk(Some(("x-requested-with", "annee")))
                .header("origin", "http://localhost:8080")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let r = app
        .send(
            mk(Some(("x-requested-with", "annee")))
                .header("origin", "http://127.0.0.1:4173")
                .header("host", "127.0.0.1:4173")
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
    assert_eq!(
        r.status,
        StatusCode::CREATED,
        "Origin equal to Host is same-origin"
    );
    // Form logout is protected against cross-site origins.
    let r = app
        .send(
            Request::post("/auth/logout")
                .header("origin", "https://evil.example")
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn security_headers_are_set() {
    let mut app = TestApp::demo().await;
    let r = app.get("/healthz").await;
    assert!(
        r.header("content-security-policy")
            .unwrap()
            .contains("default-src 'self'")
    );
    assert!(
        r.header("content-security-policy")
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(r.header("referrer-policy"), Some("same-origin"));
}

#[tokio::test]
async fn demo_reset_restores_sample_data() {
    let mut app = logged_in().await;
    app.post(
        "/api/events",
        json!({"type": "trip", "title": "Temporaire", "start": "2026-08-10", "end": "2026-08-12"}),
    )
    .await;
    assert_eq!(
        app.post("/api/demo/reset", json!({})).await.status,
        StatusCode::NO_CONTENT
    );
    let v = app.get("/api/events?year=2026").await.json();
    assert!(events(&v).iter().all(|e| e["title"] != "Temporaire"));
}

#[tokio::test]
async fn unknown_event_ids_are_404() {
    let mut app = logged_in().await;
    assert_eq!(
        app.patch("/api/events/f:nope", json!({"title": "x"}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.patch("/api/events/garbage", json!({"title": "x"}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.get("/api/nope").await.status, StatusCode::NOT_FOUND);
}

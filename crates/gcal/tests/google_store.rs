//! `GoogleStore` against a mocked Google Calendar API.

use std::sync::Arc;
use std::time::Duration;

use annee_domain::dates::ymd;
use annee_domain::validation::validate_new;
use annee_domain::{EventId, EventInput, EventType, NewEvent, TypeCatalog, ValidationContext};
use annee_gcal::google::auth::{ServiceAccountAuth, ServiceAccountKey, StaticToken, TokenProvider};
use annee_gcal::{
    CalendarStore, DeleteScope, GoogleConfig, GoogleStore, ImportedEvent, StoreError,
};
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REPERES: &str = "reperes-cal";
const FAMILLE: &str = "famille-cal";

fn members() -> Vec<String> {
    ["Alex", "Camille", "Sacha", "Noé"]
        .map(String::from)
        .to_vec()
}

fn store_with(server: &MockServer, ttl: Duration) -> GoogleStore {
    let mut cfg = GoogleConfig::new(
        REPERES.into(),
        FAMILLE.into(),
        chrono_tz::Europe::Paris,
        members(),
    );
    cfg.api_base = server.uri();
    cfg.cache_ttl = ttl;
    let auth: Arc<dyn TokenProvider> = Arc::new(StaticToken("test-token".into()));
    GoogleStore::new(cfg, reqwest::Client::new(), auth)
}

fn store(server: &MockServer) -> GoogleStore {
    store_with(server, Duration::from_secs(60))
}

fn events_path(cal: &str) -> String {
    format!("/calendar/v3/calendars/{cal}/events")
}

fn event_path(cal: &str, id: &str) -> String {
    format!("/calendar/v3/calendars/{cal}/events/{id}")
}

fn new_event(kind: &str, title: &str, start: &str, end: &str) -> NewEvent {
    let m = members();
    validate_new(
        &EventInput {
            kind: Some(kind.into()),
            title: Some(title.into()),
            start: Some(start.into()),
            end: Some(end.into()),
            ..Default::default()
        },
        &ValidationContext {
            members: &m,
            today: ymd(2026, 10, 16),
            catalog: &TypeCatalog::default(),
        },
    )
    .unwrap()
}

fn all_day(id: &str, title: &str, color: Option<&str>, start: &str, end_excl: &str) -> Value {
    let mut v = json!({
        "id": id, "summary": title, "etag": format!("\"etag-{id}\""),
        "start": {"date": start}, "end": {"date": end_excl}
    });
    if let Some(c) = color {
        v["colorId"] = json!(c);
    }
    v
}

#[tokio::test]
async fn list_year_merges_both_calendars_paginates_and_uses_local_year_bounds() {
    let server = MockServer::start().await;
    // Europe/Paris: 1 Jan 2026 00:00 = 31 Dec 2025 23:00Z; 1 Jan 2027 00:00 = 31 Dec 2026 23:00Z.
    Mock::given(method("GET"))
        .and(path(events_path(REPERES)))
        .and(query_param("timeMin", "2025-12-31T23:00:00Z"))
        .and(query_param("timeMax", "2026-12-31T23:00:00Z"))
        .and(query_param("singleEvents", "true"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                all_day("r1", "Toussaint", Some("5"), "2026-10-17", "2026-11-02"),
                {"id": "gone", "status": "cancelled"}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(events_path(FAMILLE)))
        .and(query_param("pageToken", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [all_day("f2", "Ski", Some("9"), "2026-02-21", "2026-03-01")]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(events_path(FAMILLE)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [all_day("f1", "Inconnu", None, "2026-05-01", "2026-05-02")],
            "nextPageToken": "p2"
        })))
        .mount(&server)
        .await;

    let s = store(&server);
    let evs = s.list_year(2026).await.unwrap();
    let titles: Vec<_> = evs.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Ski", "Inconnu", "Toussaint"],
        "sorted by start, cancelled dropped"
    );
    let ski = &evs[0];
    assert_eq!((ski.start, ski.end), (ymd(2026, 2, 21), ymd(2026, 2, 28)));
    assert_eq!(ski.id.to_string(), "f:f2");
    assert_eq!(evs[1].kind, EventType::Unclassified);
    assert_eq!(evs[2].id.to_string(), "r:r1");
    // Second call is served from the cache (the Repères mock expects exactly one request).
    assert_eq!(s.list_year(2026).await.unwrap().len(), 3);
}

#[tokio::test]
async fn cache_expires_and_writes_invalidate_it() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(events_path(REPERES)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(events_path(FAMILLE)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(events_path(FAMILLE)))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "n1",
            "Voyage",
            Some("9"),
            "2026-08-10",
            "2026-08-13",
        )))
        .mount(&server)
        .await;

    let s = store_with(&server, Duration::from_millis(100));
    s.list_year(2026).await.unwrap();
    s.list_year(2026).await.unwrap(); // cached
    tokio::time::sleep(Duration::from_millis(150)).await;
    s.list_year(2026).await.unwrap(); // expired: second fetch
    s.create(
        &new_event("trip", "Voyage", "2026-08-10", "2026-08-12"),
        "Alex",
    )
    .await
    .unwrap();
    s.list_year(2026).await.unwrap(); // invalidated by the write: third fetch
}

#[tokio::test]
async fn create_writes_calendar_color_and_metadata() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(events_path(FAMILLE)))
        .and(body_partial_json(json!({
            "summary": "Lisbonne",
            "colorId": "9",
            "start": {"date": "2026-05-14"},
            "end": {"date": "2026-05-18"},
            "extendedProperties": {"private": {
                "fy_schema": "1", "fy_members": "Alex,Camille", "fy_source": "manual",
                "fy_created_by": "Claude", "fy_updated_by": "Claude"
            }}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "new1", "summary": "Lisbonne", "colorId": "9",
            "start": {"date": "2026-05-14"}, "end": {"date": "2026-05-18"},
            "extendedProperties": {"private": {"fy_members": "Alex,Camille", "fy_created_by": "Claude"}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut n = new_event("trip", "Lisbonne", "2026-05-14", "2026-05-17");
    n.members = vec!["Alex".into(), "Camille".into()];
    let e = store(&server).create(&n, "Claude").await.unwrap();
    assert_eq!(e.id.to_string(), "f:new1");
    assert_eq!(e.end, ymd(2026, 5, 17));
    assert_eq!(e.created_by.as_deref(), Some("Claude"));
}

#[tokio::test]
async fn create_birthday_writes_a_yearly_series_in_reperes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(events_path(REPERES)))
        .and(body_partial_json(json!({
            "colorId": "2",
            "recurrence": ["RRULE:FREQ=YEARLY"],
            "start": {"date": "2018-03-12"},
            "end": {"date": "2018-03-13"},
            "extendedProperties": {"private": {"fy_birth_year": "2018"}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "bd1", "summary": "Anniversaire de Sacha", "colorId": "2",
            "recurrence": ["RRULE:FREQ=YEARLY"],
            "start": {"date": "2018-03-12"}, "end": {"date": "2018-03-13"},
            "extendedProperties": {"private": {"fy_birth_year": "2018"}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut n = new_event(
        "birthday",
        "Anniversaire de Sacha",
        "2026-03-12",
        "2026-03-12",
    );
    n.birth_year = Some(2018);
    let e = store(&server).create(&n, "Alex").await.unwrap();
    assert_eq!(e.id.to_string(), "r:bd1_20260312");
    assert_eq!(e.series_id.unwrap().to_string(), "r:bd1");
    assert_eq!(e.start, ymd(2026, 3, 12));
    assert_eq!(e.age, Some(8));
}

#[tokio::test]
async fn update_patches_only_changed_fields_with_if_match() {
    let server = MockServer::start().await;
    let mut existing = all_day("t1", "Lisbonne", Some("9"), "2026-05-14", "2026-05-18");
    existing["extendedProperties"] = json!({"private": {"fy_created_by": "Alex"}});
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(existing))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(FAMILLE, "t1")))
        .and(header("if-match", "\"etag-t1\""))
        .and(body_partial_json(json!({
            "summary": "Lisbonne en famille",
            "extendedProperties": {"private": {"fy_updated_by": "Camille", "fy_schema": "1"}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "Lisbonne en famille",
            Some("9"),
            "2026-05-14",
            "2026-05-18",
        )))
        .expect(1)
        .mount(&server)
        .await;
    let n = new_event("trip", "Lisbonne en famille", "2026-05-14", "2026-05-17");
    let e = store(&server)
        .update(&"f:t1".parse().unwrap(), &n, "Camille")
        .await
        .unwrap();
    assert_eq!(e.title, "Lisbonne en famille");

    // The patch body carries no dates, colour or location because none of them changed.
    let reqs = server.received_requests().await.unwrap();
    let patch = reqs.iter().find(|r| r.method.as_str() == "PATCH").unwrap();
    let body: Value = serde_json::from_slice(&patch.body).unwrap();
    for k in [
        "start",
        "end",
        "colorId",
        "location",
        "description",
        "recurrence",
    ] {
        assert!(body.get(k).is_none(), "{k} should not be patched: {body}");
    }
}

#[tokio::test]
async fn update_dates_and_clear_optional_fields() {
    let server = MockServer::start().await;
    let mut existing = all_day("t1", "Lisbonne", Some("9"), "2026-05-14", "2026-05-18");
    existing["location"] = json!("Portugal");
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(existing))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(FAMILLE, "t1")))
        .and(body_partial_json(json!({
            "start": {"date": "2026-05-15", "dateTime": null, "timeZone": null},
            "end": {"date": "2026-05-19", "dateTime": null, "timeZone": null},
            "location": null
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "Lisbonne",
            Some("9"),
            "2026-05-15",
            "2026-05-19",
        )))
        .expect(1)
        .mount(&server)
        .await;
    let n = new_event("trip", "Lisbonne", "2026-05-15", "2026-05-18");
    let e = store(&server)
        .update(&"f:t1".parse().unwrap(), &n, "Camille")
        .await
        .unwrap();
    assert_eq!((e.start, e.end), (ymd(2026, 5, 15), ymd(2026, 5, 18)));
}

#[tokio::test]
async fn changing_to_a_type_of_the_other_calendar_moves_then_patches_the_color() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "Crèche",
            Some("9"),
            "2026-08-03",
            "2026-08-05",
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{}/move", event_path(FAMILLE, "t1"))))
        .and(query_param("destination", REPERES))
        .and(header("content-length", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json({
            let mut v = all_day("t1", "Crèche", Some("9"), "2026-08-03", "2026-08-05");
            v["etag"] = json!("\"etag-moved\"");
            v
        }))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(REPERES, "t1")))
        .and(header("if-match", "\"etag-moved\""))
        .and(body_partial_json(json!({"colorId": "4"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "Crèche",
            Some("4"),
            "2026-08-03",
            "2026-08-05",
        )))
        .expect(1)
        .mount(&server)
        .await;
    let n = new_event("daycare_closure", "Crèche", "2026-08-03", "2026-08-04");
    let e = store(&server)
        .update(&"f:t1".parse().unwrap(), &n, "Alex")
        .await
        .unwrap();
    assert_eq!(e.kind, EventType::DaycareClosure);
    assert_eq!(e.id.to_string(), "r:t1");
}

#[tokio::test]
async fn precondition_failed_is_retried_once() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "Lisbonne",
            Some("9"),
            "2026-05-14",
            "2026-05-18",
        )))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(412))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "Autre",
            Some("9"),
            "2026-05-14",
            "2026-05-18",
        )))
        .mount(&server)
        .await;
    let n = new_event("trip", "Autre", "2026-05-14", "2026-05-17");
    let e = store(&server)
        .update(&"f:t1".parse().unwrap(), &n, "Alex")
        .await
        .unwrap();
    assert_eq!(e.title, "Autre");
}

#[tokio::test]
async fn persistent_precondition_failure_is_a_conflict() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "Lisbonne",
            Some("9"),
            "2026-05-14",
            "2026-05-18",
        )))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(412))
        .mount(&server)
        .await;
    let n = new_event("trip", "Autre", "2026-05-14", "2026-05-17");
    let err = store(&server)
        .update(&"f:t1".parse().unwrap(), &n, "Alex")
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::Conflict));
    assert_eq!(
        err.to_string(),
        "Cet événement a été modifié entre-temps. Rechargez la page."
    );
}

#[tokio::test]
async fn editing_an_occurrence_patches_the_series_master() {
    let server = MockServer::start().await;
    let instance = json!({
        "id": "bd1_20270312", "summary": "Anniversaire de Sacha", "colorId": "2", "recurringEventId": "bd1",
        "start": {"date": "2027-03-12"}, "end": {"date": "2027-03-13"},
        "extendedProperties": {"private": {"fy_birth_year": "2018"}}
    });
    let master = json!({
        "id": "bd1", "summary": "Anniversaire de Sacha", "colorId": "2", "etag": "\"etag-master\"",
        "recurrence": ["RRULE:FREQ=YEARLY"],
        "start": {"date": "2018-03-12"}, "end": {"date": "2018-03-13"},
        "extendedProperties": {"private": {"fy_birth_year": "2018"}}
    });
    Mock::given(method("GET"))
        .and(path(event_path(REPERES, "bd1_20270312")))
        .respond_with(ResponseTemplate::new(200).set_body_json(instance.clone()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(event_path(REPERES, "bd1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(master.clone()))
        .mount(&server)
        .await;
    // Move the birthday by one day: the series starts on 13 March 2018.
    Mock::given(method("PATCH"))
        .and(path(event_path(REPERES, "bd1")))
        .and(header("if-match", "\"etag-master\""))
        .and(body_partial_json(json!({
            "start": {"date": "2018-03-13"}, "end": {"date": "2018-03-14"},
            "recurrence": ["RRULE:FREQ=YEARLY"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json({
            let mut m = master.clone();
            m["start"] = json!({"date": "2018-03-13"});
            m["end"] = json!({"date": "2018-03-14"});
            m
        }))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path(event_path(REPERES, "bd1_20270313")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "bd1_20270313", "summary": "Anniversaire de Sacha", "colorId": "2", "recurringEventId": "bd1",
            "start": {"date": "2027-03-13"}, "end": {"date": "2027-03-14"},
            "extendedProperties": {"private": {"fy_birth_year": "2018"}}
        })))
        .mount(&server).await;

    let mut n = new_event(
        "birthday",
        "Anniversaire de Sacha",
        "2027-03-13",
        "2027-03-13",
    );
    n.birth_year = Some(2018);
    let e = store(&server)
        .update(&"r:bd1_20270312".parse().unwrap(), &n, "Alex")
        .await
        .unwrap();
    assert_eq!(e.start, ymd(2027, 3, 13));
    assert_eq!(e.age, Some(9));
}

#[tokio::test]
async fn deleting_an_occurrence_deletes_the_series() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(event_path(REPERES, "bd1_20270312")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "bd1_20270312", "recurringEventId": "bd1",
            "start": {"date": "2027-03-12"}, "end": {"date": "2027-03-13"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(event_path(REPERES, "bd1")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let scope = store(&server)
        .delete(&"r:bd1_20270312".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(scope, DeleteScope::Series);
}

#[tokio::test]
async fn deleting_a_single_event() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
            "t1",
            "x",
            Some("9"),
            "2026-05-14",
            "2026-05-15",
        )))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(event_path(FAMILLE, "t1")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        store(&server)
            .delete(&"f:t1".parse().unwrap())
            .await
            .unwrap(),
        DeleteScope::Event
    );
}

#[tokio::test]
async fn http_errors_are_mapped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "missing")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(event_path(FAMILLE, "boom")))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal"))
        .mount(&server)
        .await;
    let s = store(&server);
    let id: EventId = "f:missing".parse().unwrap();
    assert!(matches!(s.get(&id).await, Err(StoreError::NotFound)));
    let id: EventId = "f:boom".parse().unwrap();
    let err = s.get(&id).await.unwrap_err();
    assert!(matches!(err, StoreError::Upstream(_)));
    assert_eq!(
        err.to_string(),
        "Google Agenda ne répond pas. Réessayez dans un instant."
    );
}

#[tokio::test]
async fn timed_events_are_read_with_their_times() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(events_path(REPERES)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path(events_path(FAMILLE)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [{
            "id": "tm", "summary": "Soirée", "colorId": "3",
            "start": {"dateTime": "2026-03-14T15:00:00+01:00"}, "end": {"dateTime": "2026-03-14T23:00:00+01:00"}
        }]}))).mount(&server).await;
    let evs = store(&server).list_year(2026).await.unwrap();
    assert_eq!(evs[0].start_time.unwrap().to_string(), "15:00:00");
    assert_eq!(evs[0].end, ymd(2026, 3, 14));
}

#[tokio::test]
async fn imported_events_use_the_private_property_filter() {
    let server = MockServer::start().await;
    for (src, id) in [("import:education", "e1"), ("import:public_holidays", "h1")] {
        Mock::given(method("GET")).and(path(events_path(REPERES)))
            .and(query_param("privateExtendedProperty", format!("fy_source={src}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [{
                "id": id, "summary": "x", "colorId": "5",
                "start": {"date": "2026-10-17"}, "end": {"date": "2026-11-02"},
                "extendedProperties": {"private": {"fy_source": src, "fy_source_id": format!("id-{id}")}}
            }]})))
            .expect(1).mount(&server).await;
    }
    let evs = store(&server)
        .list_imported(ymd(2026, 1, 1), ymd(2026, 12, 31))
        .await
        .unwrap();
    assert_eq!(evs.len(), 2);
    assert!(evs.iter().all(|e| e.read_only && e.source_id.is_some()));
}

#[tokio::test]
async fn imported_events_are_created_patched_and_deleted() {
    let server = MockServer::start().await;
    let spec = ImportedEvent {
        kind: EventType::SchoolHoliday,
        title: "Vacances de la Toussaint".into(),
        start: ymd(2025, 10, 18),
        end: ymd(2025, 11, 2),
        source: annee_domain::Source::ImportEducation,
        source_id: "edu:2025-2026:vacances-de-la-toussaint:2025-10-18".into(),
    };
    Mock::given(method("POST"))
        .and(path(events_path(REPERES)))
        .and(body_partial_json(json!({
            "colorId": "5",
            "start": {"date": "2025-10-18"}, "end": {"date": "2025-11-03"},
            "extendedProperties": {"private": {
                "fy_source": "import:education",
                "fy_source_id": "edu:2025-2026:vacances-de-la-toussaint:2025-10-18"
            }}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "new"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(REPERES, "e1")))
        .and(body_partial_json(json!({"end": {"date": "2025-11-03"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "e1"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(event_path(REPERES, "e2")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let s = store(&server);
    s.create_imported(&spec).await.unwrap();
    s.update_imported(&"r:e1".parse().unwrap(), &spec)
        .await
        .unwrap();
    s.delete_imported(&"r:e2".parse().unwrap()).await.unwrap();
}

#[tokio::test]
async fn service_account_auth_exchanges_a_signed_assertion_and_caches_the_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "ya29.abc", "expires_in": 3600})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (private_pem, public_pem) = generate_pem();
    let key = ServiceAccountKey {
        client_email: "annee@example.iam.gserviceaccount.com".into(),
        private_key: private_pem.clone(),
        token_uri: format!("{}/token", server.uri()),
    };
    let auth = ServiceAccountAuth::new(key, reqwest::Client::new()).unwrap();
    assert_eq!(auth.access_token().await.unwrap(), "ya29.abc");
    assert_eq!(auth.access_token().await.unwrap(), "ya29.abc");

    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1);
    let form: Vec<(String, String)> = url_form(&reqs[0].body);
    let get = |k: &str| form.iter().find(|(n, _)| n == k).unwrap().1.clone();
    assert_eq!(
        get("grant_type"),
        "urn:ietf:params:oauth:grant-type:jwt-bearer"
    );
    // Verify the assertion with the public key.
    let mut v = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    v.set_audience(&[format!("{}/token", server.uri())]);
    let data = jsonwebtoken::decode::<Value>(
        &get("assertion"),
        &jsonwebtoken::DecodingKey::from_rsa_pem(public_pem.as_bytes()).unwrap(),
        &v,
    )
    .unwrap();
    assert_eq!(data.claims["iss"], "annee@example.iam.gserviceaccount.com");
    assert_eq!(
        data.claims["scope"],
        "https://www.googleapis.com/auth/calendar"
    );
}

fn url_form(body: &[u8]) -> Vec<(String, String)> {
    String::from_utf8_lossy(body)
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (dec(k), dec(v)))
        .collect()
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

/// Throwaway RSA keys, generated per test run so that no private key is committed.
fn generate_pem() -> (String, String) {
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
    let key = rsa::RsaPrivateKey::new(&mut rand08::rngs::OsRng, 2048).unwrap();
    (
        key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string(),
        key.to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .unwrap(),
    )
}

#[tokio::test]
async fn count_colors_counts_series_masters_once_across_all_years() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(events_path(REPERES)))
        .and(query_param("singleEvents", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [
            all_day("bd1", "Anniversaire", Some("2"), "2018-03-12", "2018-03-13"),
            all_day("r1", "Toussaint", Some("5"), "2026-10-17", "2026-11-02"),
            all_day("r2", "Noël", Some("5"), "2026-12-19", "2027-01-04"),
            {"id": "gone", "status": "cancelled", "colorId": "5"}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(events_path(FAMILLE)))
        .and(query_param("singleEvents", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [
            all_day("f1", "Ski", Some("9"), "2026-02-21", "2026-03-01"),
            all_day("f2", "Sans couleur", None, "2026-05-01", "2026-05-02")
        ]})))
        .mount(&server)
        .await;
    let counts = store(&server).count_colors().await.unwrap();
    let expected: std::collections::BTreeMap<String, usize> = [("2", 1), ("5", 2), ("9", 1)]
        .map(|(k, v)| (k.to_string(), v))
        .into();
    assert_eq!(counts, expected);
}

#[tokio::test]
async fn recolor_applies_a_swap_in_one_pass_and_skips_migrated_events() {
    let server = MockServer::start().await;
    let tagged = |id: &str, color: &str| {
        let mut v = all_day(id, "x", Some(color), "2026-05-01", "2026-05-02");
        v["extendedProperties"] = json!({"private": {"fy_migration": "m1"}});
        v
    };
    Mock::given(method("GET"))
        .and(path(events_path(FAMILLE)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [
            all_day("a", "Ski", Some("9"), "2026-02-21", "2026-03-01"),
            all_day("b", "Réunion", Some("11"), "2026-04-01", "2026-04-02"),
            all_day("c", "Sport", Some("6"), "2026-04-03", "2026-04-04"),
            all_day("d", "Inchangé", Some("3"), "2026-04-05", "2026-04-06"),
            tagged("e", "11")
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(events_path(REPERES)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .mount(&server)
        .await;
    for (id, color) in [("a", json!("11")), ("b", json!("9")), ("c", Value::Null)] {
        Mock::given(method("PATCH"))
            .and(path(event_path(FAMILLE, id)))
            .and(body_partial_json(json!({
                "colorId": color,
                "extendedProperties": {"private": {"fy_migration": "m1"}}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(all_day(
                id,
                "x",
                None,
                "2026-01-01",
                "2026-01-02",
            )))
            .expect(1)
            .mount(&server)
            .await;
    }
    let mapping = [
        ("9".to_string(), Some("11".to_string())),
        ("11".to_string(), Some("9".to_string())),
        ("6".to_string(), None),
    ]
    .into();
    let n = store(&server)
        .recolor("m1", &mapping, &TypeCatalog::default())
        .await
        .unwrap();
    assert_eq!(
        (n.patched, n.skipped.len()),
        (3, 0),
        "d is not in the mapping, e already carries the migration id"
    );
}

#[tokio::test]
async fn a_listing_that_overlaps_an_invalidation_is_not_cached() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(events_path(REPERES)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(events_path(FAMILLE)))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"items": []}))
                .set_delay(Duration::from_millis(300)),
        )
        .expect(2)
        .mount(&server)
        .await;
    let s = Arc::new(store(&server));
    let first = tokio::spawn({
        let s = s.clone();
        async move { s.list_year(2026).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    // The catalog changes while the listing is in flight.
    s.invalidate_cache();
    first.await.unwrap().unwrap();
    // The stale listing was served but not kept: the next call fetches again (both mocks expect 2).
    s.list_year(2026).await.unwrap();
}

#[tokio::test]
async fn recolor_skips_an_event_google_refuses_for_good_but_not_a_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(events_path(FAMILLE)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [
            all_day("ok", "A", Some("9"), "2026-02-21", "2026-03-01"),
            all_day("locked", "B", Some("9"), "2026-04-01", "2026-04-02"),
            all_day("gone", "C", Some("9"), "2026-04-03", "2026-04-04"),
            all_day("last", "D", Some("9"), "2026-04-05", "2026-04-06")
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(events_path(REPERES)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .mount(&server)
        .await;
    let patched = || {
        ResponseTemplate::new(200).set_body_json(all_day(
            "x",
            "x",
            None,
            "2026-01-01",
            "2026-01-02",
        ))
    };
    for id in ["ok", "last"] {
        Mock::given(method("PATCH"))
            .and(path(event_path(FAMILLE, id)))
            .respond_with(patched())
            .expect(1)
            .mount(&server)
            .await;
    }
    // Not writable by the service account: skipped. Deleted meanwhile: skipped.
    Mock::given(method("PATCH"))
        .and(path(event_path(FAMILLE, "locked")))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(
                json!({"error": {"errors": [{"reason": "forbiddenForNonOrganizer"}]}}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(event_path(FAMILLE, "gone")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let mapping = [("9".to_string(), Some("6".to_string()))].into();
    let report = store(&server)
        .recolor("m", &mapping, &TypeCatalog::default())
        .await
        .unwrap();
    assert_eq!((report.patched, report.skipped.len()), (2, 2));
    let mut ids = report.skipped.clone();
    ids.sort();
    assert_eq!(ids, ["gone", "locked"]);

    // A 403 that is a quota is transient: the pass fails, to be retried.
    for reason in [
        "rateLimitExceeded",
        "dailyLimitExceeded",
        "calendarUsageLimitsExceeded",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(events_path(FAMILLE)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [
                all_day("a", "A", Some("9"), "2026-02-21", "2026-03-01")
            ]})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(events_path(REPERES)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
            .mount(&server)
            .await;
        Mock::given(method("PATCH"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_json(json!({"error": {"errors": [{"reason": reason}]}})),
            )
            .mount(&server)
            .await;
        let err = store(&server)
            .recolor("m", &mapping, &TypeCatalog::default())
            .await;
        assert!(matches!(err, Err(StoreError::Upstream(_))));
    }
}

//! Repères import job (§6): fixtures, idempotency, deletion, failure handling, scheduling rules.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use annee_domain::dates::ymd;
use annee_domain::{EventType, Source};
use annee_gcal::{CalendarStore, DemoStore, ImportedEvent};
use annee_server::config::ImportsSection;
use annee_server::imports::education::{
    EduError, EduRecord, EducationSource, FixtureEducation, HttpEducation, parse_page,
};
use annee_server::imports::{ImportService, Importer};
use annee_server::storage::Db;
use async_trait::async_trait;
use axum::http::StatusCode;
use chrono::NaiveDate;
use common::TestApp;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn today() -> NaiveDate {
    ymd(2026, 10, 16)
}

fn store() -> Arc<DemoStore> {
    Arc::new(DemoStore::new(
        chrono_tz::Europe::Paris,
        ["Alex", "Camille", "Sacha", "Noé"]
            .map(String::from)
            .to_vec(),
    ))
}

fn importer(store: Arc<DemoStore>, education: Arc<dyn EducationSource>) -> Importer {
    Importer::new(
        store,
        education,
        ImportsSection::default(),
        chrono_tz::Europe::Paris,
    )
}

async fn imported(store: &DemoStore) -> Vec<annee_domain::Event> {
    store
        .list_imported(ymd(2024, 1, 1), ymd(2030, 12, 31))
        .await
        .unwrap()
}

#[tokio::test]
async fn the_fixture_produces_the_expected_school_holidays() {
    let store = store();
    let imp = importer(store.clone(), Arc::new(FixtureEducation));
    let (counts, _) = imp.run(today(), false).await.unwrap();
    assert_eq!(
        counts.created, 6,
        "2026-2027 holidays; 2027-2028 is not published in the fixture"
    );
    let events = imported(&store).await;
    let school: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventType::SchoolHoliday)
        .collect();
    assert_eq!(school.len(), 6);
    let toussaint = school
        .iter()
        .find(|e| e.title == "Vacances de la Toussaint")
        .unwrap();
    assert_eq!(
        (toussaint.start, toussaint.end),
        (ymd(2026, 10, 17), ymd(2026, 11, 1))
    );
    assert_eq!(toussaint.source, Source::ImportEducation);
    assert!(toussaint.read_only);
    // Public holidays: current year + 2 following.
    let ph: Vec<_> = events
        .iter()
        .filter(|e| {
            e.kind == EventType::PublicHoliday
                && (2026..=2028).contains(&chrono::Datelike::year(&e.start))
        })
        .collect();
    assert_eq!(ph.len(), 33);
    assert!(
        ph.iter()
            .any(|e| e.start == ymd(2028, 12, 25) && e.title == "Noël")
    );
}

#[tokio::test]
async fn acceptance_case_toussaint_2025() {
    // With "today" in school year 2025-2026 the Toussaint 2025 import is 18 October → 2 November.
    let store = store();
    let imp = importer(store.clone(), Arc::new(FixtureEducation));
    imp.run(ymd(2025, 10, 1), false).await.unwrap();
    let e = imported(&store)
        .await
        .into_iter()
        .find(|e| e.title == "Vacances de la Toussaint" && e.start.year_is(2025))
        .unwrap();
    assert_eq!((e.start, e.end), (ymd(2025, 10, 18), ymd(2025, 11, 2)));
}

trait YearIs {
    fn year_is(&self, y: i32) -> bool;
}
impl YearIs for NaiveDate {
    fn year_is(&self, y: i32) -> bool {
        chrono::Datelike::year(self) == y
    }
}

#[tokio::test]
async fn second_run_changes_nothing() {
    let store = store();
    let imp = importer(store.clone(), Arc::new(FixtureEducation));
    imp.run(today(), false).await.unwrap();
    let (counts, lines) = imp.run(today(), false).await.unwrap();
    assert_eq!((counts.created, counts.updated, counts.deleted), (0, 0, 0));
    assert_eq!(counts.unchanged, 39);
    assert!(lines.is_empty());
}

#[tokio::test]
async fn dry_run_writes_nothing() {
    let store = store();
    let imp = importer(store.clone(), Arc::new(FixtureEducation));
    let (counts, lines) = imp.run(today(), true).await.unwrap();
    assert_eq!(counts.created, 6);
    assert_eq!(lines.len(), 6);
    assert!(
        imported(&store)
            .await
            .iter()
            .all(|e| e.kind != EventType::SchoolHoliday)
    );
}

/// An education source serving fixed records.
struct Fixed(Vec<EduRecord>);

#[async_trait]
impl EducationSource for Fixed {
    async fn fetch(&self, school_year: &str, _: &str) -> Result<Vec<EduRecord>, EduError> {
        Ok(self
            .0
            .iter()
            .filter(|r| r.annee_scolaire.as_deref() == Some(school_year))
            .cloned()
            .collect())
    }
}

fn records_2026_2027() -> Vec<EduRecord> {
    parse_page(include_str!("fixtures/education_2026-2027.json")).unwrap()
}

#[tokio::test]
async fn changed_titles_and_dates_are_updated_and_removed_holidays_deleted() {
    let store = store();
    importer(store.clone(), Arc::new(Fixed(records_2026_2027())))
        .run(today(), false)
        .await
        .unwrap();

    let mut changed = records_2026_2027();
    changed.retain(|r| r.description.as_deref() != Some("Vacances d'Hiver")); // removed upstream
    for r in &mut changed {
        if r.description.as_deref() == Some("Vacances de Printemps") {
            r.end_date = Some("2027-05-03T22:00:00+00:00".into()); // one day longer
        }
    }
    let (counts, _) = importer(store.clone(), Arc::new(Fixed(changed)))
        .run(today(), false)
        .await
        .unwrap();
    assert_eq!((counts.created, counts.updated, counts.deleted), (0, 1, 1));
    let events = imported(&store).await;
    assert!(events.iter().all(|e| e.title != "Vacances d’hiver"));
    let printemps = events
        .iter()
        .find(|e| e.title == "Vacances de printemps")
        .unwrap();
    assert_eq!(printemps.end, ymd(2027, 5, 3));
}

#[tokio::test]
async fn obsolete_imported_events_of_managed_years_are_deleted_but_other_years_are_kept() {
    let store = store();
    let mk = |sid: &str, start: NaiveDate| ImportedEvent {
        kind: EventType::SchoolHoliday,
        title: "Ancienne".into(),
        start,
        end: start,
        source: Source::ImportEducation,
        source_id: sid.into(),
    };
    store
        .create_imported(&mk("edu:2026-2027:ancienne:2026-11-11", ymd(2026, 11, 11)))
        .await
        .unwrap();
    store
        .create_imported(&mk("edu:2024-2025:historique:2025-02-10", ymd(2025, 2, 10)))
        .await
        .unwrap();
    let (counts, _) = importer(store.clone(), Arc::new(Fixed(records_2026_2027())))
        .run(today(), false)
        .await
        .unwrap();
    assert_eq!(counts.deleted, 1);
    let titles: Vec<_> = imported(&store)
        .await
        .into_iter()
        .map(|e| e.title)
        .collect();
    assert_eq!(
        titles.iter().filter(|t| *t == "Ancienne").count(),
        1,
        "the 2024-2025 event is kept, the 2026-2027 one deleted"
    );
}

#[tokio::test]
async fn unpublished_school_year_does_not_delete_what_exists() {
    let store = store();
    importer(store.clone(), Arc::new(Fixed(records_2026_2027())))
        .run(today(), false)
        .await
        .unwrap();
    // Dataset now empty (e.g. temporarily unpublished): nothing school-related is deleted.
    let (counts, _) = importer(store.clone(), Arc::new(Fixed(vec![])))
        .run(today(), false)
        .await
        .unwrap();
    assert_eq!(counts.deleted, 0);
    assert_eq!(
        imported(&store)
            .await
            .iter()
            .filter(|e| e.kind == EventType::SchoolHoliday)
            .count(),
        6
    );
}

#[tokio::test]
async fn manual_events_are_never_touched() {
    let store = store();
    let m = ["Alex", "Camille", "Sacha", "Noé"]
        .map(String::from)
        .to_vec();
    let new = annee_domain::validation::validate_new(
        &annee_domain::EventInput {
            kind: Some("trip".into()),
            title: Some("Vacances de la Toussaint (chez nous)".into()),
            start: Some("2026-10-17".into()),
            end: Some("2026-11-01".into()),
            ..Default::default()
        },
        &annee_domain::ValidationContext {
            members: &m,
            today: today(),
            catalog: &annee_domain::TypeCatalog::default(),
        },
    )
    .unwrap();
    store.create(&new, "Alex").await.unwrap();
    let imp = importer(store.clone(), Arc::new(Fixed(records_2026_2027())));
    imp.run(today(), false).await.unwrap();
    imp.run(today(), false).await.unwrap();
    let year = store.list_year(2026).await.unwrap();
    assert_eq!(
        year.iter()
            .filter(|e| e.title == "Vacances de la Toussaint (chez nous)")
            .count(),
        1
    );
    assert!(
        year.iter()
            .any(|e| e.source_id.as_deref()
                == Some("edu:2026-2027:vacances-de-la-toussaint:2026-10-17")),
        "the import added its own event next to the manual one"
    );
}

// ---- HTTP source -------------------------------------------------------------------------

fn http_source(server: &MockServer) -> HttpEducation {
    HttpEducation::new(reqwest::Client::new())
        .with_base_url(server.uri())
        .with_backoff(vec![Duration::from_millis(5), Duration::from_millis(5)])
}

const DATASET: &str = "/api/explore/v2.1/catalog/datasets/fr-en-calendrier-scolaire/records";

#[tokio::test]
async fn http_source_filters_on_location_and_school_year() {
    let server = MockServer::start().await;
    let page: Value =
        serde_json::from_str(include_str!("fixtures/education_2025-2026.json")).unwrap();
    Mock::given(method("GET"))
        .and(path(DATASET))
        .and(query_param(
            "where",
            "location=\"Rennes\" and annee_scolaire=\"2025-2026\"",
        ))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        .expect(1)
        .mount(&server)
        .await;
    let records = http_source(&server)
        .fetch("2025-2026", "Rennes")
        .await
        .unwrap();
    assert_eq!(records.len(), 7);
}

#[tokio::test]
async fn http_source_follows_pagination() {
    let server = MockServer::start().await;
    let rec = |n: u32| json!({"description": format!("R{n}"), "zones": "Zone B", "start_date": "2026-01-01T00:00:00+00:00"});
    Mock::given(method("GET"))
        .and(path(DATASET))
        .and(query_param("offset", "0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count": 3, "results": [rec(1), rec(2)]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(DATASET))
        .and(query_param("offset", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count": 3, "results": [rec(3)]})),
        )
        .mount(&server)
        .await;
    assert_eq!(
        http_source(&server)
            .fetch("2025-2026", "Rennes")
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn http_source_retries_three_times_then_fails() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DATASET))
        .respond_with(ResponseTemplate::new(500))
        .expect(3)
        .mount(&server)
        .await;
    let err = http_source(&server)
        .fetch("2025-2026", "Rennes")
        .await
        .unwrap_err();
    assert!(err.0.contains("500"), "{err}");
}

#[tokio::test]
async fn http_source_recovers_after_a_transient_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DATASET))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(DATASET))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"total_count": 0, "results": []})),
        )
        .mount(&server)
        .await;
    assert!(
        http_source(&server)
            .fetch("2026-2027", "Rennes")
            .await
            .unwrap()
            .is_empty()
    );
}

// ---- Service: recording, single run at a time, failures ------------------------------------

async fn service(store: Arc<DemoStore>, education: Arc<dyn EducationSource>) -> Arc<ImportService> {
    let db = Db::memory().await.unwrap();
    ImportService::new(
        importer(store, education),
        db,
        Arc::new(annee_gcal::TypeRegistry::default()),
        Arc::new(today),
    )
}

#[tokio::test]
async fn a_run_is_recorded_with_counts() {
    let svc = service(store(), Arc::new(FixtureEducation)).await;
    let run = svc.run("manual").await.unwrap();
    assert_eq!(run.status, "success");
    assert_eq!((run.created, run.deleted), (Some(6), Some(0)));
    assert_eq!(run.trigger.as_deref(), Some("manual"));
    assert!(run.finished_at.is_some());
    let again = svc.run("schedule").await.unwrap();
    assert_eq!(
        (again.created, again.updated, again.deleted),
        (Some(0), Some(0), Some(0))
    );
    let runs = svc.recent_runs(10).await.unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].id, again.id, "most recent first");
    assert!(svc.last_success().await.unwrap().is_some());
}

struct Failing;

#[async_trait]
impl EducationSource for Failing {
    async fn fetch(&self, _: &str, _: &str) -> Result<Vec<EduRecord>, EduError> {
        Err(EduError("data.education.gouv.fr answered 500".into()))
    }
}

#[tokio::test]
async fn failure_keeps_existing_events_and_is_recorded() {
    let store = store();
    importer(store.clone(), Arc::new(FixtureEducation))
        .run(today(), false)
        .await
        .unwrap();
    let before = imported(&store).await.len();
    let svc = service(store.clone(), Arc::new(Failing)).await;
    let run = svc.run("manual").await.unwrap();
    assert_eq!(run.status, "failed");
    assert!(run.error.unwrap().contains("500"));
    assert_eq!(
        imported(&store).await.len(),
        before,
        "existing events are kept"
    );
    assert!(svc.last_success().await.unwrap().is_none());
}

struct Slow(Arc<AtomicUsize>);

#[async_trait]
impl EducationSource for Slow {
    async fn fetch(&self, _: &str, _: &str) -> Result<Vec<EduRecord>, EduError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(vec![])
    }
}

#[tokio::test]
async fn only_one_run_at_a_time() {
    let calls = Arc::new(AtomicUsize::new(0));
    let svc = service(store(), Arc::new(Slow(calls.clone()))).await;
    assert!(svc.spawn("manual"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(svc.is_running());
    assert!(
        !svc.spawn("manual"),
        "a second request while running does not start another run"
    );
    assert!(svc.run("cli").await.is_none());
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!svc.is_running());
    assert_eq!(svc.recent_runs(10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn interrupted_runs_are_marked_failed_at_startup() {
    let svc = service(store(), Arc::new(FixtureEducation)).await;
    sqlx::query("INSERT INTO import_runs (started_at, status, trigger) VALUES ('2026-01-01T00:00:00Z', 'running', 'manual')")
        .execute(&Db::memory().await.unwrap().pool)
        .await
        .unwrap();
    // (a separate in-memory db) — exercise the real path on the service's own database:
    sqlx::query("INSERT INTO import_runs (started_at, status, trigger) VALUES ('2026-01-01T00:00:00Z', 'running', 'manual')")
        .execute(&svc_pool(&svc))
        .await
        .unwrap();
    svc.recover_interrupted().await.unwrap();
    let runs = svc.recent_runs(5).await.unwrap();
    assert_eq!(runs[0].status, "failed");
    assert!(runs[0].error.as_deref().unwrap().contains("Interrompu"));
}

fn svc_pool(svc: &ImportService) -> sqlx::SqlitePool {
    svc.pool().clone()
}

#[tokio::test]
async fn startup_catch_up_runs_only_when_stale() {
    let svc = service(store(), Arc::new(FixtureEducation)).await;
    svc.run_if_stale().await; // never ran → starts
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(svc.recent_runs(10).await.unwrap().len(), 1);
    svc.run_if_stale().await; // fresh success → nothing
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(svc.recent_runs(10).await.unwrap().len(), 1);
    // A success older than 31 days is stale.
    sqlx::query("UPDATE import_runs SET finished_at='2020-01-01T00:00:00Z'")
        .execute(svc.pool())
        .await
        .unwrap();
    svc.run_if_stale().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(svc.recent_runs(10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn scheduler_accepts_the_default_cron_in_paris_time() {
    let svc = service(store(), Arc::new(FixtureEducation)).await;
    let mut sched = svc
        .start_scheduler("0 0 4 1 * *", chrono_tz::Europe::Paris)
        .await
        .unwrap();
    sched.shutdown().await.unwrap();
    assert!(
        svc.start_scheduler("not a cron", chrono_tz::Europe::Paris)
            .await
            .is_err()
    );
}

// ---- Admin API -------------------------------------------------------------------------------

#[tokio::test]
async fn admin_import_endpoints() {
    let mut app = TestApp::demo().await;
    assert_eq!(
        app.get("/api/admin/import").await.status,
        StatusCode::UNAUTHORIZED
    );
    app.login().await;
    let status = app.get("/api/admin/import").await.json();
    assert_eq!(status, json!({"running": false, "runs": []}));

    let r = app
        .api(axum::http::Method::POST, "/api/admin/import", None)
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    assert_eq!(r.json(), json!({"running": true}));
    let mut last = Value::Null;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        last = app.get("/api/admin/import").await.json();
        if last["running"] == false && !last["runs"].as_array().unwrap().is_empty() {
            break;
        }
    }
    let run = &last["runs"][0];
    assert_eq!(run["status"], "success", "{last}");
    assert_eq!(run["trigger"], "manual");
    assert_eq!(run["created"], 6);
    assert!(run["started_at"].is_string() && run["finished_at"].is_string());

    // Imported events are now visible, read-only, next to the demo sample data.
    let v = app.get("/api/events?year=2027").await.json();
    let hiver = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == "Vacances d’hiver")
        .unwrap();
    assert_eq!(hiver["read_only"], true);
    assert_eq!(hiver["start"], "2027-02-20");
}

// ---- Type migrations (SPEC §19.5) ------------------------------------------------------------

#[tokio::test]
async fn events_imported_with_a_stale_color_are_healed_by_the_next_run() {
    use annee_domain::{EventType, Source};
    let store = store();
    let imp = importer(store.clone(), Arc::new(FixtureEducation));
    imp.run(today(), false).await.unwrap();
    // A write that raced a color migration leaves a holiday typed as unclassified.
    let victim = imported(&store).await[0].clone();
    store
        .update_imported(
            &victim.id,
            &annee_gcal::ImportedEvent {
                kind: EventType::Unclassified,
                title: victim.title.clone(),
                start: victim.start,
                end: victim.end,
                source: Source::ImportPublicHolidays,
                source_id: victim.source_id.clone().unwrap(),
            },
        )
        .await
        .unwrap();
    let (counts, _) = imp.run(today(), false).await.unwrap();
    assert_eq!((counts.updated, counts.created, counts.deleted), (1, 0, 0));
    let healed = imported(&store)
        .await
        .into_iter()
        .find(|e| e.id == victim.id)
        .unwrap();
    assert_eq!(healed.kind, victim.kind);
    assert_eq!(imp.run(today(), false).await.unwrap().0.updated, 0);
}

#[tokio::test]
async fn an_import_during_a_migration_is_recorded_as_skipped() {
    let registry = Arc::new(annee_gcal::TypeRegistry::default());
    let db = Db::memory().await.unwrap();
    let svc = ImportService::new(
        importer(store(), Arc::new(FixtureEducation)),
        db,
        registry.clone(),
        Arc::new(today),
    );
    registry.set_migrating(true);
    let run = svc.run("manual").await.unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(run.error.as_deref(), Some("skipped:migrating"));
    assert_eq!(svc.recent_runs(10).await.unwrap().len(), 1);
    registry.set_migrating(false);
    assert_eq!(svc.run("manual").await.unwrap().status, "success");
}

#[tokio::test]
async fn a_run_without_the_kind_check_does_not_read_recolored_events_as_stale() {
    use annee_domain::{EventType, Source};
    let store = store();
    importer(store.clone(), Arc::new(FixtureEducation))
        .run(today(), false)
        .await
        .unwrap();
    let victim = imported(&store).await[0].clone();
    store
        .update_imported(
            &victim.id,
            &annee_gcal::ImportedEvent {
                kind: EventType::Unclassified,
                title: victim.title.clone(),
                start: victim.start,
                end: victim.end,
                source: Source::ImportPublicHolidays,
                source_id: victim.source_id.clone().unwrap(),
            },
        )
        .await
        .unwrap();
    // `annee import --dry-run` cannot know the stored colors: no spurious update.
    let (counts, _) = importer(store.clone(), Arc::new(FixtureEducation))
        .without_kind_check()
        .run(today(), true)
        .await
        .unwrap();
    assert_eq!(counts.updated, 0);
    let (counts, _) = importer(store, Arc::new(FixtureEducation))
        .run(today(), true)
        .await
        .unwrap();
    assert_eq!(counts.updated, 1);
}

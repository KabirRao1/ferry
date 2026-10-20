//! Scripts around a real send: F15-AC2..AC12 through the app, against a
//! local server and an in-memory workspace.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use ferry_app::{
    App, Client, Config, Edit, Effect, FixedAmbient, MemFs, Message, Phase, Received, RowEdit,
    RunnerEdit, ScriptLevel, ScriptPhase, Table, TestStatus, WorkspaceStore, collection, run,
};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REQUEST: &str = "collections/api/req.toml";

/// The app on an in-memory workspace, sending through a real client.
struct H {
    app: App,
    fs: MemFs,
    store: WorkspaceStore<MemFs>,
    client: Client,
    /// Reports `Export Results` asked to save.
    reports: Vec<String>,
}

impl H {
    fn new(setup: impl FnOnce(&MemFs)) -> Self {
        let fs = MemFs::new();
        let store = WorkspaceStore::new(fs.clone(), &PathBuf::from("/ws"));
        store.init().unwrap();
        setup(&fs);
        let app = App::default()
            .with_ambient(Box::new(FixedAmbient::new(
                UNIX_EPOCH + Duration::from_secs(1_792_264_040),
                0,
            )))
            .with_workspace(store.load());
        Self {
            app,
            fs,
            store,
            client: Client::new(Config::default()).unwrap(),
            reports: Vec::new(),
        }
    }

    fn text(&self, rel: &str) -> Option<String> {
        self.fs.text(PathBuf::from("/ws").join(rel))
    }

    /// Handle `message` and every effect it leads to, as the UI would.
    async fn update(&mut self, message: Message) {
        let mut queue = vec![message];
        while let Some(message) = queue.pop() {
            let mut effects: Vec<Effect> = self.app.update(message).into_iter().collect();
            while let Some(effect) = effects.pop() {
                match effect {
                    Effect::Batch(list) => effects.extend(list),
                    Effect::Workspace { action, ops } => {
                        let error = self.store.apply(&ops).err().map(|e| e.to_string());
                        let workspace = Arc::new(self.store.load());
                        queue.push(Message::WorkspaceApplied {
                            action,
                            error,
                            workspace,
                        });
                    }
                    effect @ (Effect::Send { .. }
                    | Effect::ScriptedSend { .. }
                    | Effect::RunStep { .. }) => {
                        if let Some(m) = run(effect, self.client.clone()).await {
                            queue.push(m);
                        }
                    }
                    Effect::SaveReport { contents, .. } => self.reports.push(contents),
                    _ => {}
                }
            }
        }
    }

    async fn edit(&mut self, edit: Edit) {
        self.update(Message::Edit(edit)).await;
    }

    async fn send(&mut self) {
        self.update(Message::SendPressed).await;
    }

    fn received(&self) -> &Received {
        match &self.app.tab().phase {
            Phase::Received(r) => r,
            other => panic!("expected a response, got {other:?}"),
        }
    }

    fn failed(&self) -> &str {
        match &self.app.tab().phase {
            Phase::Failed(error) => error,
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}

fn request_file(url: &str, pre: &str, post: &str) -> String {
    let mut file = collection::RequestFile {
        name: "Req".into(),
        ..collection::RequestFile::default()
    };
    file.request.url = url.into();
    file.request.headers = vec![ferry_app::KeyValue::new("X-Token", "{{token}}")];
    file.request.scripts.pre_request = pre.into();
    file.request.scripts.post_response = post.into();
    collection::write_request(&file)
}

/// A collection with `request_file`, a Dev environment and globals.
fn workspace(url: &str, collection_pre: &str, pre: &str, post: &str) -> impl FnOnce(&MemFs) {
    let (url, collection_pre, pre, post) = (
        url.to_owned(),
        collection_pre.to_owned(),
        pre.to_owned(),
        post.to_owned(),
    );
    move |fs: &MemFs| {
        let put = |rel: &str, text: &str| fs.put(PathBuf::from("/ws").join(rel), text);
        put(
            "collections/api/collection.toml",
            &format!(
                "name = \"API\"\n[[variables]]\nkey = \"base\"\nvalue = \"{url}\"\n[scripts]\npre_request = {collection_pre:?}\n"
            ),
        );
        put(REQUEST, &request_file("{{base}}/items", &pre, &post));
        put(
            "environments/dev.toml",
            "name = \"Dev\"\n[[variables]]\nkey = \"token\"\nvalue = \"shared-token\"\n",
        );
    }
}

async fn open(h: &mut H) {
    h.update(Message::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )))
    .await;
    h.update(Message::OpenItem(REQUEST.into())).await;
}

#[tokio::test]
async fn f15_ac2_pre_request_scripts_set_what_the_request_uses() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .and(header("x-token", "from-script"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(r#"{"id": 7}"#, "application/json"))
        .expect(1)
        .mount(&server)
        .await;
    let mut h = H::new(workspace(
        &server.uri(),
        "pm.environment.set('token', 'from-' + pm.variables.get('part'));",
        "console.log('pre', pm.environment.get('token'));",
        "pm.test('status', () => pm.response.to.have.status(200));
         pm.test('id', () => pm.expect(pm.response.json().id).to.equal(8));
         console.warn('post');",
    ));
    h.fs.put(
        PathBuf::from("/ws/globals.toml"),
        "[[variables]]\nkey = \"part\"\nvalue = \"script\"\n",
    );
    h.app = h.app.with_workspace(h.store.load());
    open(&mut h).await;
    h.send().await;
    let r = h.received();
    let tests: Vec<_> = r
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.status))
        .collect();
    assert_eq!(
        tests,
        [("status", TestStatus::Passed), ("id", TestStatus::Failed)]
    );
    assert_eq!(r.tests[1].error.as_deref(), Some("expected 7 to equal 8"));
    assert_eq!(r.script_error, None);
    // The value went to the local file only (ADR-0045).
    let local = h.text("environments/dev.local.toml").unwrap();
    assert!(local.contains("from-script"), "{local}");
    assert!(
        h.text("environments/dev.toml")
            .unwrap()
            .contains("shared-token")
    );
    // The console shows the request as sent and the script output.
    let entry = h.app.console().entries.last().unwrap();
    assert_eq!(entry.url, format!("{}/items", server.uri()));
    assert!(
        entry
            .request_headers
            .iter()
            .any(|x| x.value == "from-script")
    );
    let lines: Vec<_> = entry
        .script_lines
        .iter()
        .map(|l| (l.level, l.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            (ScriptLevel::Log, "pre from-script"),
            (ScriptLevel::Warn, "post")
        ]
    );
    // The next send sees the saved value without a script.
    assert_eq!(
        h.app
            .workspace()
            .environment("environments/dev.toml")
            .unwrap()
            .file
            .as_ref()
            .unwrap()
            .variables[0]
            .value,
        "from-script"
    );
}

#[tokio::test]
async fn f15_ac3_a_failing_pre_request_script_blocks_the_send() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let mut h = H::new(workspace(&server.uri(), "", "throw new Error('stop');", ""));
    open(&mut h).await;
    h.send().await;
    assert_eq!(
        h.failed(),
        "Pre-request script failed in the request: Error: stop"
    );
}

#[tokio::test]
async fn f15_ac3_a_failing_post_response_script_keeps_the_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let mut h = H::new(workspace(
        &server.uri(),
        "",
        "",
        "pm.test('first', () => {}); pm.response.json();",
    ));
    open(&mut h).await;
    h.send().await;
    let r = h.received();
    assert_eq!(r.response.status, 204);
    assert_eq!(r.tests.len(), 1);
    assert!(
        r.script_error.as_deref().unwrap().starts_with(
            "Post-response script failed in the request: Error: Response body is not valid JSON"
        ),
        "{:?}",
        r.script_error
    );
}

#[tokio::test]
async fn f15_ac7_pre_request_scripts_change_the_request_sent() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/items"))
        .and(query_param("page", "2"))
        .and(header("x-added", "yes"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let mut h = H::new(workspace(
        &server.uri(),
        "",
        "pm.request.method = 'PUT';
         pm.request.url = pm.request.url + '?page=2';
         pm.request.headers.add({key: 'X-Added', value: 'yes'});
         pm.request.body.update('{\"a\": 1}');",
        "",
    ));
    open(&mut h).await;
    h.send().await;
    assert_eq!(h.received().response.status, 200);
    // The draft itself is unchanged.
    assert_eq!(h.app.tab().draft.url, "{{base}}/items");
    assert!(!h.app.tab().is_dirty());
}

#[tokio::test]
async fn f15_ac12_send_request_feeds_the_real_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"token": "t-1"}"#, "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .and(header("x-token", "t-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let mut h = H::new(workspace(
        &server.uri(),
        "",
        "pm.sendRequest({url: pm.variables.get('base') + '/token', method: 'POST'}, (err, res) => {
           pm.variables.set('token', res.json().token);
         });",
        "",
    ));
    open(&mut h).await;
    h.send().await;
    assert_eq!(h.received().response.status, 200);
    // pm.variables values are never saved.
    assert_eq!(h.text("environments/dev.local.toml"), None);
}

#[tokio::test]
async fn f15_ac4_cancel_stops_a_running_script() {
    let mut h = H::new(workspace("http://127.0.0.1:9", "", "while (true) {}", ""));
    open(&mut h).await;
    let Some(effect) = h.app.update(Message::SendPressed) else {
        panic!("expected a send")
    };
    let client = h.client.clone();
    let task = tokio::spawn(run(effect, client));
    tokio::time::sleep(Duration::from_millis(200)).await;
    let started = Instant::now();
    assert!(h.app.update(Message::CancelPressed).is_none());
    let message = task.await.unwrap().unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    h.app.update(message);
    assert!(
        matches!(h.app.tab().phase, Phase::Cancelled),
        "{:?}",
        h.app.tab().phase
    );
}

#[tokio::test]
async fn f15_ac2_requests_without_scripts_send_as_before() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut h = H::new(|_| {});
    h.edit(Edit::Url(format!("{}/plain", server.uri()))).await;
    let effect = h.app.update(Message::SendPressed);
    assert!(matches!(effect, Some(Effect::Send { .. })), "{effect:?}");
    // With a script, the same request goes the scripted way.
    let mut h = H::new(|_| {});
    h.edit(Edit::Url(format!("{}/plain", server.uri()))).await;
    h.edit(Edit::Script(
        ScriptPhase::PostResponse,
        "pm.test('t', () => {});".into(),
    ))
    .await;
    h.edit(Edit::Row(Table::Headers, 0, RowEdit::Key("A".into())))
        .await;
    let effect = h.app.update(Message::SendPressed);
    assert!(
        matches!(effect, Some(Effect::ScriptedSend { .. })),
        "{effect:?}"
    );
}

#[tokio::test]
async fn f15_ac5_globals_and_collection_changes_go_to_local_files() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut h = H::new(workspace(
        &server.uri(),
        "",
        "",
        "pm.globals.set('seen', 'yes');
         pm.collectionVariables.set('base', pm.variables.get('base'));
         pm.collectionVariables.set('extra', '1');
         pm.environment.unset('token');",
    ));
    open(&mut h).await;
    h.send().await;
    assert_eq!(h.received().response.status, 200);
    let globals = h.text("globals.local.toml").unwrap();
    assert!(globals.contains("key = \"seen\""), "{globals}");
    let local = h.text("collections/api/collection.local.toml").unwrap();
    assert!(local.contains("key = \"extra\""), "{local}");
    let shared = h.text("collections/api/collection.toml").unwrap();
    assert!(!shared.contains("extra"), "{shared}");
    // Unsetting a value that only the shared file has leaves no local file.
    assert_eq!(h.text("environments/dev.local.toml"), None);
}

// --- F16 the Runner tab ---------------------------------------------------------------

fn runner(h: &H) -> &ferry_app::RunnerDoc {
    match &h.app.tab().doc {
        ferry_app::Doc::Runner(r) => r,
        other => panic!("{other:?}"),
    }
}

/// A collection whose two requests count their calls in `pm.variables`.
fn counting(url: &str) -> impl FnOnce(&MemFs) {
    let url = url.to_owned();
    move |fs: &MemFs| {
        let put = |rel: &str, text: &str| fs.put(PathBuf::from("/ws").join(rel), text);
        put(
            "collections/api/collection.toml",
            &format!("name = \"API\"\n[[variables]]\nkey = \"base\"\nvalue = \"{url}\"\n"),
        );
        put(
            "collections/api/_order.toml",
            "order = [\"one.toml\", \"two.toml\"]\n",
        );
        put(
            "collections/api/one.toml",
            "name = \"One\"\nurl = \"{{base}}/one?user={{user}}\"\n[scripts]\npost_response = \"pm.variables.set('n', (Number(pm.variables.get('n')) || 0) + 1); pm.test('ok', () => pm.response.to.have.status(200));\"\n",
        );
        put(
            "collections/api/two.toml",
            "name = \"Two\"\nurl = \"{{base}}/two\"\n[scripts]\npost_response = \"pm.test('n', () => pm.expect(pm.variables.get('n')).to.equal(String(pm.info.iteration + 1))); pm.globals.set('last', pm.iterationData.get('user'));\"\n",
        );
    }
}

#[tokio::test]
async fn f16_ac1_opening_the_runner() {
    let mut h = H::new(counting("http://127.0.0.1:9"));
    h.update(Message::OpenRunner("collections/api".into()))
        .await;
    let r = runner(&h);
    assert_eq!(r.name, "API");
    let items: Vec<_> = r
        .items
        .iter()
        .map(|i| (i.name.as_str(), i.method.as_str(), i.checked))
        .collect();
    assert_eq!(items, [("One", "GET", true), ("Two", "GET", true)]);
    assert_eq!((r.iterations.as_str(), r.delay.as_str()), ("1", "0"));
    assert!(r.can_run());
    assert_eq!(h.app.tab().title(), "API");
    // Opening it again brings the same tab forward; requests and unknown
    // paths open nothing.
    let tabs = h.app.tabs().len();
    h.update(Message::OpenRunner("collections/api".into()))
        .await;
    h.update(Message::OpenRunner("collections/api/one.toml".into()))
        .await;
    h.update(Message::OpenRunner("collections/none".into()))
        .await;
    assert_eq!(h.app.tabs().len(), tabs);
}

#[tokio::test]
async fn f16_ac1_invalid_settings_disable_run() {
    let mut h = H::new(counting("http://127.0.0.1:9"));
    h.update(Message::OpenRunner("collections/api".into()))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Iterations("0".into())))
        .await;
    assert_eq!(
        runner(&h).iterations_error(),
        Some("Enter a number from 1 to 1000")
    );
    assert!(!runner(&h).can_run());
    h.update(Message::RunnerEdit(RunnerEdit::Iterations("2".into())))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Delay("70000".into())))
        .await;
    assert_eq!(
        runner(&h).delay_error(),
        Some("Enter a number from 0 to 60000")
    );
    h.update(Message::RunnerEdit(RunnerEdit::Delay("5".into())))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Item(0, false)))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Item(1, false)))
        .await;
    assert!(!runner(&h).can_run(), "nothing checked");
    h.update(Message::RunnerEdit(RunnerEdit::Item(1, true)))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Bail(true))).await;
    assert!(runner(&h).can_run());
    // A bad data file blocks the run until it is cleared.
    let tab = h.app.tab().id;
    let effect = h.app.update(Message::RunnerPickData);
    assert!(matches!(effect, Some(Effect::PickDataFile { tab: t }) if t == tab));
    h.update(Message::DataFileLoaded {
        tab,
        path: "d.txt".into(),
        result: Ok("[1]".into()),
    })
    .await;
    assert_eq!(
        runner(&h).data,
        Some(Err(
            "Data file must be CSV with a header row or a JSON array of objects".into()
        ))
    );
    assert!(!runner(&h).can_run());
    assert!(h.app.update(Message::RunStart).is_none());
    h.update(Message::DataFileLoaded {
        tab,
        path: "d.csv".into(),
        result: Err("Could not read d.csv: denied".into()),
    })
    .await;
    assert_eq!(
        runner(&h).data,
        Some(Err("Could not read d.csv: denied".into()))
    );
    h.update(Message::RunnerEdit(RunnerEdit::ClearData)).await;
    assert_eq!(runner(&h).data_file, None);
    assert!(runner(&h).can_run());
    assert!(h.app.update(Message::RunStart).is_some());
}

#[tokio::test]
async fn f16_ac2_runs_with_data_and_carries_variables() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut h = H::new(counting(&server.uri()));
    h.update(Message::OpenRunner("collections/api".into()))
        .await;
    let tab = h.app.tab().id;
    h.update(Message::DataFileLoaded {
        tab,
        path: "users.csv".into(),
        result: Ok("user\nada\nbob\n".into()),
    })
    .await;
    h.update(Message::RunStart).await;
    let run_state = runner(&h).run.as_ref().unwrap();
    assert_eq!(run_state.status, ferry_app::runner::RunStatus::Finished);
    let got: Vec<_> = run_state
        .results
        .iter()
        .map(|r| {
            (
                r.iteration,
                r.name.as_str(),
                r.status,
                r.passed(),
                r.failed_tests(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (0, "One", Some(200), 1, 0),
            (0, "Two", Some(200), 1, 0),
            (1, "One", Some(200), 1, 0),
            (1, "Two", Some(200), 1, 0),
        ]
    );
    assert_eq!(
        run_state.results[2].url,
        format!("{}/one?user=bob", server.uri())
    );
    // Saved scopes reach their local files; pm.variables never do.
    let globals = h.text("globals.local.toml").unwrap();
    assert!(globals.contains("value = \"bob\""), "{globals}");
    // Selecting a result, then exporting the report.
    h.update(Message::RunSelect(1)).await;
    assert_eq!(runner(&h).selected, Some(1));
    h.update(Message::RunExport).await;
    let report: serde_json::Value = serde_json::from_str(&h.reports[0]).unwrap();
    assert_eq!(report["totals"]["requests"], 4);
    assert_eq!(report["iterations"], 2);
    h.update(Message::ReportSaved {
        tab,
        result: Ok("C:/r/run.json".into()),
    })
    .await;
    assert_eq!(runner(&h).notice.as_deref(), Some("Saved to C:/r/run.json"));
}

#[tokio::test]
async fn f16_ac4_stop_ends_the_run() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut h = H::new(counting(&server.uri()));
    h.update(Message::OpenRunner("collections/api".into()))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Iterations("50".into())))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Delay("60000".into())))
        .await;
    // The first step runs at once; the second waits a minute.
    let Some(first) = h.app.update(Message::RunStart) else {
        panic!("no step")
    };
    let message = run(first, h.client.clone()).await.unwrap();
    let Some(second) = h.app.update(message) else {
        panic!("no second step")
    };
    let waiting = tokio::spawn(run(second, h.client.clone()));
    assert!(h.app.update(Message::RunStop).is_none());
    let message = tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(h.app.update(message).is_none(), "nothing runs after a stop");
    let run_state = runner(&h).run.as_ref().unwrap();
    assert_eq!(run_state.status, ferry_app::runner::RunStatus::Stopped);
    assert_eq!(run_state.notes, ["Run stopped"]);
    assert_eq!(run_state.results.len(), 1);
    // Settings can change once it has stopped.
    h.update(Message::RunnerEdit(RunnerEdit::Iterations("3".into())))
        .await;
    assert_eq!(runner(&h).iterations, "3");
    // A new run ignores results of the old one.
    let Some(_) = h.app.update(Message::RunStart) else {
        panic!("no step")
    };
    let tab = h.app.tab().id;
    let stale = h.app.update(Message::RunStepFinished {
        tab,
        generation: 1,
        result: Arc::new(ferry_app::Finished::Cancelled),
        report: Arc::new(ferry_app::ScriptReport::default()),
    });
    assert!(stale.is_none());
    assert!(runner(&h).running());
    // While running, settings stay as they are.
    h.update(Message::RunnerEdit(RunnerEdit::Iterations("9".into())))
        .await;
    assert_eq!(runner(&h).iterations, "3");
}

#[tokio::test]
async fn f16_ac4_closing_the_tab_stops_the_run() {
    let mut h = H::new(counting("http://127.0.0.1:9"));
    h.update(Message::OpenRunner("collections/api".into()))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Delay("60000".into())))
        .await;
    h.update(Message::RunnerEdit(RunnerEdit::Iterations("2".into())))
        .await;
    let Some(first) = h.app.update(Message::RunStart) else {
        panic!("no step")
    };
    let message = run(first, h.client.clone()).await.unwrap();
    let Some(second) = h.app.update(message) else {
        panic!("no second step")
    };
    let tab = h.app.tab().id;
    let step = tokio::spawn(run(second, h.client.clone()));
    h.app.update(Message::CloseTab(tab));
    let done = tokio::time::timeout(Duration::from_secs(5), step).await;
    assert!(done.is_ok(), "the waiting step was cancelled");
}

//! App state: the settings, and the two followers that read Arena's log.
//!
//! - The **local** follower always runs. It feeds the overlays (through its live view) and,
//!   unless that is turned off, saves every draft and game to the archive on this Mac.
//! - The **upload** follower runs only with a 17Lands token. It is `recall-core`'s uploader
//!   as Recall runs it: blocking, retrying, with its own checkpoint.
//!
//! Two followers over one log, because an upload may wait minutes on a network that is down
//! and the overlays must not wait with it.
//!
//! Nothing polls: every change to what the windows display calls [`AppState::notify`], which
//! sends [`Signal::Status`] to the main thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use recall_core::checkpoint::{self, Checkpoints};
use recall_core::events::{GameEvent, Service};
use recall_core::follower::Follower;
use recall_core::paths as log_paths;
use recall_core::services::archive::Archive;
use recall_core::services::seventeenlands::{self, ApiClient, SeventeenLands, SubmitOutcome, Submitter};
use serde_json::Value;
use smol::channel::Sender;

use crate::feed::{Signal, Views};
use crate::paths;
use crate::settings::Settings;

pub const LOCAL_THREAD: &str = "brainstorm-local";
pub const UPLOAD_THREAD: &str = "brainstorm-upload";

/// Where uploads go, if anywhere.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Uploads {
    /// No 17Lands token is saved.
    #[default]
    NoToken,
    /// A development build with no `BRAINSTORM_HOST`: never the live API by accident.
    Held,
    To(String),
}

/// A snapshot of everything the windows show.
#[derive(Clone, Default)]
pub struct Status {
    pub following: bool,
    pub log_path: Option<String>,
    pub settings: Settings,
    pub uploads: Uploads,
    pub upload_count: u64,
    pub last_endpoint: Option<String>,
    pub last_time: Option<String>,
    pub upload_failed: u64,
    pub last_failure: Option<String>,
}

/// Upload activity, shared between the upload follower and the windows.
#[derive(Default, Clone)]
struct UploadStatus {
    count: u64,
    last_endpoint: Option<String>,
    last_time: Option<String>,
    failed: u64,
    last_failure: Option<String>,
}

/// A follower thread and the flag that stops it.
struct Running {
    cancel: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Running {
    fn stop(self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.thread.join();
    }
}

pub struct AppState {
    settings: Mutex<Settings>,
    signals: Sender<Signal>,
    /// `BRAINSTORM_LOG`, else found on each start.
    log_override: Option<PathBuf>,
    log_path: Mutex<Option<PathBuf>>,
    local: Mutex<Option<Running>>,
    upload: Mutex<Option<Running>>,
    following: Arc<AtomicBool>,
    upload_status: Arc<Mutex<UploadStatus>>,
}

impl AppState {
    /// The state, and whether this is the first run (there were no settings yet).
    pub fn new(signals: Sender<Signal>) -> (Self, bool) {
        let (settings, first_run) = Settings::load();
        let state = Self {
            settings: Mutex::new(settings),
            signals,
            log_override: std::env::var_os("BRAINSTORM_LOG").map(PathBuf::from),
            log_path: Mutex::new(None),
            local: Mutex::new(None),
            upload: Mutex::new(None),
            following: Arc::new(AtomicBool::new(false)),
            upload_status: Arc::default(),
        };
        (state, first_run)
    }

    /// Tell the windows that the displayed status changed. Never blocks.
    pub fn notify(&self) {
        let _ = self.signals.try_send(Signal::Status);
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    /// Change the settings and save them. The followers restart when what they do changed
    /// (the token, saving); the rest is only for the overlays to read.
    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let (before, after) = {
            let mut settings = self.settings.lock().unwrap();
            let before = settings.clone();
            change(&mut settings);
            (before, settings.clone())
        };
        if before == after {
            return Ok(());
        }
        let saved = after.save().map_err(|e| format!("Couldn't save the settings: {e}"));
        if before.token != after.token || before.save != after.save {
            self.stop();
            self.start();
        }
        self.notify();
        saved
    }

    /// Check that `token` looks like a 17Lands token (a UUID), save it and start uploading.
    pub fn save_token(&self, token: &str) -> Result<(), String> {
        let valid = recall_core::config::validate_uuid_v4(token.trim())
            .ok_or("That doesn't look like a 17Lands token (it should be a UUID).")?;
        self.update_settings(|settings| settings.token = Some(valid))
    }

    fn uploads(&self, settings: &Settings) -> Uploads {
        if settings.token.is_none() {
            return Uploads::NoToken;
        }
        match std::env::var("BRAINSTORM_HOST") {
            Ok(host) => Uploads::To(host),
            Err(_) if cfg!(debug_assertions) => Uploads::Held,
            Err(_) => Uploads::To(seventeenlands::DEFAULT_HOST.to_string()),
        }
    }

    pub fn is_following(&self) -> bool {
        self.following.load(Ordering::Relaxed)
    }

    /// Start the followers that should run and aren't. A no-op while there is no log to
    /// follow: `main.rs` tries again every so often, since Arena writes its first log only
    /// once it has been launched.
    pub fn start(&self) {
        let Some(path) = self
            .log_override
            .clone()
            .or_else(|| log_paths::possible_current_filepaths().into_iter().find(|p| p.exists()))
        else {
            return;
        };
        *self.log_path.lock().unwrap() = Some(path.clone());
        let settings = self.settings();
        let log = path.to_string_lossy().to_string();

        let mut local = self.local.lock().unwrap();
        if local.is_none() {
            *local = self.spawn_local(log.clone(), settings.save);
        }
        drop(local);

        let mut upload = self.upload.lock().unwrap();
        if upload.is_none()
            && let (Some(token), Uploads::To(host)) = (settings.token.clone(), self.uploads(&settings))
        {
            *upload = self.spawn_upload(log, token, host);
        }
    }

    fn spawn_local(&self, log: String, save: bool) -> Option<Running> {
        let cancel = Arc::new(AtomicBool::new(false));
        let following = self.following.clone();
        let signals = self.signals.clone();
        let thread = std::thread::Builder::new()
            .name(LOCAL_THREAD.into())
            .spawn({
                let cancel = cancel.clone();
                move || {
                    let notify = || {
                        let _ = signals.try_send(Signal::Status);
                    };
                    following.store(true, Ordering::Relaxed);
                    notify();
                    // The checkpoint keeps a restart from saving the same events twice. The
                    // overlays are a live view: they are handed the replayed events too.
                    let mut checkpoints =
                        Checkpoints::load_from(Some(paths::data_dir().join("local-state.json")), "local".into());
                    let archive = save.then(|| Archive::new(paths::archive_dir()));
                    let mut follower = Follower::with_sink(Local(archive)).with_view(Views(signals.clone()));
                    follower.parse_log_resumable(&log, true, &cancel, Some(&mut checkpoints));
                    following.store(false, Ordering::Relaxed);
                    notify();
                }
            })
            .inspect_err(|err| log::error!(target: "brainstorm", "Couldn't start following: {err}"))
            .ok()?;
        Some(Running { cancel, thread })
    }

    fn spawn_upload(&self, log: String, token: String, host: String) -> Option<Running> {
        let cancel = Arc::new(AtomicBool::new(false));
        let status = self.upload_status.clone();
        let signals = self.signals.clone();
        let thread = std::thread::Builder::new()
            .name(UPLOAD_THREAD.into())
            .spawn({
                let cancel = cancel.clone();
                move || {
                    let state = paths::data_dir().join("upload-state.json");
                    // Coming from Recall: carry on from where it left off instead of
                    // uploading the whole of today's log a second time.
                    if !state.exists()
                        && let Some(recall) = checkpoint::default_path().filter(|p| p.exists())
                    {
                        let _ = std::fs::create_dir_all(paths::data_dir());
                        let _ = std::fs::copy(recall, &state);
                    }
                    let mut checkpoints = Checkpoints::load_from(Some(state), checkpoint::scope_for(&host, &token));
                    // The client shares the cancel flag, so a stop also ends a retry backoff.
                    let wire = Observing {
                        inner: ApiClient::new(host).with_cancel(cancel.clone()),
                        status,
                        on_change: Box::new(move || {
                            let _ = signals.try_send(Signal::Status);
                        }),
                    };
                    let mut follower = Follower::with_sink(SeventeenLands::new(token, wire));
                    follower.parse_log_resumable(&log, true, &cancel, Some(&mut checkpoints));
                }
            })
            .inspect_err(|err| log::error!(target: "brainstorm", "Couldn't start uploading: {err}"))
            .ok()?;
        Some(Running { cancel, thread })
    }

    /// Stop both followers. Returns within about one follower tick (half a second), also
    /// while an upload is backing off; only a request already on the wire can hold it longer.
    pub fn stop(&self) {
        let running = [self.local.lock().unwrap().take(), self.upload.lock().unwrap().take()];
        for follower in running.into_iter().flatten() {
            follower.stop();
        }
        self.following.store(false, Ordering::Relaxed);
        self.notify();
    }

    pub fn status(&self) -> Status {
        let settings = self.settings();
        let upload = self.upload_status.lock().unwrap().clone();
        Status {
            following: self.is_following(),
            log_path: self.log_path.lock().unwrap().as_ref().map(|p| p.to_string_lossy().to_string()),
            uploads: self.uploads(&settings),
            settings,
            upload_count: upload.count,
            last_endpoint: upload.last_endpoint,
            last_time: upload.last_time,
            upload_failed: upload.failed,
            last_failure: upload.last_failure,
        }
    }
}

/// The local follower's service: the archive, or nothing. Always takes the event, so the
/// checkpoint moves either way and turning saving on later starts from then.
struct Local(Option<Archive>);

impl Service for Local {
    fn handle(&mut self, event: &GameEvent) -> bool {
        if let Some(archive) = &mut self.0 {
            archive.handle(event);
        }
        true
    }
}

/// A `Submitter` that delegates to the live client and records what it sent, for the status
/// line. The POST itself is untouched.
struct Observing {
    inner: ApiClient,
    status: Arc<Mutex<UploadStatus>>,
    /// Called after each recorded outcome, so the windows refresh without polling.
    on_change: Box<dyn Fn() + Send>,
}

impl Submitter for Observing {
    fn submit(&mut self, endpoint: &str, payload: Value, use_gzip: bool) -> SubmitOutcome {
        let outcome = self.inner.submit(endpoint, payload, use_gzip);
        if let Ok(mut status) = self.status.lock() {
            let now = chrono::Local::now().format("%H:%M:%S").to_string();
            let short = endpoint.rsplit('/').next().unwrap_or(endpoint);
            match outcome {
                SubmitOutcome::Accepted => {
                    status.count += 1;
                    status.last_endpoint = Some(short.to_string());
                    status.last_time = Some(now);
                }
                SubmitOutcome::Rejected { status: code } => {
                    status.failed += 1;
                    status.last_failure = Some(format!("{short}: HTTP {code} @ {now}"));
                }
                // A stop, not a lost event: the next start uploads it.
                SubmitOutcome::Cancelled => {}
                SubmitOutcome::Failed => {
                    status.failed += 1;
                    status.last_failure = Some(format!("{short}: not delivered @ {now}"));
                }
            }
        }
        // After the lock is released: the listener reads the status back.
        (self.on_change)();
        outcome
    }
}

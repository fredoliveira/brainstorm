//! How what the followers see reaches the main thread.
//!
//! The followers run on threads of their own (see `state.rs`) and report through the `log`
//! crate, the live view and the upload observer. All of it goes into one channel as
//! [`Signal`]s; a task on the main thread drains it (see `main.rs`). Nothing polls.

use std::collections::VecDeque;

use gpui_kit::SharedString;
use log::{Level, LevelFilter, Log, Metadata, Record};
use recall_core::events::{DraftSource, GameEvent};
use recall_core::live::{GameView, Live, View};
use smol::channel::Sender;

use crate::state::{Status, UPLOAD_THREAD};

/// Lines kept for the log window.
const CAPACITY: usize = 2000;

pub enum Signal {
    Draft(DraftEvent),
    /// The game in progress as it now stands, or `None` once there is none.
    Game(Option<GameView>),
    /// Something the status shows changed (a follower started or stopped, an upload outcome,
    /// a setting).
    Status,
    Log(LogLine),
    /// A menu-bar menu item was chosen; the string is its id.
    Menu(String),
}

/// A pack waiting for a pick.
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    /// Arena's name for the event, e.g. `PremierDraft_HOB_20260811`.
    pub event_name: String,
    pub pack: i64,
    pub pick: i64,
    pub card_ids: Vec<i64>,
}

impl Pack {
    /// `PremierDraft_HOB_20260811` → (`PremierDraft`, `HOB`).
    pub fn format_and_set(&self) -> Option<(&str, &str)> {
        let mut parts = self.event_name.split('_');
        Some((parts.next()?, parts.next()?))
    }
}

pub enum DraftEvent {
    Pack(Pack),
    Pick { pack: i64, pick: i64 },
    Complete,
}

impl DraftEvent {
    fn from(event: &GameEvent) -> Option<Self> {
        match event {
            GameEvent::DraftPack(p) => {
                // A LogBusiness pack is logged together with its pick: too late to be useful.
                if p.source == DraftSource::LogBusiness {
                    return None;
                }
                // Card ids are integers in human drafts and strings in bot drafts.
                let card_ids = p
                    .card_ids
                    .as_array()?
                    .iter()
                    .filter_map(|id| id.as_i64().or_else(|| id.as_str()?.parse().ok()))
                    .collect();
                Some(DraftEvent::Pack(Pack {
                    event_name: p.event_name.as_ref()?.as_str()?.to_string(),
                    pack: p.pack_number.unwrap_or(0),
                    pick: p.pick_number.unwrap_or(0),
                    card_ids,
                }))
            }
            GameEvent::DraftPick(p) => {
                Some(DraftEvent::Pick { pack: p.pack_number.unwrap_or(0), pick: p.pick_number.unwrap_or(0) })
            }
            GameEvent::EventCourse { .. } => Some(DraftEvent::Complete),
            _ => None,
        }
    }
}

/// The live view on the local follower: hands what the overlays show to the main thread.
/// Events replayed after a restart count like any other, which is how a draft or a game
/// already under way is picked up where it stands.
pub struct Views(pub Sender<Signal>);

impl View for Views {
    fn handle(&mut self, update: Live<'_>) {
        let signal = match update {
            Live::Event { event, .. } => DraftEvent::from(event).map(Signal::Draft),
            Live::Game(game) => Some(Signal::Game(game.cloned())),
        };
        if let Some(signal) = signal {
            let _ = self.0.try_send(signal);
        }
    }
}

#[derive(Clone)]
pub struct LogLine {
    pub ts: SharedString,
    pub level: Level,
    pub msg: SharedString,
}

/// What the windows show: the log and the status. It outlives them, so reopening one shows
/// what happened meanwhile.
#[derive(Default)]
pub struct Feed {
    pub lines: VecDeque<LogLine>,
    pub status: Status,
}

impl Feed {
    pub fn push(&mut self, line: LogLine) {
        if self.lines.len() >= CAPACITY {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }
}

/// The global `log` sink. Installed before the app exists; lines queue in the channel until
/// the main thread starts draining it.
struct Logger(Sender<Signal>);

pub fn install_logger(tx: Sender<Signal>) {
    if log::set_boxed_logger(Box::new(Logger(tx))).is_ok() {
        // Debug: the 17Lands transport logs an accepted upload ("<endpoint> -> 200") at debug.
        log::set_max_level(LevelFilter::Debug);
    }
}

impl Log for Logger {
    fn enabled(&self, meta: &Metadata) -> bool {
        meta.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        let target = record.target();
        // Ours and the core's only: third-party crates (the HTTP client) are noisy at debug.
        if !target.starts_with("recall") && !target.starts_with("brainstorm") {
            return;
        }
        // Two followers read the same log, and both say what they find in it. The local one
        // always runs, so the upload follower's account of the log is the one dropped; what
        // only it can say (the transport's lines) is kept.
        let transport = target.ends_with("::transport");
        if std::thread::current().name() == Some(UPLOAD_THREAD) && !transport {
            return;
        }
        let line = LogLine {
            ts: chrono::Local::now().format("%H:%M:%S").to_string().into(),
            level: record.level(),
            msg: record.args().to_string().into(),
        };
        // The terminal gets everything when run from one.
        eprintln!("{} {:<5} {}", line.ts, line.level, line.msg);
        // The window gets what a player cares about: no background-sync chatter, and of the
        // debug lines only the transport's, which is where an accepted upload is reported.
        let wanted = target != recall_core::follower::CHATTER && (record.level() <= Level::Info || transport);
        if wanted {
            let _ = self.0.try_send(Signal::Log(line));
        }
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use recall_core::events::{Context, DraftPack};
    use serde_json::{Value, json};

    use super::*;

    fn pack(source: DraftSource, card_ids: Value) -> GameEvent {
        let time = chrono::NaiveDateTime::default();
        GameEvent::DraftPack(DraftPack {
            source,
            draft_id: None,
            event_name: Some("PremierDraft_HOB_20260811".into()),
            pack_number: Some(1),
            pick_number: Some(3),
            card_ids,
            raw: Value::Null,
            ctx: Context { player_id: None, log_time: time, utc_time: time, event_time: None, raw_time: String::new() },
        })
    }

    #[test]
    fn packs_carry_card_ids_as_integers_or_strings() {
        for ids in [json!([101, 102]), json!(["101", "102"])] {
            let Some(DraftEvent::Pack(pack)) = DraftEvent::from(&pack(DraftSource::DraftNotify, ids)) else {
                panic!("expected a pack");
            };
            assert_eq!((pack.pack, pack.pick), (1, 3));
            assert_eq!(pack.card_ids, [101, 102]);
        }
    }

    #[test]
    fn packs_logged_with_their_pick_are_ignored() {
        assert!(DraftEvent::from(&pack(DraftSource::LogBusiness, json!([101]))).is_none());
        assert!(DraftEvent::from(&pack(DraftSource::DraftNotify, Value::Null)).is_none());
    }
}

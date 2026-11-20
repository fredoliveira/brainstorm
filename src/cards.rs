//! Card data: 17Lands win rates, names and images, and mana costs from Scryfall.
//!
//! Everything here blocks on the network or the disk, so call it from a background task. All
//! of it is cached under the cache directory; the app works offline with what it has.
//!
//! - **Ratings**: one request per set, format and player group, kept for three hours.
//!   17Lands' numbers move fast in a set's first weeks, and the app runs for days.
//! - **Cards**: name, type and image for cards the ratings table leaves out (basic lands) or
//!   that are not from the set being drafted (any deck card). Kept for good: an Arena card id
//!   never changes meaning.
//! - **Costs**: 17Lands' card lookup has no mana costs or colours, so those come from
//!   Scryfall, one card at a time, also kept for good. Scryfall learns a new set's Arena ids
//!   late, so a card it does not know by id is asked for by name.
//! - **Images**: downloaded once as WebP and turned into the two files the panels show.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::paths;

const RATINGS_MAX_AGE: Duration = Duration::from_secs(3 * 3600);
const USER_AGENT: &str = concat!("brainstorm/", env!("CARGO_PKG_VERSION"));
/// Scryfall asks for no more than ten requests a second.
const SCRYFALL_PAUSE: Duration = Duration::from_millis(110);

/// 17Lands' player cohorts (its `user_group`), ranked by each player's own win rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    #[default]
    All,
    Top,
    Middle,
    Bottom,
}

impl Group {
    pub const EVERY: [Group; 4] = [Group::All, Group::Top, Group::Middle, Group::Bottom];

    /// As 17Lands spells it, and as the cache files are named.
    fn key(self) -> &'static str {
        match self {
            Group::All => "all",
            Group::Top => "top",
            Group::Middle => "middle",
            Group::Bottom => "bottom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Group::All => "All",
            Group::Top => "Top",
            Group::Middle => "Middle",
            Group::Bottom => "Bottom",
        }
    }

    /// The group after this one, for cycling through them from the panel header.
    pub fn next(self) -> Group {
        let at = Self::EVERY.iter().position(|g| *g == self).unwrap_or(0);
        Self::EVERY[(at + 1) % Self::EVERY.len()]
    }
}

/// What the panels know about a card.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Card {
    pub id: i64,
    pub name: String,
    /// Colour letters (`"WU"`); empty for colourless cards and for cards not from a ratings
    /// table.
    pub color: String,
    pub rarity: String,
    pub types: Vec<String>,
    pub image: String,
    /// GIH WR: how often decks won games in which the card was drawn. `None` when 17Lands'
    /// sample is too small, and for cards not from a ratings table.
    pub win_rate: Option<f64>,
}

impl Card {
    pub fn is_land(&self) -> bool {
        self.types.first().is_some_and(|t| t.contains("Land"))
    }

    /// `"Creature - Human Wizard"` → `"Creature"`.
    pub fn type_line(&self) -> &str {
        let line = self.types.first().map(String::as_str).unwrap_or("");
        line.split(" - ").next().unwrap_or(line)
    }
}

/// A set's ratings for one format and player group.
#[derive(Debug)]
pub struct Ratings {
    pub by_id: HashMap<i64, Card>,
    /// The format the numbers are for, which is not the draft's own when that had none.
    pub format: String,
    /// Mean and standard deviation of the set's win rates, to grade a card against.
    pub mean: f64,
    pub sd: f64,
}

impl Ratings {
    fn new(cards: Vec<Card>, format: &str) -> Self {
        let rates: Vec<f64> = cards.iter().filter_map(|c| c.win_rate).collect();
        let mean = if rates.is_empty() { 0.55 } else { rates.iter().sum::<f64>() / rates.len() as f64 };
        let variance = if rates.is_empty() {
            0.
        } else {
            rates.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / rates.len() as f64
        };
        let mut by_id = HashMap::new();
        for card in cards {
            by_id.entry(card.id).or_insert(card);
        }
        Self { by_id, format: format.to_string(), mean, sd: variance.sqrt().max(0.005) }
    }

    fn has_data(&self) -> bool {
        self.by_id.values().any(|c| c.win_rate.is_some())
    }

    /// How far a win rate is from the set's mean, in standard deviations.
    pub fn z(&self, win_rate: f64) -> f64 {
        (win_rate - self.mean) / self.sd
    }
}

/// What Scryfall adds to a card: its mana cost as Scryfall writes it (`{2}{W}{U}`), its mana
/// value, and its colours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub mana_cost: String,
    pub cmc: f64,
    /// Colour letters in WUBRG order. A card with no colours of its own (a land) has its
    /// colour identity here instead: the colours of mana it makes or asks for.
    pub colors: String,
}

/// A card's image as the two files the panels show.
#[derive(Debug, Clone, PartialEq)]
pub struct Art {
    /// The art box, for a row's thumbnail.
    pub thumb: PathBuf,
    /// The whole card, for the hover preview.
    pub full: PathBuf,
}

pub struct Store {
    dir: PathBuf,
    agent: ureq::Agent,
    ratings: Mutex<HashMap<String, (Arc<Ratings>, SystemTime)>>,
    cards: Mutex<Option<HashMap<i64, Card>>>,
    costs: Mutex<Option<Costs>>,
}

/// What Scryfall has said so far.
struct Costs {
    /// Kept on disk for good.
    known: HashMap<i64, Cost>,
    /// Cards it did not know, by id or by name, so each is asked about once a run.
    missed: HashSet<i64>,
}

/// The one store, on the cache directory.
pub fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| Store::new(paths::cache_dir()))
}

fn age(file: &Path) -> Option<Duration> {
    std::fs::metadata(file).ok()?.modified().ok()?.elapsed().ok()
}

fn write(file: &Path, bytes: &[u8]) {
    // Beside the file and then over it: a reader never sees half of it.
    let tmp = file.with_extension("tmp");
    if std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, file)).is_err() {
        let _ = std::fs::remove_file(tmp);
    }
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(dir.join("img"));
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).user_agent(USER_AGENT).build();
        Self { dir, agent, ratings: Mutex::default(), cards: Mutex::default(), costs: Mutex::default() }
    }

    fn get(&self, url: &str, query: &[(&str, &str)]) -> Option<Vec<u8>> {
        let mut request = self.agent.get(url).set("Accept", "*/*");
        for (key, value) in query {
            request = request.query(key, value);
        }
        let mut bytes = Vec::new();
        request.call().ok()?.into_reader().take(32 << 20).read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }

    // ── Ratings ──────────────────────────────────────────────────────────────

    /// Ratings for the chosen player group, always: switching groups from pack to pack would
    /// show the same card at very different win rates. Only the format falls back, for the
    /// whole set at once: formats with little play (early Quick Draft, special events) have
    /// no win rates at all for a while, and then Premier Draft's are used.
    pub fn ratings(&self, set: &str, format: &str, group: Group) -> Option<Arc<Ratings>> {
        let primary = self.one(set, format, group);
        if primary.as_ref().is_some_and(|r| r.has_data()) || format == "PremierDraft" {
            return primary;
        }
        self.one(set, "PremierDraft", group).or(primary)
    }

    fn one(&self, set: &str, format: &str, group: Group) -> Option<Arc<Ratings>> {
        let key = match group {
            Group::All => format!("{set}-{format}"),
            group => format!("{set}-{format}-{}", group.key()),
        };
        let fresh = |since: SystemTime| since.elapsed().is_ok_and(|age| age < RATINGS_MAX_AGE);
        if let Some((ratings, loaded)) = self.ratings.lock().unwrap().get(&key)
            && fresh(*loaded)
        {
            return Some(ratings.clone());
        }

        let file = self.dir.join(format!("ratings-{key}.json"));
        let file_age = age(&file);
        let now = SystemTime::now();
        let from_disk = || std::fs::read(&file).ok().and_then(|bytes| Some((decode_ratings(&bytes)?, file_age?)));
        let (cards, updated) = match file_age {
            Some(age) if age < RATINGS_MAX_AGE => from_disk().map(|(cards, age)| (cards, now - age)),
            _ => None,
        }
        .or_else(|| {
            let mut query = vec![("expansion", set), ("event_type", format), ("time_period", "ALL_TIME")];
            if group != Group::All {
                query.push(("user_group", group.key()));
            }
            let bytes = self.get("https://www.17lands.com/api/card_data", &query)?;
            let cards = decode_ratings(&bytes)?;
            write(&file, &bytes);
            Some((cards, now))
        })
        // Stale beats nothing.
        .or_else(|| from_disk().map(|(cards, age)| (cards, now - age)))?;

        let ratings = Arc::new(Ratings::new(cards, format));
        // Expires with the file, not three hours from now.
        self.ratings.lock().unwrap().insert(key, (ratings.clone(), updated));
        Some(ratings)
    }

    /// When any ratings were last fetched, judging by the cached files.
    pub fn newest_ratings(&self) -> Option<SystemTime> {
        std::fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("ratings-"))
            .filter_map(|entry| entry.metadata().ok()?.modified().ok())
            .max()
    }

    // ── Cards ────────────────────────────────────────────────────────────────

    /// Name, type and image for these cards, from 17Lands' card lookup. Cards it does not
    /// know are simply missing from the answer.
    pub fn lookup(&self, ids: &[i64]) -> HashMap<i64, Card> {
        let file = self.dir.join("cards.json");
        let mut guard = self.cards.lock().unwrap();
        let known = guard.get_or_insert_with(|| {
            let saved: Vec<Card> =
                std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            saved.into_iter().map(|card| (card.id, card)).collect()
        });
        let mut missing: Vec<i64> = ids.iter().copied().filter(|id| !known.contains_key(id)).collect();
        missing.sort_unstable();
        missing.dedup();
        if !missing.is_empty() {
            let list = missing.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            let found =
                self.get("https://www.17lands.com/data/cards", &[("ids", &list)]).and_then(|b| decode_cards(&b));
            if let Some(found) = found.filter(|found| !found.is_empty()) {
                known.extend(found.into_iter().map(|card| (card.id, card)));
                let mut all: Vec<&Card> = known.values().collect();
                all.sort_by_key(|card| card.id);
                if let Ok(bytes) = serde_json::to_vec(&all) {
                    write(&file, &bytes);
                }
            }
        }
        ids.iter().filter_map(|id| Some((*id, known.get(id)?.clone()))).collect()
    }

    /// Mana costs and colours for these cards (Arena id and name), from Scryfall. Slow the
    /// first time a card is asked for (a request or two each, a tenth of a second apart),
    /// instant after.
    ///
    /// Scryfall is asked by Arena id first, and by name when it does not know the id: it has
    /// a new set's cards weeks before it has their Arena ids, which is exactly when that set
    /// is being drafted.
    pub fn costs(&self, cards: &[(i64, String)]) -> HashMap<i64, Cost> {
        let file = self.dir.join("scryfall.json");
        let mut guard = self.costs.lock().unwrap();
        let costs = guard.get_or_insert_with(|| {
            // Older files also recorded the cards Scryfall did not know, as nulls.
            let saved: HashMap<i64, Option<Cost>> =
                std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            Costs {
                known: saved.into_iter().filter_map(|(id, cost)| Some((id, cost?))).collect(),
                missed: HashSet::new(),
            }
        });

        let mut requests = 0;
        let mut ask = |url: &str, query: &[(&str, &str)]| -> Result<Option<Cost>, ()> {
            if requests > 0 {
                std::thread::sleep(SCRYFALL_PAUSE);
            }
            requests += 1;
            let mut request = self.agent.get(url).set("Accept", "application/json");
            for (key, value) in query {
                request = request.query(key, value);
            }
            match request.call() {
                Ok(response) => Ok(response.into_string().ok().and_then(|text| decode_cost(&text))),
                Err(ureq::Error::Status(404, _)) => Ok(None),
                // Offline, rate-limited, anything else: no answer, ask again another time.
                Err(_) => Err(()),
            }
        };
        let mut found = false;
        for (id, name) in cards {
            if costs.known.contains_key(id) || costs.missed.contains(id) {
                continue;
            }
            let by_id = ask(&format!("https://api.scryfall.com/cards/arena/{id}"), &[]);
            let answer = match by_id {
                Ok(None) if !name.is_empty() => ask("https://api.scryfall.com/cards/named", &[("exact", name)]),
                other => other,
            };
            match answer {
                Ok(Some(cost)) => {
                    costs.known.insert(*id, cost);
                    found = true;
                }
                // Not known under either: not asked again this run, but not written down
                // either, since Scryfall may well know it next week.
                Ok(None) => {
                    costs.missed.insert(*id);
                }
                Err(()) => break,
            }
        }
        if found && let Ok(bytes) = serde_json::to_vec(&costs.known) {
            write(&file, &bytes);
        }
        cards.iter().filter_map(|(id, _)| Some((*id, costs.known.get(id)?.clone()))).collect()
    }

    // ── Images ───────────────────────────────────────────────────────────────

    /// The card image at `url` as local files, downloading and converting it the first time.
    pub fn art(&self, url: &str) -> Option<Art> {
        // ".../display/front/0/2/02080f42-….webp?1786005780" → "02080f42-…"
        let name = url.split('?').next()?.rsplit('/').next()?;
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        let art = Art {
            thumb: self.dir.join("img").join(format!("{stem}.art.png")),
            full: self.dir.join("img").join(format!("{stem}.jpg")),
        };
        if art.thumb.exists() && art.full.exists() {
            return Some(art);
        }
        let bytes = self.get(url, &[])?;
        let card = image::load_from_memory(&bytes).ok()?;
        let (w, h) = (card.width() as f32, card.height() as f32);
        // The art box of a standard card frame, as fractions of the card image.
        let crop = card.crop_imm((w * 0.08) as u32, (h * 0.115) as u32, (w * 0.84) as u32, (h * 0.435) as u32);
        let thumb = crop.resize(160, 160, image::imageops::FilterType::Lanczos3);
        let full = card.resize(560, 800, image::imageops::FilterType::Lanczos3).to_rgb8();
        // Written under other names first: a file that exists is a file that is whole.
        let tmp = |path: &Path| path.with_extension("tmp");
        thumb.save_with_format(tmp(&art.thumb), image::ImageFormat::Png).ok()?;
        full.save_with_format(tmp(&art.full), image::ImageFormat::Jpeg).ok()?;
        std::fs::rename(tmp(&art.thumb), &art.thumb).ok()?;
        std::fs::rename(tmp(&art.full), &art.full).ok()?;
        Some(art)
    }
}

fn decode_ratings(bytes: &[u8]) -> Option<Vec<Card>> {
    #[derive(Deserialize)]
    struct Row {
        name: String,
        mtga_id: Option<i64>,
        #[serde(default)]
        color: String,
        #[serde(default)]
        rarity: String,
        #[serde(default)]
        url: String,
        types: Option<Vec<String>>,
        ever_drawn_win_rate: Option<f64>,
    }
    #[derive(Deserialize)]
    struct Response {
        data: Vec<Row>,
    }
    let response: Response = serde_json::from_slice(bytes).ok()?;
    Some(
        response
            .data
            .into_iter()
            .filter_map(|row| {
                Some(Card {
                    id: row.mtga_id?,
                    name: row.name,
                    color: row.color,
                    rarity: row.rarity,
                    types: row.types.unwrap_or_default(),
                    image: row.url,
                    win_rate: row.ever_drawn_win_rate,
                })
            })
            .collect(),
    )
}

fn decode_cards(bytes: &[u8]) -> Option<Vec<Card>> {
    #[derive(Deserialize)]
    struct Row {
        id: i64,
        name: String,
        rarity: Option<String>,
        image_url: Option<String>,
        types: Option<Vec<String>>,
    }
    #[derive(Deserialize)]
    struct Response {
        cards: Vec<Row>,
    }
    let response: Response = serde_json::from_slice(bytes).ok()?;
    Some(
        response
            .cards
            .into_iter()
            .map(|row| Card {
                id: row.id,
                name: row.name,
                color: String::new(),
                rarity: row.rarity.unwrap_or_default(),
                types: row.types.unwrap_or_default(),
                image: row.image_url.unwrap_or_default(),
                win_rate: None,
            })
            .collect(),
    )
}

/// A two-faced card has its cost and colours on the front face.
fn decode_cost(text: &str) -> Option<Cost> {
    let card: serde_json::Value = serde_json::from_str(text).ok()?;
    let mana_cost = card["mana_cost"].as_str().or_else(|| card["card_faces"][0]["mana_cost"].as_str())?;
    let letters = |list: &serde_json::Value| -> String {
        let found: Vec<&str> = list.as_array().into_iter().flatten().filter_map(|c| c.as_str()).collect();
        "WUBRG".chars().filter(|c| found.iter().any(|f| f.starts_with(*c))).collect()
    };
    let colors = [&card["colors"], &card["card_faces"][0]["colors"], &card["color_identity"]]
        .into_iter()
        .map(letters)
        .find(|colors| !colors.is_empty())
        .unwrap_or_default();
    Some(Cost { mana_cost: mana_cost.to_string(), cmc: card["cmc"].as_f64().unwrap_or(0.), colors })
}

/// One symbol of a mana cost.
#[derive(Debug, Clone, PartialEq)]
pub enum Pip {
    /// A colour letter: W, U, B, R, G, or C for colourless.
    Color(char),
    /// Generic mana, X, or anything else, as text.
    Other(String),
}

/// `{2}{W}{U}` → `[Other("2"), Color('W'), Color('U')]`. A hybrid symbol shows its first half.
pub fn pips(mana_cost: &str) -> Vec<Pip> {
    mana_cost
        .split(['{', '}'])
        .filter(|symbol| !symbol.is_empty() && *symbol != " // ")
        .map(|symbol| {
            let first = symbol.split('/').find(|part| part.len() == 1 && "WUBRGC".contains(part)).unwrap_or(symbol);
            match first.chars().next() {
                Some(c) if first.len() == 1 && "WUBRGC".contains(c) => Pip::Color(c),
                _ => Pip::Other(symbol.split('/').next().unwrap_or(symbol).to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratings_decode_and_grade() {
        let json = br#"{"data": [
            {"name": "A", "mtga_id": 1, "color": "W", "rarity": "common", "url": "u", "types": ["Creature - Cat"], "ever_drawn_win_rate": 0.60},
            {"name": "B", "mtga_id": 2, "color": "", "rarity": "rare", "url": "u", "types": ["Land"], "ever_drawn_win_rate": 0.50},
            {"name": "C", "mtga_id": 3, "color": "U", "rarity": "common", "url": "u", "ever_drawn_win_rate": null},
            {"name": "No id", "mtga_id": null, "color": "U", "rarity": "common", "url": "u"}
        ]}"#;
        let ratings = Ratings::new(decode_ratings(json).unwrap(), "PremierDraft");
        assert_eq!(ratings.by_id.len(), 3);
        assert!((ratings.mean - 0.55).abs() < 1e-9);
        assert!((ratings.sd - 0.05).abs() < 1e-9);
        assert!((ratings.z(0.60) - 1.).abs() < 1e-9);
        assert!(ratings.has_data());
        assert_eq!(ratings.by_id[&1].type_line(), "Creature");
        assert!(ratings.by_id[&2].is_land() && !ratings.by_id[&1].is_land());
    }

    #[test]
    fn a_table_with_no_win_rates_has_no_data() {
        let json = br#"{"data": [{"name": "A", "mtga_id": 1, "ever_drawn_win_rate": null}]}"#;
        let ratings = Ratings::new(decode_ratings(json).unwrap(), "QuickDraft");
        assert!(!ratings.has_data());
    }

    #[test]
    fn card_lookups_decode() {
        let json = br#"{"cards": [{"id": 75890, "name": "Plains", "rarity": "basic", "image_url": "https://x/y.webp?1", "types": ["Basic Land - Plains"]}], "emblems": []}"#;
        let cards = decode_cards(json).unwrap();
        assert_eq!(cards[0].name, "Plains");
        assert!(cards[0].is_land());
    }

    #[test]
    fn costs_come_from_the_card_or_its_front_face() {
        assert_eq!(
            decode_cost(r#"{"mana_cost": "{2}{W}", "cmc": 3.0, "colors": ["W"], "color_identity": ["W"]}"#),
            Some(Cost { mana_cost: "{2}{W}".into(), cmc: 3., colors: "W".into() })
        );
        assert_eq!(
            decode_cost(
                r#"{"cmc": 2.0, "color_identity": ["G", "U"],
                    "card_faces": [{"mana_cost": "{1}{U}", "colors": ["U"]}, {"mana_cost": "", "colors": ["G"]}]}"#
            ),
            Some(Cost { mana_cost: "{1}{U}".into(), cmc: 2., colors: "U".into() })
        );
    }

    #[test]
    fn a_land_takes_its_colours_from_its_identity() {
        let fountain = r#"{"mana_cost": "", "cmc": 0.0, "colors": [], "color_identity": ["U", "W"]}"#;
        assert_eq!(decode_cost(fountain).unwrap().colors, "WU");
        let wastes = r#"{"mana_cost": "", "cmc": 0.0, "colors": [], "color_identity": []}"#;
        assert_eq!(decode_cost(wastes).unwrap().colors, "");
    }

    #[test]
    fn mana_costs_split_into_pips() {
        assert_eq!(pips("{2}{W}{U}"), [Pip::Other("2".into()), Pip::Color('W'), Pip::Color('U')]);
        assert_eq!(pips("{X}{R/G}"), [Pip::Other("X".into()), Pip::Color('R')]);
        assert_eq!(pips("{2/W}"), [Pip::Color('W')]);
        assert_eq!(pips(""), []);
    }

    #[test]
    fn groups_cycle() {
        assert_eq!(Group::All.next(), Group::Top);
        assert_eq!(Group::Bottom.next(), Group::All);
    }
}

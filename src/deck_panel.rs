//! The deck panel: what is left in your library while you play, card by card, with the
//! chance that the next card off the top is each one.
//!
//! A row is built like the draft panel's: rarity on the left edge, a piece of the art, the
//! name over the card's colours and type. Where the draft panel has a win rate, this one has
//! the copies still unseen.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui_kit::*;

use crate::cards::{self, Art, Card, Cost, Pip};
use crate::game::Game;
use crate::overlay;
use crate::settings::Settings;
use crate::style::{self, Layout};

/// One distinct card of the deck.
#[derive(Debug, PartialEq)]
struct Row {
    card_id: i64,
    name: SharedString,
    left: usize,
    total: usize,
    /// The mana cost, which shows a spell's colours too. A land has no cost: its colours.
    pips: Vec<Pip>,
    type_line: SharedString,
    rarity: String,
    thumb: Option<PathBuf>,
}

pub struct DeckPanel {
    game: Entity<Game>,
    prefs: Entity<Settings>,
    cards: HashMap<i64, Card>,
    costs: HashMap<i64, Cost>,
    art: HashMap<i64, Art>,
    /// Cards already asked about, found or not, so each is looked up once.
    asked: HashSet<i64>,
    /// The cards being looked up now.
    loading: Vec<i64>,
    hovered: Option<usize>,
    /// Dropping it cancels the lookup.
    load: Option<Task<()>>,
    _subscriptions: [Subscription; 2],
}

impl DeckPanel {
    pub fn new(game: Entity<Game>, prefs: Entity<Settings>, cx: &mut Context<Self>) -> Self {
        let subscriptions =
            [cx.observe(&game, |this, _, cx| this.refresh(cx)), cx.observe(&prefs, |this, _, cx| this.refresh(cx))];
        let mut panel = Self {
            game,
            prefs,
            cards: HashMap::new(),
            costs: HashMap::new(),
            art: HashMap::new(),
            asked: HashSet::new(),
            loading: Vec::new(),
            hovered: None,
            load: None,
            _subscriptions: subscriptions,
        };
        panel.refresh(cx);
        panel
    }

    fn layout(&self, cx: &App) -> Layout {
        Layout::new(self.prefs.read(cx).compact)
    }

    /// The deck's rows: spells by mana value, then lands, each by name.
    fn rows(&self, cx: &App) -> Vec<Row> {
        let Some(view) = &self.game.read(cx).view else { return Vec::new() };
        let mut rows: Vec<(u8, f64, Row)> = view
            .remaining()
            .into_iter()
            .map(|card| {
                let info = self.cards.get(&card.card_id);
                let cost = self.costs.get(&card.card_id);
                let row = Row {
                    card_id: card.card_id,
                    name: info.map_or_else(|| format!("Card {}", card.card_id), |c| c.name.clone()).into(),
                    left: card.left,
                    total: card.total,
                    pips: cost.map(pips_for).unwrap_or_default(),
                    type_line: info.map(|c| c.type_line().to_string()).unwrap_or_default().into(),
                    rarity: info.map(|c| c.rarity.clone()).unwrap_or_default(),
                    thumb: self.art.get(&card.card_id).map(|art| art.thumb.clone()),
                };
                // A card with no known cost sorts after those with one.
                (info.is_some_and(Card::is_land) as u8, cost.map_or(f64::MAX, |c| c.cmc), row)
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then_with(|| a.2.name.cmp(&b.2.name)));
        merge(rows.into_iter().map(|(_, _, row)| row))
    }

    /// The panel's size, or `None` when it has nothing to show.
    pub fn size(&self, cx: &App) -> Option<(f32, f32)> {
        if !self.prefs.read(cx).deck_overlay {
            return None;
        }
        let rows = self.rows(cx).len();
        if rows == 0 {
            return None;
        }
        let layout = self.layout(cx);
        Some((layout.width(), layout.height(rows, layout.deck_row())))
    }

    /// The hovered row's card image and where the row's middle is, down from the panel's top.
    pub fn preview(&self, cx: &App) -> Option<(f32, PathBuf)> {
        let index = self.hovered?;
        let rows = self.rows(cx);
        let image = self.art.get(&rows.get(index)?.card_id)?.full.clone();
        let layout = self.layout(cx);
        Some((layout.header() + (index as f32 + 0.5) * layout.deck_row(), image))
    }

    /// The game or the settings changed: look up the cards of the deck not yet known.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        cx.defer(overlay::sync);

        let Some(view) = &self.game.read(cx).view else {
            self.hovered = None;
            return;
        };
        let mut missing: Vec<i64> = view.deck.iter().copied().filter(|id| !self.asked.contains(id)).collect();
        missing.sort_unstable();
        missing.dedup();
        if missing.is_empty() || missing == self.loading {
            return;
        }
        self.loading = missing.clone();
        self.load = Some(cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            // Names first: they are one request, and the panel reads fine without the rest.
            let ids = missing.clone();
            let found = background.spawn(async move { cards::store().lookup(&ids) }).await;
            let images: Vec<(i64, String)> =
                found.values().filter(|c| !c.image.is_empty()).map(|c| (c.id, c.image.clone())).collect();
            // Scryfall is asked by id, and by name for the cards it does not know by id.
            let wanted: Vec<(i64, String)> = missing
                .iter()
                .map(|id| (*id, found.get(id).map(|card| card.name.clone()).unwrap_or_default()))
                .collect();
            let named = this.update(cx, |this, cx| {
                this.cards.extend(found);
                this.asked.extend(missing);
                this.loading.clear();
                cx.notify();
                // Two printings of a card become one row once both have a name.
                cx.defer(overlay::sync);
            });
            if named.is_err() {
                return;
            }
            // Then costs and colours, which also put the rows in their final order.
            let costs = background.spawn(async move { cards::store().costs(&wanted) }).await;
            let costed = this.update(cx, |this, cx| {
                this.costs.extend(costs);
                cx.notify();
            });
            if costed.is_err() {
                return;
            }
            // Art arrives card by card; the first time a card is seen it is a download.
            for (id, url) in images {
                let Some(art) = background.spawn(async move { cards::store().art(&url) }).await else { continue };
                let placed = this.update(cx, |this, cx| {
                    this.art.insert(id, art);
                    cx.notify();
                });
                if placed.is_err() {
                    return;
                }
            }
        }));
    }

    fn hover(&mut self, index: usize, hovered: bool, cx: &mut Context<Self>) {
        // Moving from one row to the next can report the new row before the old one's exit.
        let next = if hovered { Some(index) } else { self.hovered.filter(|&at| at != index) };
        if next != self.hovered {
            self.hovered = next;
            cx.defer(overlay::sync);
        }
    }
}

/// What a row shows for a card's colours: its mana cost, or for a card without one (a land)
/// a dot for each colour.
fn pips_for(cost: &Cost) -> Vec<Pip> {
    let pips = cards::pips(&cost.mana_cost);
    if pips.is_empty() { cost.colors.chars().map(Pip::Color).collect() } else { pips }
}

/// Fold neighbouring rows with the same name into one: two printings of a card are two
/// Arena ids, and one card to whoever is counting what is left.
fn merge(rows: impl Iterator<Item = Row>) -> Vec<Row> {
    let mut merged: Vec<Row> = Vec::new();
    for row in rows {
        match merged.last_mut() {
            Some(last) if last.name == row.name => {
                last.left += row.left;
                last.total += row.total;
                last.thumb = last.thumb.take().or(row.thumb);
            }
            _ => merged.push(row),
        }
    }
    merged
}

fn rarity_color(rarity: &str) -> u32 {
    match rarity {
        "mythic" => style::MYTHIC,
        "rare" => style::RARE,
        "uncommon" => style::UNCOMMON,
        _ => style::COMMON,
    }
}

impl Render for DeckPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.layout(cx);
        let s = layout.scale;
        let rows = self.rows(cx);
        let Some(view) = self.game.read(cx).view.as_ref() else { return div() };
        // What the counts are out of: the library itself when its size is known.
        let library = view.library.unwrap_or_else(|| rows.iter().map(|row| row.left).sum());

        let header = div()
            .h(px(layout.header()))
            .flex_none()
            .px(px(style::PADDING_X * s))
            .flex()
            .items_center()
            .gap(px(6. * s))
            .text_size(px(style::HEADER_TEXT * s))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Library"))
            .child(div().text_color(rgba(style::MUTED)).child(format!("{library} cards")))
            .child(div().flex_1())
            .child(div().text_color(rgba(style::MUTED)).child("Next draw · Left"));

        let rows = rows.into_iter().enumerate().map(|(index, row)| {
            let chance = if library > 0 && row.left > 0 {
                format!("{:.0}%", row.left as f64 / library as f64 * 100.)
            } else {
                String::new()
            };
            let pips = row.pips.into_iter().map(|pip| {
                let dot = div().size(px(10. * s)).flex_none().rounded_full();
                match pip {
                    Pip::Color(color) => dot.bg(rgba(style::pip(color))),
                    Pip::Other(text) => dot
                        .bg(rgba(style::PILL_HOVER))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(7.5 * s))
                        .line_height(px(10. * s))
                        .font_weight(FontWeight::BOLD)
                        .child(text),
                }
            });
            div()
                .id(("row", index))
                .h(px(layout.deck_row()))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8. * s))
                // None left: still listed, so the deck reads the same all game, but faint.
                .opacity(if row.left == 0 { 0.35 } else { 1. })
                .hover(|style| style.bg(rgba(style::HOVER)))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.hover(index, *hovered, cx)))
                .child(div().w(px(3.)).h_full().flex_none().bg(rgba(rarity_color(&row.rarity))))
                .child(
                    div()
                        .w(px((42. * s).round()))
                        .h(px((30. * s).round()))
                        .flex_none()
                        .rounded(px(4.))
                        .bg(rgba(style::THUMB))
                        .children(
                            row.thumb.map(|thumb| img(thumb).size_full().rounded(px(4.)).object_fit(ObjectFit::Cover)),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2. * s))
                        .child(
                            div()
                                .text_size(px(12. * s))
                                .line_height(px(14. * s))
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .child(row.name),
                        )
                        .child(
                            div().flex().items_center().gap(px(2. * s)).children(pips).child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .pl(px(2. * s))
                                    .text_size(px(10. * s))
                                    .line_height(px(12. * s))
                                    .text_color(rgba(style::MUTED))
                                    .child(row.type_line),
                            ),
                        ),
                )
                .child(div().flex_none().text_size(px(11. * s)).text_color(rgba(style::MUTED)).child(chance))
                .child(
                    div()
                        .w(px(14. * s))
                        .mr(px(style::PADDING_X * s))
                        .flex_none()
                        .flex()
                        .justify_end()
                        // The same size as the chance beside it; weight and colour tell them apart.
                        .text_size(px(11. * s))
                        .font_weight(FontWeight::BOLD)
                        .child(row.left.to_string()),
                )
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(style::RADIUS))
            .border_1()
            .border_color(rgba(style::BORDER))
            .bg(rgba(style::PANEL))
            .text_color(rgba(style::TEXT))
            .child(header)
            .children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cost, Pip, Row, merge, pips_for};

    fn row(card_id: i64, name: &str, left: usize, total: usize) -> Row {
        Row {
            card_id,
            name: name.to_string().into(),
            left,
            total,
            pips: Vec::new(),
            type_line: "".into(),
            rarity: String::new(),
            thumb: None,
        }
    }

    #[test]
    fn printings_of_one_card_are_one_row() {
        let rows = [row(1, "Island", 2, 3), row(2, "Spirebluff Canal", 1, 1), row(3, "Spirebluff Canal", 3, 3)];
        assert_eq!(merge(rows.into_iter()), [row(1, "Island", 2, 3), row(2, "Spirebluff Canal", 4, 4)]);
    }

    #[test]
    fn a_spell_shows_its_cost_and_a_land_its_colours() {
        let spell = Cost { mana_cost: "{1}{U}".into(), cmc: 2., colors: "U".into() };
        assert_eq!(pips_for(&spell), [Pip::Other("1".into()), Pip::Color('U')]);
        let land = Cost { mana_cost: String::new(), cmc: 0., colors: "WU".into() };
        assert_eq!(pips_for(&land), [Pip::Color('W'), Pip::Color('U')]);
    }
}

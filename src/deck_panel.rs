//! The deck panel: what is left in your library while you play, card by card, with the
//! chance that the next card off the top is each one.

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
    cost: Vec<Pip>,
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
                    cost: cost.map(|c| cards::pips(&c.mana_cost)).unwrap_or_default(),
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
            let spells: Vec<i64> =
                missing.iter().copied().filter(|id| !found.get(id).is_some_and(Card::is_land)).collect();
            let images: Vec<(i64, String)> =
                found.values().filter(|c| !c.image.is_empty()).map(|c| (c.id, c.image.clone())).collect();
            let named = this.update(cx, |this, cx| {
                this.cards.extend(found);
                this.asked.extend(missing);
                this.loading.clear();
                cx.notify();
            });
            if named.is_err() {
                return;
            }
            let costs = background.spawn(async move { cards::store().costs(&spells) }).await;
            if this.update(cx, |this, cx| (this.costs.extend(costs), cx.notify())).is_err() {
                return;
            }
            for (id, url) in images {
                let Some(art) = background.spawn(async move { cards::store().art(&url) }).await else { continue };
                if this.update(cx, |this, _| this.art.insert(id, art)).is_err() {
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

/// Fold neighbouring rows with the same name into one: two printings of a card are two
/// Arena ids, and one card to whoever is counting what is left.
fn merge(rows: impl Iterator<Item = Row>) -> Vec<Row> {
    let mut merged: Vec<Row> = Vec::new();
    for row in rows {
        match merged.last_mut() {
            Some(last) if last.name == row.name => {
                last.left += row.left;
                last.total += row.total;
            }
            _ => merged.push(row),
        }
    }
    merged
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
            .child(div().text_color(rgba(style::MUTED)).child("Next draw"));

        let rows = rows.into_iter().enumerate().map(|(index, row)| {
            let chance = if library > 0 && row.left > 0 {
                format!("{:.0}%", row.left as f64 / library as f64 * 100.)
            } else {
                String::new()
            };
            div()
                .id(("row", index))
                .h(px(layout.deck_row()))
                .flex_none()
                .px(px(style::PADDING_X * s))
                .flex()
                .items_center()
                .gap(px(6. * s))
                .text_size(px(12. * s))
                // None left: still listed, so the deck reads the same all game, but faint.
                .opacity(if row.left == 0 { 0.35 } else { 1. })
                .hover(|style| style.bg(rgba(style::HOVER)))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.hover(index, *hovered, cx)))
                .child(div().w(px(14. * s)).flex_none().font_weight(FontWeight::BOLD).child(row.left.to_string()))
                .child(div().flex_1().min_w_0().truncate().child(row.name))
                .child(div().flex_none().flex().items_center().gap(px(2. * s)).children(row.cost.into_iter().map(
                    |pip| {
                        let dot = div().size(px(11. * s)).flex_none().rounded_full();
                        match pip {
                            Pip::Color(color) => dot.bg(rgba(style::pip(color))),
                            Pip::Other(text) => dot
                                .bg(rgba(style::PILL_HOVER))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(8. * s))
                                .line_height(px(11. * s))
                                .font_weight(FontWeight::BOLD)
                                .child(text),
                        }
                    },
                )))
                .child(
                    div()
                        .w(px(30. * s))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .text_size(px(11. * s))
                        .text_color(rgba(style::MUTED))
                        .child(chance),
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
    use super::{Row, merge};

    fn row(card_id: i64, name: &str, left: usize, total: usize) -> Row {
        Row { card_id, name: name.to_string().into(), left, total, cost: Vec::new() }
    }

    #[test]
    fn printings_of_one_card_are_one_row() {
        let rows = [row(1, "Island", 2, 3), row(2, "Spirebluff Canal", 1, 1), row(3, "Spirebluff Canal", 3, 3)];
        assert_eq!(merge(rows.into_iter()), [row(1, "Island", 2, 3), row(2, "Spirebluff Canal", 4, 4)]);
    }
}

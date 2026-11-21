//! The draft panel: the pack you are picking from, ranked by 17Lands GIH WR.

use std::path::PathBuf;

use gpui_kit::*;

use crate::Shared;
use crate::cards::{self, Art, Card, Group};
use crate::draft::Draft;
use crate::feed::Pack;
use crate::overlay::{self, Shape};
use crate::settings::Settings;
use crate::style::{self, Layout};

struct Row {
    card_id: i64,
    card: Option<Card>,
    /// The win rate's distance from the set's mean, in standard deviations.
    z: Option<f64>,
    art: Option<Art>,
}

pub struct DraftPanel {
    draft: Entity<Draft>,
    prefs: Entity<Settings>,
    rows: Vec<Row>,
    /// The pack and player group `rows` are for. It lags the draft and the settings while
    /// ratings load: the old rows stay up until the new ones are ready.
    shown: Option<(Pack, Group)>,
    /// What is being loaded, so a second request for the same thing starts nothing.
    loading: Option<(Pack, Group)>,
    /// The format the ratings are for, when it is not the draft's own.
    note: Option<SharedString>,
    hovered: Option<usize>,
    /// Dropping it cancels the load.
    load: Option<Task<()>>,
    _subscriptions: [Subscription; 2],
}

impl DraftPanel {
    pub fn new(draft: Entity<Draft>, prefs: Entity<Settings>, cx: &mut Context<Self>) -> Self {
        let subscriptions =
            [cx.observe(&draft, |this, _, cx| this.refresh(cx)), cx.observe(&prefs, |this, _, cx| this.refresh(cx))];
        let mut panel = Self {
            draft,
            prefs,
            rows: Vec::new(),
            shown: None,
            loading: None,
            note: None,
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

    /// What the panel's window should be, or `None` when it has nothing to show.
    pub fn shape(&self, cx: &App) -> Option<Shape> {
        let prefs = self.prefs.read(cx);
        if !prefs.draft_overlay || self.shown.is_none() || self.rows.is_empty() {
            return None;
        }
        if prefs.draft_collapsed {
            return Some(Shape::Tab);
        }
        let layout = self.layout(cx);
        Some(Shape::Open(layout.width(), layout.height(self.rows.len(), layout.draft_row())))
    }

    /// The hovered row's card image and where the row's middle is, down from the panel's top.
    pub fn preview(&self, cx: &App) -> Option<(f32, PathBuf)> {
        if self.prefs.read(cx).draft_collapsed {
            return None;
        }
        let index = self.hovered?;
        let image = self.rows.get(index)?.art.as_ref()?.full.clone();
        let layout = self.layout(cx);
        Some((layout.header() + (index as f32 + 0.5) * layout.draft_row(), image))
    }

    /// The draft or the settings changed: load the rows for the pack that is waiting.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        // The size setting may be what changed.
        cx.notify();
        cx.defer(overlay::sync);
        // Its rows are gone, and with them the event that would say the pointer left one.
        if self.prefs.read(cx).draft_collapsed {
            self.hovered = None;
        }

        let group = self.prefs.read(cx).group;
        let Some(pack) = self.draft.read(cx).pack.clone() else {
            self.load = None;
            self.loading = None;
            self.shown = None;
            self.rows.clear();
            self.hovered = None;
            return;
        };
        let target = (pack, group);
        if self.shown.as_ref() == Some(&target) || self.loading.as_ref() == Some(&target) {
            return;
        }
        self.loading = Some(target.clone());
        self.load = Some(cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let (rows, note) = background
                .spawn({
                    let (pack, group) = target.clone();
                    async move { rows_for(&pack, group) }
                })
                .await;
            let images: Vec<(usize, String)> = rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| Some((index, row.card.as_ref()?.image.clone())))
                .filter(|(_, url)| !url.is_empty())
                .collect();
            let shown = this.update(cx, |this, cx| {
                this.rows = rows;
                this.note = note.map(Into::into);
                this.shown = Some(target);
                this.loading = None;
                this.hovered = None;
                cx.notify();
                cx.defer(overlay::sync);
            });
            if shown.is_err() {
                return;
            }
            // Art arrives row by row; the first time a set is drafted each one is a download.
            for (index, url) in images {
                let Some(art) = background.spawn(async move { cards::store().art(&url) }).await else { continue };
                let placed = this.update(cx, |this, cx| {
                    if let Some(row) = this.rows.get_mut(index) {
                        row.art = Some(art);
                        cx.notify();
                    }
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

    /// Clicking the header switches to the next player group, and remembers it.
    fn cycle_group(&mut self, cx: &mut Context<Self>) {
        let state = cx.global::<Shared>().state.clone();
        if let Err(err) = state.update_settings(|settings| settings.group = settings.group.next()) {
            log::warn!(target: "brainstorm", "{err}");
        }
    }

    /// Collapse the panel to its tab, or open it again, and remember which.
    fn toggle(&mut self, cx: &mut Context<Self>) {
        let state = cx.global::<Shared>().state.clone();
        if let Err(err) = state.update_settings(|settings| settings.draft_collapsed = !settings.draft_collapsed) {
            log::warn!(target: "brainstorm", "{err}");
        }
    }
}

/// The pack's rows, best win rate first, and a note when the ratings are another format's.
/// Blocks on 17Lands the first time a set and group is asked for.
fn rows_for(pack: &Pack, group: Group) -> (Vec<Row>, Option<String>) {
    let store = cards::store();
    let event = pack.format_and_set();
    let ratings = event.and_then(|(format, set)| store.ratings(set, format, group));
    let note = ratings
        .as_ref()
        .zip(event)
        .filter(|(ratings, (format, _))| ratings.format != *format)
        .map(|(ratings, _)| format!("{} data", ratings.format));

    // The ratings table omits some cards (basic lands): look those up so they still show.
    let rated = |id: &i64| ratings.as_ref().and_then(|r| r.by_id.get(id)).cloned();
    let unrated: Vec<i64> = pack.card_ids.iter().copied().filter(|id| rated(id).is_none()).collect();
    let extras = if unrated.is_empty() { Default::default() } else { store.lookup(&unrated) };

    let mut rows: Vec<Row> = pack
        .card_ids
        .iter()
        .map(|id| {
            let card = rated(id).or_else(|| extras.get(id).cloned());
            let z = card.as_ref().and_then(|c| c.win_rate).zip(ratings.as_ref()).map(|(rate, r)| r.z(rate));
            Row { card_id: *id, card, z, art: None }
        })
        .collect();
    let key = |row: &Row| {
        let card = row.card.as_ref();
        (card.and_then(|c| c.win_rate).unwrap_or(-1.), card.map(|c| c.name.clone()).unwrap_or_default())
    };
    rows.sort_by(|a, b| {
        let ((rate_a, name_a), (rate_b, name_b)) = (key(a), key(b));
        rate_b.total_cmp(&rate_a).then(name_a.cmp(&name_b))
    });
    (rows, note)
}

fn rarity_color(rarity: &str) -> u32 {
    match rarity {
        "mythic" => style::MYTHIC,
        "rare" => style::RARE,
        "uncommon" => style::UNCOMMON,
        _ => style::COMMON,
    }
}

/// Tint by distance from the set's mean win rate.
fn grade_color(z: Option<f64>) -> u32 {
    let Some(z) = z else { return style::MUTED };
    style::GRADES.iter().find(|(at_least, _)| z >= *at_least).map_or(style::GRADE_WORST, |(_, color)| *color)
}

impl Render for DraftPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.layout(cx);
        let s = layout.scale;
        let Some((pack, shown_group)) = self.shown.clone() else { return div().into_any_element() };
        if self.prefs.read(cx).draft_collapsed {
            return overlay::tab().on_click(cx.listener(|this, _, _, cx| this.toggle(cx))).into_any_element();
        }
        let group = self.prefs.read(cx).group;

        let header = div()
            .id("header")
            .group("header")
            .h(px(layout.header()))
            .flex_none()
            .pl(px(style::PADDING_X * s))
            .pr(px(style::HEADER_END * s))
            .flex()
            .items_center()
            .gap(px(6. * s))
            .text_size(px(style::HEADER_TEXT * s))
            .on_click(cx.listener(|this, _, _, cx| this.cycle_group(cx)))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Pack {} · Pick {}", pack.pack, pack.pick)),
            )
            .children(
                self.note.clone().map(|note| div().min_w_0().truncate().text_color(rgba(style::NOTE)).child(note)),
            )
            .child(div().flex_1())
            // Whose ratings these are. The whole header is the control; this shows that it
            // is one. Dimmed until the new group's ratings are in, so the numbers never
            // claim the wrong group.
            .child(
                div()
                    .flex_none()
                    .px(px(6. * s))
                    .py(px(2. * s))
                    .rounded(px(4. * s))
                    .bg(rgba(style::PILL))
                    .text_color(rgba(style::MUTED))
                    .group_hover("header", |style| style.bg(rgba(style::PILL_HOVER)).text_color(rgba(style::TEXT)))
                    .opacity(if shown_group == group { 1. } else { 0.45 })
                    .child(group.label()),
            )
            .child(div().flex_none().text_color(rgba(style::MUTED)).child("GIH WR"))
            // The one part of the header that does not switch the group.
            .child(overlay::collapse_button(s).on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.toggle(cx);
            })));

        let rows = self.rows.iter().enumerate().map(|(index, row)| {
            let card = row.card.as_ref();
            let name: SharedString = card.map_or_else(|| format!("Card {}", row.card_id), |c| c.name.clone()).into();
            let win_rate =
                card.and_then(|c| c.win_rate).map_or("–".to_string(), |rate| format!("{:.1}%", rate * 100.));
            let pips = card.map(|c| c.color.clone()).unwrap_or_default();
            let type_line: SharedString = card.map(|c| c.type_line().to_string()).unwrap_or_default().into();
            div()
                .id(("row", index))
                .h(px(layout.draft_row()))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8. * s))
                .hover(|style| style.bg(rgba(style::HOVER)))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| this.hover(index, *hovered, cx)))
                .child(div().w(px(3.)).h_full().flex_none().bg(rgba(rarity_color(card.map_or("", |c| &c.rarity)))))
                .child(
                    div()
                        .w(px((52. * s).round()))
                        .h(px((38. * s).round()))
                        .flex_none()
                        .rounded(px(4.))
                        .bg(rgba(style::THUMB))
                        .children(row.art.as_ref().map(|art| {
                            img(art.thumb.clone()).size_full().rounded(px(4.)).object_fit(ObjectFit::Cover)
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3. * s))
                        .child(
                            div()
                                .text_size(px(12.5 * s))
                                .line_height(px(15. * s))
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .child(name),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4. * s))
                                .children(pips.chars().map(|pip| {
                                    div().size(px(8. * s)).flex_none().rounded_full().bg(rgba(style::pip(pip)))
                                }))
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(10. * s))
                                        .line_height(px(12. * s))
                                        .text_color(rgba(style::MUTED))
                                        .child(type_line),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .pr(px(style::PADDING_X * s))
                        .text_size(px(15. * s))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(grade_color(row.z)))
                        .child(win_rate),
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
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{grade_color, style};

    #[test]
    fn grades_follow_the_distance_from_the_mean() {
        assert_eq!(grade_color(Some(2.)), style::GRADES[0].1);
        assert_eq!(grade_color(Some(0.5)), style::GRADES[1].1);
        assert_eq!(grade_color(Some(0.)), style::GRADES[2].1);
        assert_eq!(grade_color(Some(-1.)), style::GRADES[3].1);
        assert_eq!(grade_color(Some(-2.)), style::GRADE_WORST);
        assert_eq!(grade_color(None), style::MUTED);
    }
}

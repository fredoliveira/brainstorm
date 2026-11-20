//! The draft as the overlay sees it: the pack waiting for a pick, if there is one.

use crate::feed::{DraftEvent, Pack};

#[derive(Default)]
pub struct Draft {
    pub pack: Option<Pack>,
}

impl Draft {
    pub fn apply(&mut self, event: DraftEvent) {
        match event {
            DraftEvent::Pack(pack) => self.pack = Some(pack),
            DraftEvent::Pick { pack, pick } => {
                if self.pack.as_ref().is_some_and(|p| p.pack == pack && p.pick == pick) {
                    self.pack = None;
                }
            }
            DraftEvent::Complete => self.pack = None,
        }
    }

    /// A made-up pack of real cards, for working on the panel without a draft
    /// (`BRAINSTORM_DEMO`).
    pub fn demo() -> Self {
        Self {
            pack: Some(Pack {
                event_name: "PremierDraft_TRK_20261103".into(),
                pack: 1,
                pick: 1,
                card_ids: DEMO_CARDS.to_vec(),
            }),
        }
    }
}

/// Fourteen cards from one set, by Arena id.
pub const DEMO_CARDS: [i64; 14] =
    [106743, 106655, 106780, 106958, 106602, 106615, 106852, 106626, 106765, 106876, 106607, 106837, 106687, 106597];

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(pack: i64, pick: i64) -> Pack {
        Pack { event_name: "PremierDraft_HOB_20260811".into(), pack, pick, card_ids: vec![1, 2, 3] }
    }

    #[test]
    fn a_pick_clears_only_its_own_pack() {
        let mut draft = Draft::default();
        draft.apply(DraftEvent::Pack(pack(1, 2)));
        // The pick for the pack before arrives late, after the next pack is already showing.
        draft.apply(DraftEvent::Pick { pack: 1, pick: 1 });
        assert_eq!(draft.pack, Some(pack(1, 2)));
        draft.apply(DraftEvent::Pick { pack: 1, pick: 2 });
        assert_eq!(draft.pack, None);
    }

    #[test]
    fn finishing_the_draft_clears_the_pack() {
        let mut draft = Draft::default();
        draft.apply(DraftEvent::Pack(pack(3, 14)));
        draft.apply(DraftEvent::Complete);
        assert_eq!(draft.pack, None);
    }

    #[test]
    fn event_names_split_into_format_and_set() {
        assert_eq!(pack(1, 1).format_and_set(), Some(("PremierDraft", "HOB")));
    }
}

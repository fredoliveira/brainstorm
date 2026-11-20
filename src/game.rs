//! The game as the deck overlay sees it: `recall-core`'s view of the game in progress.

use recall_core::live::GameView;

use crate::draft::DEMO_CARDS;

#[derive(Default)]
pub struct Game {
    pub view: Option<GameView>,
}

impl Game {
    /// A made-up game a few turns in, for working on the panel without one
    /// (`BRAINSTORM_DEMO=game`).
    pub fn demo() -> Self {
        // Two copies of some cards, one of the rest.
        let deck: Vec<i64> =
            DEMO_CARDS.iter().enumerate().flat_map(|(i, id)| vec![*id; 1 + (i % 3 == 0) as usize]).collect();
        let seen = vec![DEMO_CARDS[0], DEMO_CARDS[0], DEMO_CARDS[1], DEMO_CARDS[4], DEMO_CARDS[6], DEMO_CARDS[9]];
        Self {
            view: Some(GameView {
                seat: Some(1),
                library: Some(deck.len() - seen.len()),
                hand: seen[..3].to_vec(),
                seen,
                deck,
                turn: 4,
                ..GameView::default()
            }),
        }
    }
}

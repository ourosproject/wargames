//! The card library registry — the streamlined "add an attack/defense" mechanism.
//!
//! Register a card once and it's instantly available everywhere: the referee's legal-move
//! menu, the thin model's choices, the dashboard, and (later) the node-based builder.
//! No giant match statements to grow — adding a card touches exactly one line.

use crate::card::{Card, CardSpec};
use crate::state::{GameState, Side};

#[derive(Default)]
pub struct CardRegistry {
    cards: Vec<Box<dyn Card>>,
}

impl CardRegistry {
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    /// Add a card to the library. This is the whole "add an attack or defense" step.
    pub fn register(&mut self, card: Box<dyn Card>) -> &mut Self {
        self.cards.push(card);
        self
    }

    pub fn get(&self, id: &str) -> Option<&dyn Card> {
        self.cards.iter().map(|c| c.as_ref()).find(|c| c.id() == id)
    }

    /// The legal-move menu for `side` in `state` — the only cards the thin model may pick.
    pub fn legal(&self, side: Side, state: &GameState) -> Vec<CardSpec> {
        self.cards
            .iter()
            .filter(|c| c.side() == side && c.precondition(state))
            .map(|c| c.spec())
            .collect()
    }

    /// Full catalog — for the dashboard / node-builder to render the palette.
    pub fn all_specs(&self) -> Vec<CardSpec> {
        self.cards.iter().map(|c| c.spec()).collect()
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

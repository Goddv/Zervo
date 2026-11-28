//! Transient New Tab prompt, transcript and generated preview.

use crate::newtab_custom::Preview;
use crate::state::TabId;

pub const PROMPT_LIMIT: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dictation {
    Starting,
    Listening,
    Finishing,
}

#[derive(Default)]
pub struct Composer {
    pub tab: Option<TabId>,
    pub generation: bool,
    pub loading_photo: bool,
    pub dictation: Option<Dictation>,
    pub prompt: String,
    pub preview: Option<Preview>,
    pub issue: Option<&'static str>,
    pub focus_prompt: bool,
    pub ticket: u64,
}

impl Composer {
    pub fn close(&mut self) {
        *self = Self {
            ticket: self.ticket.wrapping_add(1),
            ..Self::default()
        };
    }

    pub fn busy(&self) -> bool {
        self.generation || self.dictation.is_some()
    }
}

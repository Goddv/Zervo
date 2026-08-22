//! Web notifications, drawn as toasts in the corner of the page.
//!
//! Servo dispatches `new Notification(...)` to the embedder already
//! (`EmbedderMsg::ShowNotification`); all that was ever missing was somewhere
//! for one to land. There is no system-notification integration here — that
//! wants a signed bundle and `UNUserNotificationCenter` on macOS, and an
//! equivalent on each other platform — so a notification is shown inside the
//! window, over the page that raised it.
//!
//! Nothing in here is modal. `controls.rs` holds the things a page is *waiting*
//! on and draws one at a time; a notification is told, not asked, so several can
//! stack and the page carries on regardless.

use std::time::{Duration, Instant};

/// How long a notification stays up when it has not asked to stay.
const LINGER: Duration = Duration::from_secs(6);

/// How many are kept at all before the oldest is pushed off.
///
/// The spec's own answer to a flood is the `tag` field, which replaces rather
/// than adds; this is the backstop for pages that do not use it.
const HISTORY: usize = 20;

/// How many are drawn at once while the tray is open. Beyond this the stack
/// would run off the bottom of the window anyway.
const SHOWN: usize = 6;

pub struct Toast {
    /// Minted here and never reused, so the chrome can key a toast's widgets on
    /// something that survives the one above it being dismissed. Positional ids
    /// have bitten this codebase before.
    pub id: u64,
    pub title: String,
    pub body: String,
    /// The page's own identifier for this notification. A second notification
    /// with the same tag replaces the first rather than joining it.
    pub tag: String,
    /// `requireInteraction` — stay until dismissed rather than timing out.
    pub sticky: bool,
    raised: Instant,
}

/// A toast as the chrome sees it: no clock and nothing borrowed, so the draw
/// pass does not have to hold a `Ref` on the collection while it runs.
#[derive(Clone)]
pub struct ToastView {
    pub id: u64,
    pub title: String,
    pub body: String,
}

/// What the chrome is given each frame: the notifications to draw, how many
/// are being kept, and whether the reader has opened the tray.
pub struct View<'a> {
    pub visible: &'a [ToastView],
    pub count: usize,
    pub open: bool,
}

#[derive(Default)]
pub struct Toasts {
    /// Newest last. Kept after they stop showing themselves, so the bell has
    /// something to open onto — a notification you missed by six seconds is
    /// exactly the one worth going back for.
    items: Vec<Toast>,
    next_id: u64,
    /// Whether the reader has opened the tray from the bell.
    open: bool,
}

impl Toasts {
    /// Show a notification, or replace the one it supersedes.
    pub fn raise(&mut self, title: String, body: String, tag: String, sticky: bool, now: Instant) {
        self.next_id += 1;
        let toast = Toast {
            id: self.next_id,
            title,
            body,
            tag,
            sticky,
            raised: now,
        };

        // An empty tag is not an identifier — the spec says so, and treating it
        // as one would make every untagged notification replace the last.
        if !toast.tag.is_empty() {
            // Replacing makes it new again — it is the current state of that
            // conversation, not an old one being edited in place.
            self.items.retain(|old| old.tag != toast.tag);
        }

        self.items.push(toast);
        if self.items.len() > HISTORY {
            self.items.remove(0);
        }
    }

    /// How many are being kept — the number on the bell.
    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Clear everything, which is what the bell's own menu is for.
    pub fn clear(&mut self) {
        self.items.clear();
        self.open = false;
    }

    /// Whether a notification is still showing itself unasked.
    fn lingering(toast: &Toast, now: Instant) -> bool {
        toast.sticky || now.duration_since(toast.raised) < LINGER
    }

    /// When the next toast expires, so the event loop can sleep until then
    /// rather than polling. `None` when nothing is waiting on the clock.
    ///
    /// Only the ones still counting down have a deadline; once a notification
    /// has lingered out it sits in the tray with no clock left to run, and a
    /// scheduler woken by those would never idle.
    pub fn next_deadline(&self, now: Instant) -> Option<Instant> {
        self.items
            .iter()
            .filter(|toast| !toast.sticky && Self::lingering(toast, now))
            .map(|toast| toast.raised + LINGER)
            .min()
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|toast| toast.id != id);
        if self.items.is_empty() {
            self.open = false;
        }
    }

    /// Snapshot for the draw pass, newest first: what should be on screen right
    /// now, which is everything while the tray is open and only what is still
    /// lingering while it is shut.
    ///
    /// Allocates nothing in the common case of having no notifications at all.
    pub fn view(&self, now: Instant) -> Vec<ToastView> {
        self.items
            .iter()
            .rev()
            .filter(|toast| self.open || Self::lingering(toast, now))
            .take(SHOWN)
            .map(|toast| ToastView {
                id: toast.id,
                title: toast.title.clone(),
                body: toast.body.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin() -> Instant {
        Instant::now()
    }

    fn titles(toasts: &Toasts, now: Instant) -> Vec<String> {
        toasts
            .view(now)
            .into_iter()
            .map(|toast| toast.title)
            .collect()
    }

    fn raise(toasts: &mut Toasts, title: &str, tag: &str, sticky: bool, now: Instant) {
        toasts.raise(title.into(), String::new(), tag.into(), sticky, now);
    }

    /// A notification with a tag replaces the one it supersedes rather than
    /// stacking on top of it — which is the entire point of the tag.
    #[test]
    fn a_tag_replaces_rather_than_stacks() {
        let now = origin();
        let mut toasts = Toasts::default();
        raise(&mut toasts, "One", "chat", false, now);
        raise(&mut toasts, "Two", "chat", false, now);
        assert_eq!(titles(&toasts, now), vec!["Two"]);
    }

    /// An empty tag is not an identifier, so untagged notifications must not
    /// all collapse onto each other.
    #[test]
    fn an_empty_tag_does_not_replace() {
        let now = origin();
        let mut toasts = Toasts::default();
        raise(&mut toasts, "One", "", false, now);
        raise(&mut toasts, "Two", "", false, now);
        assert_eq!(toasts.count(), 2);
    }

    /// A replaced notification gets the full linger again, otherwise a chat
    /// that updates its tag every few seconds would vanish mid-conversation.
    #[test]
    fn replacing_restarts_the_clock() {
        let start = origin();
        let mut toasts = Toasts::default();
        raise(&mut toasts, "One", "chat", false, start);
        raise(&mut toasts, "Two", "chat", false, start + LINGER);
        let later = start + LINGER + Duration::from_secs(1);
        assert_eq!(titles(&toasts, later), vec!["Two"]);
    }

    /// A sticky notification is one the page asked to keep until answered, so
    /// no amount of time may stop it showing.
    #[test]
    fn sticky_notifications_never_time_out() {
        let start = origin();
        let mut toasts = Toasts::default();
        raise(&mut toasts, "Keep", "", true, start);
        raise(&mut toasts, "Go", "", false, start);
        assert_eq!(titles(&toasts, start + LINGER * 10), vec!["Keep"]);
    }

    /// Timing out hides a notification but does not throw it away — the whole
    /// reason the bell has anything to open onto.
    #[test]
    fn lingering_out_hides_but_keeps() {
        let start = origin();
        let mut toasts = Toasts::default();
        raise(&mut toasts, "Missed", "", false, start);
        let later = start + LINGER + Duration::from_secs(1);
        assert!(titles(&toasts, later).is_empty());
        assert_eq!(toasts.count(), 1);

        toasts.toggle();
        assert_eq!(titles(&toasts, later), vec!["Missed"]);
    }

    /// Newest first, so the tray reads top-down in the order things happened.
    #[test]
    fn the_newest_is_shown_first() {
        let now = origin();
        let mut toasts = Toasts::default();
        raise(&mut toasts, "First", "", false, now);
        raise(&mut toasts, "Second", "", false, now);
        assert_eq!(titles(&toasts, now), vec!["Second", "First"]);
    }

    /// The deadline is the *soonest* pending expiry. Sticky notifications have
    /// none, and neither do ones that have already lingered out — a scheduler
    /// woken by either would spin forever.
    #[test]
    fn only_notifications_still_counting_down_have_a_deadline() {
        let start = origin();
        let mut toasts = Toasts::default();
        raise(
            &mut toasts,
            "Late",
            "",
            false,
            start + Duration::from_secs(5),
        );
        raise(&mut toasts, "Soon", "", false, start);
        assert_eq!(toasts.next_deadline(start), Some(start + LINGER));

        // Both have lingered out by now, so there is nothing left to wake for.
        assert_eq!(toasts.next_deadline(start + LINGER * 10), None);

        let mut sticky = Toasts::default();
        raise(&mut sticky, "Keep", "", true, start);
        assert_eq!(sticky.next_deadline(start), None);
    }

    /// Ids are never reused, so dismissing one toast cannot take another with
    /// it and the chrome can key state on an id safely.
    #[test]
    fn ids_are_unique_and_dismissal_takes_only_its_own() {
        let now = origin();
        let mut toasts = Toasts::default();
        for i in 0..3 {
            raise(&mut toasts, &format!("{i}"), "", true, now);
        }
        let ids: Vec<u64> = toasts.view(now).iter().map(|toast| toast.id).collect();
        assert_eq!(ids.len(), 3);
        toasts.dismiss(ids[1]);
        let left: Vec<u64> = toasts.view(now).iter().map(|toast| toast.id).collect();
        assert_eq!(left, vec![ids[0], ids[2]]);
    }

    /// A page that ignores tags cannot make the history grow without bound.
    #[test]
    fn the_history_is_capped_and_drops_the_oldest() {
        let now = origin();
        let mut toasts = Toasts::default();
        for i in 0..HISTORY + 2 {
            raise(&mut toasts, &format!("{i}"), "", true, now);
        }
        assert_eq!(toasts.count(), HISTORY);
        toasts.toggle();
        // "0" and "1" fell off the front; the newest is what shows.
        assert_eq!(titles(&toasts, now)[0], format!("{}", HISTORY + 1));
    }

    /// Dismissing the last one shuts the tray, so the bell cannot be left open
    /// over nothing.
    #[test]
    fn emptying_the_tray_closes_it() {
        let now = origin();
        let mut toasts = Toasts::default();
        raise(&mut toasts, "Only", "", true, now);
        toasts.toggle();
        assert!(toasts.is_open());
        let id = toasts.view(now)[0].id;
        toasts.dismiss(id);
        assert!(!toasts.is_open());
        assert_eq!(toasts.count(), 0);
    }
}

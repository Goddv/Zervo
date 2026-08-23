//! The four pages a browser on a young engine shows most.
//!
//! Turn 7b of the study calls `zervo://unsupported` "the page Zervo will show
//! more than any other browser", and it is right: the README's *Limitations*
//! and `docs/PARITY.md` between them describe an engine that will refuse a
//! good deal of the web for a while yet. A browser that meets that with a
//! blank rectangle is lying by omission.
//!
//! Four pages, because there are four different things going wrong and only
//! one of them is a wait:
//!
//! - **Unsupported** — the engine cannot do it. Nothing the reader can press
//!   will change that, so the page says so plainly, offers the one thing that
//!   does work (another browser), and gives the wait a shape.
//! - **Offline** — resolves itself. It names what is queued rather than what
//!   failed, and has no button at all.
//! - **Certificate** — a decision, not a wait. 7b: "the one error page that
//!   must not be candy. Same material, no glow, no game, and the proceed link
//!   is plain text at the bottom."
//! - **Not found** — has an answer. The library already ranks hosts by visits,
//!   so a typo usually has an obvious correction sitting right there; offering
//!   it beats reporting the failure.
//!
//! Each is reachable by typing its address, and each carries what it knows in
//! the query string — `zervo://unsupported?host=…&needs=…`. That is the same
//! rule the other internal pages follow: every one of them is shown in the
//! address bar, so every one has to be something you can type back into it.
//! It is also how the engine will hand over the detail on the day it can: the
//! page reads its own address rather than a side channel.

use egui::{Align2, Color32, FontId, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::glass::{self, Glass};
use crate::icons::{self, Icon};
use crate::theme::{self, Palette, Tier};
use crate::ui::{ChromeContext, UiAction};

/// Which kind of trouble this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trouble {
    /// The engine has not implemented what the page needs.
    Unsupported,
    /// There is no network.
    Offline,
    /// The certificate does not match the site it was served for.
    Certificate,
    /// The host does not resolve.
    NotFound,
    /// The engine's content process fell over on this page.
    ///
    /// Not one of turn 7b's four, and here because `docs/PARITY.md` asked for
    /// it by name — "`notify_crashed` is not implemented, so a crashed content
    /// process is a frozen page with no explanation" — and `docs/TODO.md`
    /// answered its own question: "Even an error page beats that." It is the
    /// only failure this engine version reports at all, so it is also the only
    /// one of these five that arrives on its own rather than by being typed.
    ///
    /// It is deliberately not folded into [`Trouble::Unsupported`]. A gap is a
    /// wait and a crash is a bug; telling somebody the engine has not built a
    /// feature yet, when what actually happened is that it fell over on a page
    /// it renders perfectly well most days, is a browser lying to be tidy.
    Crashed,
}

impl Trouble {
    pub const ALL: [Trouble; 5] = [
        Trouble::Unsupported,
        Trouble::Offline,
        Trouble::Certificate,
        Trouble::NotFound,
        Trouble::Crashed,
    ];

    /// The authority of its `zervo://` address.
    pub fn slug(self) -> &'static str {
        match self {
            Trouble::Unsupported => "unsupported",
            Trouble::Offline => "offline",
            Trouble::Certificate => "certificate",
            Trouble::NotFound => "notfound",
            Trouble::Crashed => "crashed",
        }
    }

    pub fn url(self) -> String {
        format!("zervo://{}", self.slug())
    }

    /// What the tab is called. Short, because it is read in a 226-point column
    /// beside a favicon — "Servo can't render this page yet" is the headline,
    /// not the tab.
    pub fn title(self) -> &'static str {
        match self {
            Trouble::Unsupported => "Can't render this",
            Trouble::Offline => "Offline",
            Trouble::Certificate => "Not private",
            Trouble::NotFound => "No such site",
            Trouble::Crashed => "Page stopped",
        }
    }

    /// Recognise one of these addresses, whatever it carries in its query.
    pub fn from_url(url: &str) -> Option<Trouble> {
        let rest = url.strip_prefix("zervo://")?;
        let authority = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        Trouble::ALL
            .into_iter()
            .find(|trouble| trouble.slug() == authority)
    }

    pub fn icon(self) -> Icon {
        match self {
            Trouble::Unsupported => Icon::Warning,
            Trouble::Offline => Icon::Globe,
            Trouble::Certificate => Icon::Lock,
            Trouble::NotFound => Icon::Search,
            Trouble::Crashed => Icon::XCircle,
        }
    }

    /// The one colour the page is allowed. Four different meanings, four
    /// colours: a wait, a fact, a decision, a mistake.
    fn tint(self, palette: &Palette) -> Color32 {
        match self {
            Trouble::Unsupported => palette.warning,
            Trouble::Offline => palette.info,
            Trouble::Certificate => palette.danger,
            Trouble::NotFound => palette.accent,
            Trouble::Crashed => palette.danger,
        }
    }

    /// The colour the address bar's badge takes for this page. Public because
    /// the badge is drawn by the chrome, not by the page.
    pub fn badge_tint(self, palette: &Palette) -> Color32 {
        theme::mix(palette.text_muted, self.tint(palette), 0.8)
    }

    fn headline(self) -> &'static str {
        match self {
            Trouble::Unsupported => "Servo can't render this page yet",
            Trouble::Offline => "You're offline",
            Trouble::Certificate => "This connection isn't private",
            Trouble::NotFound => "No such site",
            Trouble::Crashed => "This page stopped",
        }
    }

    /// Whether this page is a wait. Only a wait gets the game — 7b is explicit
    /// about it: "Offline resolves itself, a bad certificate is a decision, and
    /// a typo has an answer; none of those is a wait, so none of them gets a
    /// game."
    fn is_a_wait(self) -> bool {
        self == Trouble::Unsupported
    }
}

/// What the address said, beyond which page it is.
///
/// Everything here is optional because the address can be typed by hand, and a
/// page that only reads well when the engine filled it in is a page nobody can
/// look at. Each field has a truthful fallback.
struct Told {
    /// The site that would not load.
    host: Option<String>,
    /// What it needed and did not get, or which host the certificate was for.
    detail: Option<String>,
}

impl Told {
    fn read(url: &str) -> Told {
        let query = url.split_once('?').map(|(_, rest)| rest).unwrap_or("");
        let mut told = Told {
            host: None,
            detail: None,
        };
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decode(value);
            if value.is_empty() {
                continue;
            }
            match key {
                "host" => told.host = Some(value),
                "needs" | "cert" | "detail" => told.detail = Some(value),
                _ => {},
            }
        }
        told
    }
}

/// The little of percent-decoding a query string of our own making needs.
///
/// Not a general decoder and not trying to be: these addresses are written by
/// Zervo and read by Zervo, and the only characters that have to survive the
/// round trip are the space and the percent itself.
fn percent_decode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        match byte {
            b'+' => out.push(' '),
            b'%' => {
                let hex: String = bytes.by_ref().take(2).map(char::from).collect();
                match u8::from_str_radix(&hex, 16) {
                    Ok(decoded) => out.push(char::from(decoded)),
                    // Not a valid escape: keep it as it was typed rather than
                    // dropping characters out of the middle of a hostname.
                    Err(_) => {
                        out.push('%');
                        out.push_str(&hex);
                    },
                }
            },
            _ => out.push(char::from(byte)),
        }
    }
    out
}

/// Percent-encode a value for one of these addresses.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            },
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The address for a page that failed, with what is known about it.
///
/// One caller today — `AppState::notify_crashed`, which is the only failure
/// this engine version reports. The other four pages wait on hooks Servo 0.5.0
/// does not have: there is no load-failure callback and no certificate
/// callback, so nothing can tell the embedder why a load ended.
pub fn address(trouble: Trouble, host: &str, detail: &str) -> String {
    let mut url = trouble.url();
    let mut sep = '?';
    for (key, value) in [("host", host), ("detail", detail)] {
        if !value.is_empty() {
            url.push(sep);
            url.push_str(key);
            url.push('=');
            url.push_str(&encode(value));
            sep = '&';
        }
    }
    url
}

/// Where reload goes on one of these pages: the site that would not load.
///
/// `None` when the address carries no host — a page reached by typing
/// `zervo://offline` has nothing to try again, and a reload that silently
/// reopened the error page would look exactly like a reload that failed.
pub fn retry_target(url: &str) -> Option<String> {
    let host = Told::read(url).host?;
    (!host.is_empty()).then(|| format!("https://{host}"))
}

// ── The page ───────────────────────────────────────────────────────────────

/// How wide the column gets. 7b's own 620.
const COLUMN: f32 = 620.0;
/// The gap between the message and the game under it.
const GAP: f32 = 18.0;
/// The game panel's height, from the artboard.
const GAME_HEIGHT: f32 = 230.0;

pub fn draw(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    trouble: Trouble,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    // The same ground the new tab page lays down, for the same reason: at the
    // first seam it is an opaque base of the page's own, and past that it is a
    // veil over the window's light. An error page that painted itself a flat
    // grey would be the one surface in the browser that had opted out of the
    // seam.
    root.ctx()
        .layer_painter(egui::LayerId::background())
        .rect_filled(
            content_rect,
            theme::content_corners(&palette),
            theme::page_ground(&palette),
        );

    let (tab, address) = chrome
        .browser
        .active_tab()
        .map(|tab| (tab.id, tab.url.clone()))
        .unwrap_or_default();
    let told = Told::read(&address);

    // The floor is what keeps the card readable in a narrow window; the cap is
    // what keeps it *on* the page. Without the second, a window narrow enough
    // to hit the floor drew a 240-point card centred on a page narrower than
    // that, and the card hung over the sidebar beside it — nothing clips the
    // page to its own rect.
    let width = COLUMN
        .min(content_rect.width() - 80.0)
        .max(240.0)
        .min((content_rect.width() - 16.0).max(0.0));
    // Worked out once and handed to both: the page says the suggestion and the
    // button acts on it, and the two disagreeing would be worse than neither.
    let suggested = match trouble {
        Trouble::NotFound => suggestion(chrome, told.host.as_deref().unwrap_or_default()),
        _ => None,
    };
    let body_text = body(chrome, trouble, &told, suggested.as_ref());
    let buttons = buttons(trouble, &told, suggested.as_ref());

    // Measured before anything is drawn, so the whole column can be centred on
    // the page rather than the message being centred and the game hanging off
    // the bottom of it.
    // One ink decision for the page. `Palette::over` asks what is behind a
    // rect, and the whole card sits on one patch of the page's own ground, so
    // asking twice — once to measure and once to draw — could only produce two
    // answers that had to agree.
    let ink = palette.over(content_rect);
    let plan = Plan::of(
        root,
        trouble,
        &body_text,
        &buttons,
        width,
        ink.text,
        ink.text_muted,
        trouble.tint(&palette),
    );
    let playing = trouble.is_a_wait() && content_rect.height() > plan.height + GAME_HEIGHT + 80.0;
    let total = plan.height + if playing { GAP + GAME_HEIGHT } else { 0.0 };
    let top = (content_rect.center().y - total * 0.5).max(content_rect.min.y + 24.0);

    let message = Rect::from_min_size(
        pos2(content_rect.center().x - width * 0.5, top),
        vec2(width, plan.height),
    );
    draw_message(
        root, chrome, trouble, message, &plan, &buttons, &ink, actions,
    );

    if playing {
        let panel = Rect::from_min_size(
            pos2(message.min.x, message.max.y + GAP),
            vec2(width, GAME_HEIGHT),
        );
        draw_game(root, &palette, panel, tab);
    }
}

/// One button on the message card.
struct Press {
    icon: Icon,
    label: String,
    /// The one filled button, if there is one. 7b gives the accent to the
    /// action that actually works, and nothing else on the page competes.
    lead: bool,
    action: Deed,
}

/// What pressing it does. An enum rather than a closure so the whole page can
/// be built before anything borrows `chrome` mutably.
enum Deed {
    /// Hand the address to a browser that can render it.
    Elsewhere(String),
    /// Go somewhere — the site that failed, or the one it was probably meant
    /// to be.
    Go(String),
    /// Read the roadmap, in this browser, because it can render a page of
    /// Markdown perfectly well.
    Read(String),
    /// Leave. Closes the tab, because a page that failed is a tab that is
    /// worth nothing, and "back" on a tab with no history behind it is a
    /// button that does not do what it says.
    Leave,
}

fn buttons(trouble: Trouble, told: &Told, suggested: Option<&(String, usize)>) -> Vec<Press> {
    // Only what there is actually something to do. A page that always shows
    // three buttons and quietly no-ops two of them is worse than a page with
    // one, and an error page is exactly where a reader is least able to tell
    // the difference between "that did nothing" and "that failed again".
    let site = told
        .host
        .as_deref()
        .filter(|host| !host.is_empty())
        .map(|host| format!("https://{host}"));
    match trouble {
        Trouble::Unsupported => {
            let mut out = Vec::new();
            if let Some(url) = &site {
                let (name, _) = other_browser();
                out.push(Press {
                    icon: Icon::ExternalLink,
                    label: format!("Open in {name}"),
                    lead: true,
                    action: Deed::Elsewhere(url.clone()),
                });
                out.push(Press {
                    icon: Icon::Reload,
                    label: "Try again".to_owned(),
                    lead: false,
                    action: Deed::Go(url.clone()),
                });
            }
            out.push(Press {
                icon: Icon::Info,
                label: "What's missing".to_owned(),
                lead: site.is_none(),
                action: Deed::Read(
                    "https://github.com/Goddv/Zervo/blob/main/docs/PARITY.md".to_owned(),
                ),
            });
            out
        },
        // No button at all. It resolves itself, and a button that does nothing
        // but restate the wait is worse than the wait.
        Trouble::Offline => Vec::new(),
        // Nothing that reads as "carry on". The way past is the plain line of
        // text under the card, which is 7b's own instruction.
        Trouble::Certificate => vec![Press {
            icon: Icon::Close,
            label: "Back to safety".to_owned(),
            lead: true,
            action: Deed::Leave,
        }],
        Trouble::Crashed => {
            let mut out = Vec::new();
            if let Some(url) = &site {
                out.push(Press {
                    icon: Icon::Reload,
                    label: "Load it again".to_owned(),
                    lead: true,
                    action: Deed::Go(url.clone()),
                });
            }
            out.push(Press {
                icon: Icon::Info,
                label: "Report it".to_owned(),
                lead: site.is_none(),
                action: Deed::Read("https://github.com/servo/servo/issues".to_owned()),
            });
            out
        },
        Trouble::NotFound => {
            let mut out = Vec::new();
            // The whole argument of this page: the answer is usually sitting
            // in the history already, so offer it as a thing to press rather
            // than as a sentence to read and retype.
            if let Some((host, _)) = suggested {
                out.push(Press {
                    icon: Icon::Forward,
                    label: format!("Go to {host}"),
                    lead: true,
                    action: Deed::Go(format!("https://{host}")),
                });
            }
            if let Some(url) = site {
                out.push(Press {
                    icon: Icon::Reload,
                    label: "Try again".to_owned(),
                    lead: out.is_empty(),
                    action: Deed::Go(url),
                });
            }
            out
        },
    }
}

/// One stretch of the body, and how it is set.
///
/// The artboard sets a hostname and a missing feature in monospace, and the
/// feature in the page's own colour — which is the difference between prose
/// that mentions a hostname and prose that *points at* one. A single string
/// cannot say that, so the body is a handful of runs and the layout job puts
/// them back together with one wrap.
struct Run {
    text: String,
    /// Monospace, for anything the reader could type: a host, an API's name.
    fixed: bool,
    /// The page's own colour, for the one thing it is about.
    lit: bool,
}

impl Run {
    fn plain(text: impl Into<String>) -> Run {
        Run {
            text: text.into(),
            fixed: false,
            lit: false,
        }
    }

    fn fixed(text: impl Into<String>) -> Run {
        Run {
            text: text.into(),
            fixed: true,
            lit: false,
        }
    }

    fn lit(text: impl Into<String>) -> Run {
        Run {
            text: text.into(),
            fixed: true,
            lit: true,
        }
    }
}

/// The runs, laid out as one wrapped paragraph.
fn paragraph(
    ui: &Ui,
    runs: &[Run],
    width: f32,
    ink: Color32,
    tint: Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: width,
            ..Default::default()
        },
        ..Default::default()
    };
    // A section that ends in a space loses it: egui trims trailing whitespace
    // per section, so `"…com"`, `" needs "`, `"Media Source Extensions"` came
    // out as "needsMedia". Spaces are moved to the front of the following
    // section, where they survive — the same text, laid out where the layout
    // will keep it.
    let mut moved: Vec<String> = runs.iter().map(|run| run.text.clone()).collect();
    for index in 1..moved.len() {
        if moved[index - 1].ends_with(' ') {
            moved[index - 1].pop();
            moved[index].insert(0, ' ');
        }
    }
    for (run, text) in runs.iter().zip(&moved) {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                // A little smaller in monospace: at the same nominal size a
                // fixed-pitch face reads a full step larger beside a
                // proportional one, and the line goes lumpy.
                font_id: if run.fixed {
                    FontId::monospace(12.5)
                } else {
                    body_font()
                },
                color: if run.lit { tint } else { ink },
                ..Default::default()
            },
        );
    }
    ui.painter().layout_job(job)
}

/// What the page says, in the reader's own situation.
///
/// Every one of these is written to be true when the address was typed by hand
/// as well as when something filled it in, because both happen and a page that
/// only reads well in one of them is half a page.
fn body(
    chrome: &mut ChromeContext,
    trouble: Trouble,
    told: &Told,
    suggested: Option<&(String, usize)>,
) -> Vec<Run> {
    match trouble {
        Trouble::Unsupported => {
            let site = told.host.clone();
            match (&site, &told.detail) {
                (Some(site), Some(needs)) => vec![
                    Run::fixed(site),
                    Run::plain(" needs "),
                    Run::lit(needs),
                    Run::plain(
                        ", which Servo does not implement — so the page loads and then quietly \
                         does nothing. This is the engine, not Zervo, and it is being worked on \
                         upstream.",
                    ),
                ],
                (Some(site), None) => vec![
                    Run::fixed(site),
                    Run::plain(
                        " asks for something the engine has not built yet. Servo does not tell \
                         Zervo which one, so this page cannot name it — what is still missing is \
                         below. It is the engine, not Zervo, and it is being worked on upstream.",
                    ),
                ],
                (None, _) => vec![
                    Run::plain(
                        "Some of the web asks for something the engine has not built yet — ",
                    ),
                    Run::lit("Media Source Extensions"),
                    Run::plain(
                        ", most often, which is what every streaming site is made of. Servo does not \
                     tell Zervo which one, so a page cannot name it. What is still missing is \
                     below; it is the engine, not Zervo, and it is being worked on upstream.",
                    ),
                ],
            }
        },
        Trouble::Offline => {
            let waiting = chrome
                .browser
                .workspaces
                .iter()
                .flat_map(|workspace| workspace.tabs.iter())
                .filter(|tab| tab.loading)
                .count();
            match waiting {
                0 => vec![Run::plain(
                    "Nothing is queued. The moment an interface comes back, anything you open \
                     will go on its own.",
                )],
                1 => vec![Run::plain(
                    "One tab is waiting to load. It'll go when the network does.",
                )],
                many => vec![Run::plain(format!(
                    "{many} tabs are waiting to load. They'll go when the network does."
                ))],
            }
        },
        Trouble::Certificate => {
            let site = told.host.clone();
            let tail = "That is what a machine in the middle of the connection looks like, and \
                        it is also what a misconfigured server looks like. Zervo cannot tell you \
                        which.";
            match (&site, &told.detail) {
                (Some(site), Some(name)) => vec![
                    Run::plain("The certificate "),
                    Run::fixed(site),
                    Run::plain(" presented is for "),
                    Run::lit(name),
                    Run::plain(format!(", not for {site}. ")),
                    Run::plain(tail),
                ],
                (Some(site), None) => vec![
                    Run::fixed(site),
                    Run::plain(format!(
                        " presented a certificate Zervo could not match to it. {tail}"
                    )),
                ],
                (None, _) => vec![Run::plain(format!(
                    "This site presented a certificate Zervo could not match to it. {tail}"
                ))],
            }
        },
        Trouble::Crashed => {
            let site = told.host.clone();
            let mut runs = Vec::new();
            match &site {
                Some(host) => {
                    runs.push(Run::plain("The engine's content process fell over on "));
                    runs.push(Run::fixed(host));
                    runs.push(Run::plain(". "));
                },
                None => runs.push(Run::plain("The engine's content process fell over. ")),
            }
            runs.push(Run::plain(
                "That is a bug in Servo rather than a feature it has not built, so loading the \
                 page again usually works. Nothing else in the window was affected.",
            ));
            if let Some(why) = &told.detail {
                runs.push(Run::plain(" It said: "));
                runs.push(Run::lit(why));
                runs.push(Run::plain("."));
            }
            runs
        },
        Trouble::NotFound => {
            let site = told.host.clone();
            let mut runs = vec![Run::plain("Nothing answered at ")];
            match &site {
                Some(host) => runs.push(Run::fixed(host)),
                None => runs.push(Run::plain("that address")),
            }
            match suggested {
                Some((host, visits)) => {
                    runs.push(Run::plain(". Did you mean "));
                    runs.push(Run::lit(host));
                    runs.push(Run::plain(format!(
                        "? You've been there {visits} {}.",
                        if *visits == 1 { "time" } else { "times" }
                    )));
                },
                None => runs.push(Run::plain(
                    ". Either the address is wrong or the site is gone.",
                )),
            }
            runs
        },
    }
}

/// The host the reader probably meant, and how often they have been there.
///
/// 7b's own argument for this: "The library already ranks hosts by visits — a
/// typo has an obvious answer sitting right there. Offer it, don't just report
/// the failure."
///
/// Edit distance against the twenty most-visited hosts, with a threshold that
/// scales with the name's length: one wrong character in `bbc.co.uk` is a typo,
/// and three wrong characters in a nine-letter host is a different site.
fn suggestion(chrome: &mut ChromeContext, host: &str) -> Option<(String, usize)> {
    if host.is_empty() {
        return None;
    }
    let bare = host.trim_start_matches("www.").to_ascii_lowercase();
    let allowed = (bare.len() / 4).clamp(1, 3);
    chrome
        .library
        .top_sites(20)
        .iter()
        .filter(|site| site.host.to_ascii_lowercase() != bare)
        .map(|site| {
            let distance = edit_distance(&bare, &site.host.to_ascii_lowercase());
            (distance, site.host.clone(), site.visits)
        })
        .filter(|(distance, _, _)| *distance <= allowed)
        // Nearest first, and the better-visited of two equally near ones.
        .min_by_key(|(distance, _, visits)| (*distance, usize::MAX - *visits))
        .map(|(_, host, visits)| (host, visits))
}

/// Levenshtein distance, two rows at a time.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0_usize; b.len() + 1];
    for (i, left) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            let cost = usize::from(left != right);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// A browser that is not this one: what to call it, and how to hand it a URL.
type Elsewhere = (&'static str, fn(&str));

/// The browser to hand a page over to when this one cannot render it.
///
/// macOS and Windows both ship a browser that is certainly present and
/// certainly not Zervo, so the button can say which. Elsewhere the desktop's
/// own handler is the only answer available, and that may well be Zervo — so
/// the button says "your default browser" rather than promising a name it
/// cannot check.
fn other_browser() -> Elsewhere {
    #[cfg(target_os = "macos")]
    {
        ("Safari", |url: &str| {
            let _ = std::process::Command::new("open")
                .args(["-a", "Safari", url])
                .spawn();
        })
    }
    #[cfg(target_os = "windows")]
    {
        ("Edge", |url: &str| {
            // Not `cmd /C start`. `start` is a cmd builtin, which is the
            // temptation — but cmd re-parses its own command line, so a host
            // carrying `&`, `|`, `>` or `%VAR%` stops being text and becomes
            // syntax. The host comes out of an address, and an address can be
            // typed or arrive on the command line, so that is a command
            // injection with a very short fuse.
            //
            // `explorer.exe` performs the same ShellExecute on a URI and takes
            // it as one argv entry with no shell in the way — the pattern
            // `downloads::open_file` already uses for a path.
            let _ = std::process::Command::new("explorer")
                .arg(format!("microsoft-edge:{url}"))
                .spawn();
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        ("your default browser", |url: &str| {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        })
    }
}

/// Hand a URL to something that is not Zervo.
pub fn open_elsewhere(url: &str) {
    let (_, open) = other_browser();
    open(url);
}

// ── Drawing ────────────────────────────────────────────────────────────────

const CARD_PAD: f32 = 26.0;
const BADGE: f32 = 34.0;
const BUTTON_HEIGHT: f32 = 32.0;

fn headline_font() -> FontId {
    FontId::proportional(20.0)
}

fn body_font() -> FontId {
    FontId::proportional(13.5)
}

fn wrap(
    ui: &Ui,
    text: &str,
    font: &FontId,
    width: f32,
    colour: Color32,
) -> std::sync::Arc<egui::Galley> {
    ui.painter()
        .layout(text.to_owned(), font.clone(), colour, width)
}

/// The proceed line under the certificate card, hoisted so the measure and the
/// draw cannot read two different strings.
const PROCEED: &str = "Continue anyway — only if you know exactly why this certificate is wrong.";

/// Where everything on the message card goes.
///
/// One structure, worked out once, because measuring and drawing were two
/// separate passes that computed the body's top two different ways — and one
/// of them added the *badge's* height where the headline's belonged, so a
/// headline that wrapped to three lines was overprinted by the first line of
/// the body while sixty points of card sat empty under the buttons. Two passes
/// that have to agree will not; one that both read cannot disagree.
struct Plan {
    /// The card's total height, including the plain line under it on the
    /// certificate page.
    height: f32,
    head: std::sync::Arc<egui::Galley>,
    /// Where the headline's own box starts, measured down from the card's top.
    head_top: f32,
    /// How tall the headline's box is — its own height, or the badge's,
    /// whichever is taller.
    head_block: f32,
    body: std::sync::Arc<egui::Galley>,
    /// Each button's row and its offset across, in the order they are drawn.
    /// The row wraps rather than running off the card.
    places: Vec<(usize, f32, f32)>,
    /// The proceed line, on the certificate page only.
    proceed: Option<std::sync::Arc<egui::Galley>>,
}

impl Plan {
    /// `ink` and `muted` are the colours the card will actually be drawn in,
    /// and `tint` the page's own — passed in rather than taken from the
    /// palette because a galley bakes its colours in at layout time, and
    /// laying out here in one colour to paint there in another is how the one
    /// amber run in the whole paragraph would have come out white.
    #[expect(clippy::too_many_arguments, reason = "one plan, one place")]
    fn of(
        ui: &Ui,
        trouble: Trouble,
        body: &[Run],
        buttons: &[Press],
        width: f32,
        ink: Color32,
        muted: Color32,
        tint: Color32,
    ) -> Plan {
        let inner = width - CARD_PAD * 2.0;
        let head = wrap(
            ui,
            trouble.headline(),
            &headline_font(),
            (inner - BADGE - 11.0).max(1.0),
            ink,
        );
        let head_block = head.size().y.max(BADGE);
        // A short headline is centred on the badge; a tall one starts where the
        // badge does, rather than riding up out of the card's top padding.
        let head_top = 24.0 + (BADGE - head.size().y).max(0.0) * 0.5;
        let body = paragraph(ui, body, inner.max(1.0), muted, tint);

        // The buttons, wrapped. Three of them at a 240-point floor are wider
        // than the card, and a row that neither wraps nor clamps just runs off
        // the side of it.
        let font = FontId::proportional(12.5);
        let mut places = Vec::with_capacity(buttons.len());
        let (mut x, mut row) = (0.0_f32, 0_usize);
        for press in buttons {
            let width = ui
                .painter()
                .layout_no_wrap(press.label.clone(), font.clone(), Color32::WHITE)
                .rect
                .width()
                + 45.0;
            if x > 0.0 && x + width > inner {
                row += 1;
                x = 0.0;
            }
            places.push((row, x, width));
            x += width + 9.0;
        }
        let rows = if buttons.is_empty() { 0 } else { row + 1 };

        let proceed = (trouble == Trouble::Certificate).then(|| {
            wrap(
                ui,
                PROCEED,
                &FontId::proportional(11.5),
                inner.max(1.0),
                muted,
            )
        });

        let mut height = 24.0 + head_block + 14.0 + body.size().y + 24.0;
        if rows > 0 {
            height += 18.0 + BUTTON_HEIGHT * rows as f32 + 9.0 * (rows - 1) as f32;
        }
        if let Some(proceed) = &proceed {
            // Room under the card for the one plain line that gets past it.
            height += 8.0 + proceed.size().y + 8.0;
        }
        Plan {
            height,
            head,
            head_top,
            head_block,
            body,
            places,
            proceed,
        }
    }

    /// How tall the card itself is, as against the whole block: the proceed
    /// line sits *under* the card, not in it.
    fn card_height(&self) -> f32 {
        match &self.proceed {
            Some(proceed) => self.height - (8.0 + proceed.size().y + 8.0),
            None => self.height,
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "one card, and everything on it")]
fn draw_message(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    trouble: Trouble,
    rect: Rect,
    plan: &Plan,
    buttons: &[Press],
    ink: &Palette,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let tint = trouble.tint(&palette);
    // 7b: "the one error page that must not be candy. Same material, no glow."
    // The ring goes too — an accent ring is the material saying "look how nice
    // this is" on the one page that must not.
    let plain = trouble == Trouble::Certificate;
    let card = Rect::from_min_size(rect.min, vec2(rect.width(), plan.card_height()));
    glass::paint(
        root.painter(),
        card,
        &palette,
        Glass::tier(Tier::Window)
            .strength(1.0)
            .ring(if plain {
                None
            } else {
                palette.accent_ring(0.18)
            })
            .glow(if plain { 0.0 } else { 0.35 })
            .opaque(palette.bg),
    );
    let inner_left = card.min.x + CARD_PAD;

    // ── The badge and the headline.
    let badge = Rect::from_min_size(pos2(inner_left, card.min.y + 24.0), vec2(BADGE, BADGE));
    root.painter()
        .rect_filled(badge, palette.corner(Tier::Card), tint.gamma_multiply(0.2));
    icons::draw_icon(
        root.painter(),
        Rect::from_center_size(badge.center(), vec2(18.0, 18.0)),
        trouble.icon(),
        tint,
    );
    root.painter().galley(
        pos2(badge.max.x + 11.0, card.min.y + plan.head_top),
        plan.head.clone(),
        ink.text,
    );

    // ── The body. Every offset here comes off the plan, so the card is exactly
    // as tall as what is drawn into it.
    let body_top = card.min.y + 24.0 + plan.head_block + 14.0;
    let body_height = plan.body.size().y;
    root.painter().galley(
        pos2(inner_left, body_top),
        plan.body.clone(),
        ink.text_muted,
    );

    // ── The buttons.
    let rows_top = body_top + body_height + 18.0;
    for (index, press) in buttons.iter().enumerate() {
        let font = FontId::proportional(12.5);
        let (row, offset, button_width) = plan.places[index];
        let button = Rect::from_min_size(
            pos2(
                inner_left + offset,
                rows_top + row as f32 * (BUTTON_HEIGHT + 9.0),
            ),
            vec2(button_width, BUTTON_HEIGHT),
        );
        let response = root.interact(
            button,
            Id::new("zervo_trouble_button").with(index),
            Sense::click(),
        );
        let hover = glass::ease_out(root.ctx().animate_bool_with_time(
            response.id.with("hover"),
            response.hovered(),
            0.12,
        ));
        if press.lead {
            glass::paint(
                root.painter(),
                button,
                &palette,
                Glass::tier(Tier::Control)
                    .tint(palette.active)
                    .strength(1.0)
                    // `rgba(accent,.34)` with `0 0 20px accent(.3)` behind it
                    // on 7b's own primary — except on the certificate, where
                    // nothing may glow.
                    .ring(if plain {
                        None
                    } else {
                        palette.accent_ring(0.3 + 0.2 * hover)
                    })
                    .glow(if plain { 0.0 } else { 0.55 + 0.3 * hover })
                    .opaque(palette.bg)
                    .no_shadow(),
            );
        } else {
            if hover > 0.0 {
                root.painter().rect_filled(
                    button,
                    palette.corner(Tier::Control),
                    palette.surface_hover.gamma_multiply(hover),
                );
            }
            root.painter().rect_stroke(
                button,
                palette.corner(Tier::Control),
                Stroke::new(1.0_f32, palette.border),
                StrokeKind::Inside,
            );
        }
        let colour = if press.lead {
            palette.ink_on(palette.active).0
        } else {
            theme::mix(ink.text_muted, ink.text, hover)
        };
        icons::draw_icon(
            root.painter(),
            Rect::from_center_size(
                pos2(button.min.x + 15.0, button.center().y),
                vec2(14.0, 14.0),
            ),
            press.icon,
            colour,
        );
        root.painter().text(
            pos2(button.min.x + 27.0, button.center().y),
            Align2::LEFT_CENTER,
            &press.label,
            font,
            colour,
        );
        if response.clicked() {
            match &press.action {
                Deed::Elsewhere(url) => open_elsewhere(url),
                Deed::Go(url) | Deed::Read(url) => {
                    actions.push(UiAction::Navigate(url.clone()));
                },
                Deed::Leave => {
                    if let Some(id) = chrome.browser.active_tab {
                        actions.push(UiAction::CloseTab(id));
                    }
                },
            }
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }

    // ── The way past a certificate, as plain text and nothing else.
    if let Some(proceed) = &plan.proceed {
        // Wrapped, and its own height reserved: painted with `Painter::text`
        // it laid out on one unwrappable line and, since the page's clip rect
        // is the whole content panel rather than the card, ran straight off
        // the side of it. The one line that gets past this page is the last
        // one that should be unreadable.
        let text = Rect::from_min_size(
            pos2(card.min.x + CARD_PAD, card.max.y + 8.0),
            proceed.size(),
        );
        let response = root.interact(text, Id::new("zervo_trouble_proceed"), Sense::click());
        root.painter().galley(
            text.min,
            proceed.clone(),
            if response.hovered() {
                ink.text_muted
            } else {
                ink.text_muted.gamma_multiply(0.7)
            },
        );
        // Deliberately inert. Proceeding past a certificate means telling the
        // engine to accept it, and this Servo has no hook for that — offering
        // a link that appeared to work and did not would be worse on this page
        // than on any other in the browser.
        response.on_hover_text(
            "Not yet: the engine offers no way to accept a certificate it has rejected.",
        );
    }
}

// ── Feature Creep ──────────────────────────────────────────────────────────

/// The bricks. Not decoration and not invented: every one is a gap this
/// repository admits to in `docs/PARITY.md` or the README's *Limitations*, in
/// roughly the order they hurt. Clearing them is the roadmap, which is why the
/// score reads "shipped".
const GAPS: [&str; 8] = [
    "Media Source Extensions",
    "Extensions",
    "Find in page",
    "Devtools",
    "Session restore",
    "Saving a login",
    "Clearing site data",
    "Fullscreen video",
];

/// Where the ball is, and what has been knocked out.
#[derive(Clone, Default)]
struct Creep {
    /// Which bricks are gone, by index into [`GAPS`].
    cleared: Vec<bool>,
    /// In the panel's own coordinates, 0..1 across and down, so the game
    /// survives the window being resized.
    ball: egui::Vec2,
    heading: egui::Vec2,
    paddle: f32,
    started: bool,
}

/// How fast the ball crosses the panel, in panel-widths a second.
const BALL_SPEED: f32 = 0.42;
const BALL_RADIUS: f32 = 5.0;
const PADDLE_WIDTH: f32 = 76.0;
const PADDLE_HEIGHT: f32 = 7.0;
/// Where a ball is served from, in the field's own 0..1: below the bricks and
/// above the paddle, heading up and to the right.
const SERVE: egui::Vec2 = egui::vec2(0.5, 0.72);
const HEADING: egui::Vec2 = egui::vec2(0.62, -0.78);

/// The wait, given a shape.
///
/// 7b is careful to say why this is here, and it is worth repeating where the
/// code is: "Not as decoration. This is the error a Zervo user meets
/// constantly and cannot fix, and the honest thing is to say so and give the
/// wait a shape — the bricks are the real gaps from PARITY.md and the README,
/// so clearing them is the roadmap."
///
/// The ball only moves while the pointer is in the panel. A browser that runs
/// an animation loop forever on an error page left open in a background tab is
/// spending someone's battery on a joke.
fn draw_game(root: &mut Ui, palette: &Palette, panel: Rect, tab: crate::state::TabId) {
    // Keyed by the tab, not by the page. `Id::new` is absolute, so a single
    // key made the board a property of the *process*: clear it once and every
    // trouble page opened afterwards, in any window, started finished. Tab ids
    // are monotonic, so a new tab is always a new board — and the state still
    // survives a redraw and a rewritten address within one tab, which is what
    // it is for.
    let id = Id::new("zervo_feature_creep").with(tab);
    let mut state: Creep = root
        .ctx()
        .data_mut(|data| data.get_temp::<Creep>(id))
        .unwrap_or_default();
    if state.cleared.len() != GAPS.len() {
        state.cleared = vec![false; GAPS.len()];
        state.ball = SERVE;
        state.heading = HEADING;
        state.paddle = 0.5;
    }

    glass::paint(
        root.painter(),
        panel,
        palette,
        Glass::tier(Tier::Window)
            .strength(0.75)
            .ring(palette.accent_ring(0.12))
            .opaque(palette.bg),
    );
    let ink = palette.over(panel);
    let response = root.interact(panel, id.with("field"), Sense::hover());

    // ── The heading.
    root.painter().text(
        pos2(panel.min.x + 16.0, panel.min.y + 18.0),
        Align2::LEFT_CENTER,
        "WHILE YOU WAIT — FEATURE CREEP",
        FontId::proportional(10.5),
        ink.text_muted,
    );
    let shipped = state.cleared.iter().filter(|done| **done).count();
    root.painter().text(
        pos2(panel.max.x - 16.0, panel.min.y + 18.0),
        Align2::RIGHT_CENTER,
        format!("{shipped} / {} shipped", GAPS.len()),
        FontId::monospace(11.0),
        palette.accent,
    );

    // ── The bricks, wrapped across the panel.
    let font = FontId::proportional(11.0);
    let mut bricks: Vec<Rect> = Vec::with_capacity(GAPS.len());
    let mut x = panel.min.x + 16.0;
    let mut y = panel.min.y + 34.0;
    for label in GAPS {
        let width = root
            .painter()
            .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE)
            .rect
            .width()
            + 22.0;
        if x + width > panel.max.x - 16.0 {
            x = panel.min.x + 16.0;
            y += 32.0;
        }
        bricks.push(Rect::from_min_size(pos2(x, y), vec2(width, 26.0)));
        x += width + 6.0;
    }

    // ── The ball, if anyone is watching it.
    //
    // The play field starts at the *top* brick row, not under the bottom one.
    // Started below them the ball bounced around an empty box for ever and no
    // brick could be hit, which is a screensaver rather than a game — and the
    // whole point of the panel is that the bricks come out.
    let field = Rect::from_min_max(
        pos2(panel.min.x + 10.0, panel.min.y + 34.0),
        pos2(panel.max.x - 10.0, panel.max.y - 10.0),
    );
    let pointer = root.ctx().pointer_latest_pos();
    // `contains_pointer`, not `hovered`. A brick is a clickable widget sitting
    // on top of the panel, and `hovered` goes false the moment another widget
    // takes the hover — so resting the pointer on a brick, which is exactly
    // what somebody about to press one does, froze the ball and the paddle.
    let inside = response.contains_pointer() && pointer.is_some_and(|at| panel.contains(at));
    if inside {
        state.started = true;
        if let Some(at) = pointer {
            state.paddle = ((at.x - field.min.x) / field.width().max(1.0)).clamp(0.0, 1.0);
        }
    }
    if inside && field.height() > PADDLE_HEIGHT * 6.0 {
        let dt = root.input(|input| input.stable_dt).clamp(0.0, 0.05);
        step(&mut state, &bricks, field, dt);
        root.ctx().request_repaint();
    }

    for (index, brick) in bricks.iter().enumerate() {
        let done = state.cleared.get(index).copied().unwrap_or_default();
        let hit = root.interact(*brick, id.with(("brick", index)), Sense::click());
        // Clickable as well as knockable. The artboard binds `onClick` on every
        // brick, and it is right to: the game is optional and reading the
        // roadmap is not.
        if hit.clicked() && !done {
            state.cleared[index] = true;
        }
        draw_brick(
            root,
            palette,
            *brick,
            GAPS[index],
            &font,
            done,
            hit.hovered(),
        );
    }

    if state.started && field.height() > PADDLE_HEIGHT * 6.0 {
        let ball = pos2(
            field.min.x + state.ball.x * field.width(),
            field.min.y + state.ball.y * field.height(),
        );
        root.painter()
            .circle_filled(ball, BALL_RADIUS, palette.accent);
        let paddle_x = field.min.x + state.paddle * field.width();
        root.painter().rect_filled(
            Rect::from_center_size(
                pos2(
                    paddle_x.clamp(
                        field.min.x + PADDLE_WIDTH * 0.5,
                        field.max.x - PADDLE_WIDTH * 0.5,
                    ),
                    field.max.y - PADDLE_HEIGHT,
                ),
                vec2(PADDLE_WIDTH, PADDLE_HEIGHT),
            ),
            palette.corner(Tier::Hairline),
            palette.accent.gamma_multiply(0.85),
        );
    }

    let hint = if shipped >= GAPS.len() {
        "That's the whole list. Ship it."
    } else if state.started {
        "Knock one out — the paddle follows the pointer."
    } else {
        "Knock one out — move the pointer in here, or just press a brick."
    };
    root.painter().text(
        pos2(panel.min.x + 16.0, panel.max.y - 12.0),
        Align2::LEFT_CENTER,
        hint,
        FontId::proportional(10.0),
        ink.text_muted.gamma_multiply(0.6),
    );

    root.ctx().data_mut(|data| data.insert_temp(id, state));
}

/// One tick of the ball.
fn step(state: &mut Creep, bricks: &[Rect], field: Rect, dt: f32) {
    let step = BALL_SPEED * dt;
    state.ball += state.heading * step;

    if state.ball.x <= 0.0 || state.ball.x >= 1.0 {
        state.heading.x = -state.heading.x;
        state.ball.x = state.ball.x.clamp(0.0, 1.0);
    }
    if state.ball.y <= 0.0 {
        state.heading.y = -state.heading.y;
        state.ball.y = 0.0;
    }

    let at = pos2(
        field.min.x + state.ball.x * field.width(),
        field.min.y + state.ball.y * field.height(),
    );

    // The paddle, and the miss.
    let paddle_y = field.max.y - PADDLE_HEIGHT * 1.5;
    if at.y >= paddle_y && state.heading.y > 0.0 {
        let paddle_x = (field.min.x + state.paddle * field.width()).clamp(
            field.min.x + PADDLE_WIDTH * 0.5,
            field.max.x - PADDLE_WIDTH * 0.5,
        );
        if (at.x - paddle_x).abs() <= PADDLE_WIDTH * 0.5 + BALL_RADIUS {
            state.heading.y = -state.heading.y.abs();
            // Where on the paddle it landed steers it, which is the whole of
            // what makes this a game rather than a screensaver.
            state.heading.x = ((at.x - paddle_x) / (PADDLE_WIDTH * 0.5)).clamp(-1.0, 1.0) * 0.9
                + state.heading.x * 0.1;
            let length = state.heading.length().max(0.01);
            state.heading /= length;
        }
    }
    if state.ball.y > 1.05 {
        // Missed. Served again from the middle rather than ending: nobody
        // opened this page to lose at it.
        state.ball = SERVE;
        state.heading = HEADING;
    }

    for (index, brick) in bricks.iter().enumerate() {
        if state.cleared.get(index).copied().unwrap_or(true) {
            continue;
        }
        if brick.expand(BALL_RADIUS).contains(at) {
            state.cleared[index] = true;
            state.heading.y = state.heading.y.abs();
            break;
        }
    }
}

fn draw_brick(
    root: &mut Ui,
    palette: &Palette,
    rect: Rect,
    label: &str,
    font: &FontId,
    done: bool,
    hovered: bool,
) {
    // A cleared brick is green and quiet; an outstanding one is a live surface
    // with a shadow. Both from the artboard, which spends its one green on
    // exactly this.
    if done {
        root.painter().rect_filled(
            rect,
            palette.corner(Tier::Control),
            palette.success.gamma_multiply(0.16),
        );
        root.painter().rect_stroke(
            rect,
            palette.corner(Tier::Control),
            Stroke::new(1.0_f32, palette.success.gamma_multiply(0.3)),
            StrokeKind::Inside,
        );
        root.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            palette.success.gamma_multiply(0.85),
        );
        return;
    }
    glass::paint(
        root.painter(),
        rect,
        palette,
        Glass::tier(Tier::Control)
            .strength(0.85 + 0.15 * f32::from(u8::from(hovered)))
            .no_shadow(),
    );
    root.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font.clone(),
        palette.text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_answers_to_its_own_address() {
        for trouble in Trouble::ALL {
            assert_eq!(Trouble::from_url(&trouble.url()), Some(trouble));
        }
        assert_eq!(Trouble::from_url("zervo://settings"), None);
        assert_eq!(Trouble::from_url("https://unsupported"), None);
    }

    /// A crash carries the engine's own reason, which arrives as free text and
    /// has to survive the trip through an address intact.
    #[test]
    fn a_crash_carries_the_engines_own_words() {
        let url = address(
            Trouble::Crashed,
            "servo.org",
            "assertion failed: index < len & more",
        );
        assert_eq!(Trouble::from_url(&url), Some(Trouble::Crashed));
        let told = Told::read(&url);
        assert_eq!(told.host.as_deref(), Some("servo.org"));
        assert_eq!(
            told.detail.as_deref(),
            Some("assertion failed: index < len & more"),
            "an ampersand in the reason must not split the query"
        );
    }

    /// The address is the page's only channel, so what goes into it has to come
    /// back out — including the space in a feature's name.
    #[test]
    fn what_the_address_carries_comes_back() {
        let url = address(
            Trouble::Unsupported,
            "watch.example.com",
            "Media Source Extensions",
        );
        assert_eq!(Trouble::from_url(&url), Some(Trouble::Unsupported));
        let told = Told::read(&url);
        assert_eq!(told.host.as_deref(), Some("watch.example.com"));
        assert_eq!(told.detail.as_deref(), Some("Media Source Extensions"));
    }

    /// A page reached by typing its address has no query at all, and must still
    /// read as a page rather than as a template with holes in it.
    #[test]
    fn a_typed_address_carries_nothing_and_that_is_fine() {
        let told = Told::read("zervo://certificate");
        assert!(told.host.is_none() && told.detail.is_none());
    }

    /// Reload on a trouble page means "try the site again", and there has to
    /// be a site for that to mean anything.
    #[test]
    fn reload_goes_to_the_site_or_nowhere() {
        assert_eq!(
            retry_target("zervo://unsupported?host=watch.example.com"),
            Some("https://watch.example.com".to_owned())
        );
        assert_eq!(retry_target("zervo://offline"), None);
        assert_eq!(retry_target("zervo://notfound?host="), None);
    }

    #[test]
    fn a_broken_escape_is_kept_rather_than_swallowed() {
        assert_eq!(percent_decode("a%2Gb"), "a%2Gb");
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("a+b"), "a b");
    }

    #[test]
    fn the_nearest_host_is_the_one_a_typo_meant() {
        assert_eq!(edit_distance("servo.org", "servo.org"), 0);
        assert_eq!(edit_distance("serv.org", "servo.org"), 1);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    /// The bug a screenshot cannot see: the play field started *below* the
    /// bottom brick row, so the ball bounced around an empty box for ever and
    /// no brick could be knocked out by it.
    #[test]
    fn a_ball_takes_a_brick_out() {
        let field = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 200.0));
        // A row across the top of the field, which is where the layout puts
        // the first one. Four of them rather than one, because the assertion
        // is "the ball can reach the bricks", not "this particular trajectory
        // is ergodic" — a single brick a third of the way across can be missed
        // by a periodic path for ever without anything being wrong.
        let bricks: Vec<Rect> = (0_u8..4)
            .map(|column| {
                let left = 8.0 + column as f32 * 96.0;
                Rect::from_min_max(pos2(left, 10.0), pos2(left + 92.0, 36.0))
            })
            .collect();
        let mut state = Creep {
            cleared: vec![false; bricks.len()],
            ball: SERVE,
            heading: HEADING,
            paddle: 0.5,
            started: true,
        };
        for _ in 0..4000 {
            step(&mut state, &bricks, field, 1.0 / 120.0);
            if state.cleared.iter().any(|done| *done) {
                return;
            }
        }
        panic!("the ball never reached a brick in thirty seconds of play");
    }

    /// A ball that leaves the box is a ball that is gone. Every bounce has to
    /// keep it inside, whatever the paddle is doing.
    #[test]
    fn the_ball_stays_in_the_box() {
        let field = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 200.0));
        let mut state = Creep {
            cleared: vec![false; GAPS.len()],
            ball: SERVE,
            heading: HEADING,
            // Parked in a corner, so the ball misses and is served again.
            paddle: 0.0,
            started: true,
        };
        for _ in 0..4000 {
            step(&mut state, &[], field, 1.0 / 120.0);
            assert!(
                (0.0..=1.0).contains(&state.ball.x),
                "left the box sideways at {:?}",
                state.ball
            );
            assert!(
                state.ball.y >= 0.0,
                "left the box upward at {:?}",
                state.ball
            );
        }
    }

    /// Lay a plan out in a headless egui, at whatever width is asked for.
    fn plan_at(width: f32, trouble: Trouble, buttons: &[Press]) -> Plan {
        let ctx = egui::Context::default();
        let mut out = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let body = vec![
                Run::fixed("watch.example.com"),
                Run::plain(" needs "),
                Run::lit("Media Source Extensions"),
                Run::plain(", which Servo does not implement — so the page loads."),
            ];
            out = Some(Plan::of(
                ui,
                trouble,
                &body,
                buttons,
                width,
                Color32::WHITE,
                Color32::GRAY,
                Color32::YELLOW,
            ));
        });
        out.expect("the frame ran")
    }

    fn three_buttons() -> Vec<Press> {
        buttons(
            Trouble::Unsupported,
            &Told {
                host: Some("watch.example.com".to_owned()),
                detail: None,
            },
            None,
        )
    }

    /// The card is measured once and drawn from the same numbers. It used to
    /// be measured one way and drawn another, and the two disagreed the moment
    /// the headline wrapped: the body was pinned 72 points down whatever the
    /// headline did, so a three-line headline was overprinted by it while the
    /// card kept sixty points of reserved emptiness under the buttons.
    #[test]
    fn the_headline_never_reaches_the_body() {
        for width in [240.0_f32, 280.0, 340.0, 420.0, 620.0] {
            let plan = plan_at(width, Trouble::Unsupported, &three_buttons());
            let head_bottom = plan.head_top + plan.head.size().y;
            let body_top = 24.0 + plan.head_block + 14.0;
            assert!(
                head_bottom <= body_top,
                "at {width}pt the headline ends at {head_bottom} and the body starts at {body_top}"
            );
            assert!(
                plan.head_top >= 24.0,
                "at {width}pt the headline rides up into the card's top padding, to {}",
                plan.head_top
            );
        }
    }

    /// Three buttons are wider than a 240-point card. They wrap rather than
    /// running off the side of it, and the card grows to hold the extra row.
    #[test]
    fn the_buttons_stay_on_the_card() {
        for width in [240.0_f32, 280.0, 340.0, 420.0, 620.0] {
            let buttons = three_buttons();
            let plan = plan_at(width, Trouble::Unsupported, &buttons);
            let inner = width - CARD_PAD * 2.0;
            let mut rows = 0;
            for (row, offset, button) in &plan.places {
                assert!(
                    offset + button <= inner + 0.5,
                    "at {width}pt a button reaches {} of {inner}",
                    offset + button
                );
                rows = rows.max(row + 1);
            }
            // And the height reserved covers every row that was placed.
            let stack = 24.0
                + plan.head_block
                + 14.0
                + plan.body.size().y
                + 24.0
                + 18.0
                + BUTTON_HEIGHT * rows as f32
                + 9.0 * (rows - 1) as f32;
            assert!(
                (plan.height - stack).abs() < 0.5,
                "at {width}pt the card is {} tall for {stack} of content",
                plan.height
            );
        }
    }

    /// The certificate page's one way through is under the card, and the card
    /// has to leave room for however many lines it takes.
    #[test]
    fn the_proceed_line_has_room_of_its_own() {
        for width in [240.0_f32, 620.0] {
            let plan = plan_at(width, Trouble::Certificate, &[]);
            let proceed = plan.proceed.as_ref().expect("the certificate page has one");
            assert!(
                proceed.size().x <= width - CARD_PAD * 2.0 + 0.5,
                "at {width}pt the proceed line is {} wide",
                proceed.size().x
            );
            assert!(
                (plan.height - plan.card_height() - (16.0 + proceed.size().y)).abs() < 0.5,
                "the block does not reserve the line's own height"
            );
        }
        assert!(
            plan_at(620.0, Trouble::Offline, &[]).proceed.is_none(),
            "only the certificate has a way past"
        );
    }

    /// Only the wait gets the game. A crash is not a wait — reloading is one
    /// press away and usually works — so it does not get one either.
    #[test]
    fn three_of_the_four_do_not_get_a_game() {
        let playing: Vec<Trouble> = Trouble::ALL
            .into_iter()
            .filter(|trouble| trouble.is_a_wait())
            .collect();
        assert_eq!(playing, vec![Trouble::Unsupported]);
    }
}

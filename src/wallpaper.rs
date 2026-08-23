//! Photographs for the new tab page, from sources that publish under licences
//! which allow it.
//!
//! Two of them, and neither wants an API key: Wikimedia Commons' picture of
//! the day, which is a curated archive going back years, and Openverse, which
//! indexes openly-licensed photographs and can be asked for a subject. A third
//! "source" is a file the user picked, which needs no network at all.
//!
//! Everything happens on a thread of its own. A wallpaper is never worth
//! stalling a frame for, and the page has something to show either way — so
//! the fetch, the decode and the downscale all run off the main thread and the
//! result is collected whenever the page next draws.
//!
//! The credit line is not decoration. Most of what comes back is CC BY or CC
//! BY-SA, which are licences with a condition attached, so the title, the
//! photographer and the licence travel with the picture and are drawn with it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Where a photograph comes from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// Wikimedia Commons' picture of the day, from a day picked at random.
    #[default]
    Commons,
    /// Openverse, searched for a subject.
    Openverse(Subject),
    /// A file on this machine.
    File(String),
}

impl Source {
    pub fn label(&self) -> String {
        match self {
            Source::Commons => "Wikimedia Commons".to_owned(),
            Source::Openverse(subject) => format!("Openverse — {}", subject.label().to_lowercase()),
            Source::File(path) => Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "A file".to_owned()),
        }
    }
}

/// What to ask Openverse for. A short list of subjects that photograph well at
/// the size of a window, rather than a free-text box nobody would fill in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subject {
    Landscape,
    Mountains,
    Coast,
    Forest,
    Sky,
    Night,
    City,
    Texture,
}

impl Subject {
    pub const ALL: [Subject; 8] = [
        Subject::Landscape,
        Subject::Mountains,
        Subject::Coast,
        Subject::Forest,
        Subject::Sky,
        Subject::Night,
        Subject::City,
        Subject::Texture,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Subject::Landscape => "Landscape",
            Subject::Mountains => "Mountains",
            Subject::Coast => "Coast",
            Subject::Forest => "Forest",
            Subject::Sky => "Sky",
            Subject::Night => "Night",
            Subject::City => "City",
            Subject::Texture => "Texture",
        }
    }

    /// The search Openverse actually runs. Two words beat one: "landscape"
    /// alone returns a great deal of landscape *painting*.
    fn query(self) -> &'static str {
        match self {
            Subject::Landscape => "landscape+photograph+scenery",
            Subject::Mountains => "mountain+range+peaks",
            Subject::Coast => "coast+sea+shoreline",
            Subject::Forest => "forest+trees+woodland",
            Subject::Sky => "sky+clouds+horizon",
            Subject::Night => "night+sky+stars",
            Subject::City => "city+skyline+architecture",
            Subject::Texture => "abstract+texture+pattern",
        }
    }
}

/// How often a new photograph is fetched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cadence {
    /// Only when asked.
    Manual,
    Launch,
    Hourly,
    Daily,
}

impl Cadence {
    pub const ALL: [Cadence; 4] = [
        Cadence::Manual,
        Cadence::Launch,
        Cadence::Hourly,
        Cadence::Daily,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Cadence::Manual => "Manually",
            Cadence::Launch => "Each launch",
            Cadence::Hourly => "Hourly",
            Cadence::Daily => "Daily",
        }
    }

    fn interval(self) -> Option<Duration> {
        match self {
            Cadence::Manual | Cadence::Launch => None,
            Cadence::Hourly => Some(Duration::from_secs(60 * 60)),
            Cadence::Daily => Some(Duration::from_secs(24 * 60 * 60)),
        }
    }
}

/// A photograph, and what has to be said about it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Credit {
    pub title: String,
    pub author: String,
    pub licence: String,
    /// Where it came from, so the credit can be followed back.
    pub page: String,
    /// Which source produced it, for the settings page to show.
    pub source: String,
}

impl Credit {
    /// The one line drawn over the picture. Everything the licences ask for,
    /// in the order a person reads it.
    pub fn line(&self) -> String {
        let mut line = if self.title.is_empty() {
            "Untitled".to_owned()
        } else {
            self.title.clone()
        };
        if !self.author.is_empty() {
            line.push_str(" · ");
            line.push_str(&self.author);
        }
        if !self.licence.is_empty() {
            line.push_str(" · ");
            line.push_str(&self.licence);
        }
        line
    }
}

/// What the page needs in order to draw a wallpaper: the texture if there is
/// one, the credit that has to be drawn with it, and whether anything is still
/// happening. Always handed over, even with nothing in it — a page that has to
/// say "nothing fetched yet" needs to be told that too.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub texture: Option<&'a egui::TextureHandle>,
    /// The blurred copy, handed to the palette so every surface over the
    /// picture is frosted against it.
    pub frost: Option<&'a crate::theme::Frost>,
    pub credit: &'a Credit,
    pub error: Option<&'a str>,
    pub loading: bool,
}

/// A decoded wallpaper: the picture, and a blurred copy of it.
pub struct Decoded {
    pub sharp: egui::ColorImage,
    /// What every glass surface drawn over the picture frosts itself against.
    pub frost: egui::ColorImage,
}

/// What the fetch thread hands back.
struct Fetched {
    credit: Credit,
    image: Decoded,
    /// Where the bytes were cached, so the next launch has a picture before
    /// the network does anything.
    cached: Option<PathBuf>,
}

#[derive(Default)]
pub struct Wallpaper {
    credit: Credit,
    /// The decoded picture, waiting for the main thread to make it a texture.
    pending: Option<Decoded>,
    inbox: Option<Receiver<Result<Fetched, String>>>,
    fetched_at: Option<SystemTime>,
    /// What went wrong last time, shown in the settings rather than swallowed.
    pub error: Option<String>,
    /// Set once per launch, so `Each launch` means once.
    launched: bool,
}

impl Wallpaper {
    /// Pick up whatever was cached last time, decoding it on a thread so a
    /// launch never waits on a photograph.
    pub fn restore(&mut self) {
        let Some((credit, path, at)) = read_manifest() else {
            return;
        };
        self.credit = credit.clone();
        self.fetched_at = at;
        let (sender, receiver) = channel();
        self.inbox = Some(receiver);
        std::thread::spawn(move || {
            let outcome = std::fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| decode(&bytes))
                .map(|image| Fetched {
                    credit,
                    image,
                    cached: None,
                });
            let _ = sender.send(outcome);
        });
    }

    pub fn is_loading(&self) -> bool {
        self.inbox.is_some()
    }

    pub fn credit(&self) -> &Credit {
        &self.credit
    }

    /// True when `cadence` says this source is due for a new picture. A file
    /// the user chose is never due: it is the picture they asked for.
    pub fn due(&self, source: &Source, cadence: Cadence) -> bool {
        if self.is_loading() || matches!(source, Source::File(_)) {
            return false;
        }
        match cadence {
            Cadence::Manual => false,
            Cadence::Launch => !self.launched,
            _ => match (cadence.interval(), self.fetched_at) {
                (Some(interval), Some(at)) => at.elapsed().unwrap_or(interval) >= interval,
                (Some(_), None) => true,
                (None, _) => false,
            },
        }
    }

    /// Start fetching. Returns without waiting; call [`Wallpaper::poll`] until
    /// it reports something arrived.
    pub fn fetch(&mut self, source: &Source) {
        if self.is_loading() {
            return;
        }
        self.launched = true;
        self.error = None;
        let source = source.clone();
        let (sender, receiver) = channel();
        self.inbox = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send(load(&source));
        });
    }

    /// Collect a finished fetch. True when anything changed and the page
    /// should be redrawn.
    pub fn poll(&mut self) -> bool {
        let Some(inbox) = &self.inbox else {
            return false;
        };
        match inbox.try_recv() {
            Ok(Ok(fetched)) => {
                self.inbox = None;
                self.credit = fetched.credit.clone();
                self.pending = Some(fetched.image);
                if let Some(path) = fetched.cached {
                    self.fetched_at = Some(SystemTime::now());
                    write_manifest(&fetched.credit, &path, self.fetched_at);
                }
                true
            },
            Ok(Err(why)) => {
                self.inbox = None;
                log::warn!("Wallpaper: {why}");
                self.error = Some(why);
                true
            },
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.inbox = None;
                self.error = Some("the fetch gave up".to_owned());
                true
            },
        }
    }

    /// Take the decoded picture, for the caller to upload as a texture. The
    /// upload needs an egui context, which is why this module does not do it.
    pub fn take_image(&mut self) -> Option<Decoded> {
        self.pending.take()
    }
}

// ── Fetching ───────────────────────────────────────────────────────────────

/// A photograph big enough to fill a window without a second of it being
/// spent on pixels nobody sees.
const WANT_WIDTH: u32 = 1920;
/// The longest side a *source* picture may have before it is refused outright.
/// Generous — Wikimedia serves genuinely large scans — but finite.
const MAX_SOURCE_SIDE: u32 = 16_384;
/// The longest side a texture is allowed. Beyond this the extra detail costs
/// video memory and buys nothing at window sizes.
const MAX_SIDE: u32 = 2048;
/// The longest side of the blurred copy the chrome frosts itself against.
///
/// Small on purpose. It is about to be blurred past the point where any of
/// that detail survives, and the card drawn on top scales it back up — where
/// the texture sampler's own bilinear filtering smooths it further, for free.
/// Storing it at full size would be storing a megabyte of information that has
/// already been thrown away.
pub const FROST_SIDE: u32 = 640;
/// How far the frost is blurred, in pixels of the small copy, before the
/// reader's setting scales it.
///
/// Kept well under the copy's own size on purpose. A sigma that approaches it
/// stops being a blur: the box passes reach past the edges, what they clamp
/// against dominates the result, and a *heavier* setting can come back with
/// more contrast than a lighter one rather than less. That is not a
/// theoretical worry — it is what the first pair of numbers here did, and
/// `heavier_blur_leaves_less_of_the_picture` is the test that caught it. The material's, since how frosted frosted
/// glass is is a property of the material and not of the fetcher — this thread
/// simply has no palette to ask.
const FROST_BLUR: f32 = crate::theme::Material::ZERVO.blur;
/// The most the reader's setting may scale it to, as a share of the copy's
/// shortest side — past this the artefacts above set in.
const FROST_BLUR_CEILING: f32 = 0.06;
/// A picture this large is a mistake somewhere.
const MAX_BYTES: usize = 24 * 1024 * 1024;
/// Metadata is small; a megabyte of it is not metadata.
const MAX_JSON: usize = 4 * 1024 * 1024;

fn load(source: &Source) -> Result<Fetched, String> {
    match source {
        Source::File(path) => {
            let bytes = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
            let image = decode(&bytes)?;
            Ok(Fetched {
                credit: Credit {
                    title: Path::new(path)
                        .file_stem()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    source: "This machine".to_owned(),
                    ..Credit::default()
                },
                image,
                cached: None,
            })
        },
        Source::Commons => commons(),
        Source::Openverse(subject) => openverse(*subject),
    }
}

/// Wikimedia Commons' picture of the day, from a day picked at random out of
/// the last ten years. Today's would be the same picture for everyone all day;
/// the archive is the interesting part.
fn commons() -> Result<Fetched, String> {
    let mut last = String::from("no picture of the day");
    // A handful of days have no picture in the feed. Try a few rather than
    // report a failure the user cannot act on.
    for attempt in 0..4_u32 {
        let (year, month, day) = random_day(attempt);
        let feed = format!(
            "https://api.wikimedia.org/feed/v1/wikipedia/en/featured/{year}/{month:02}/{day:02}"
        );
        let response = match crate::net::get(&feed, "application/json", MAX_JSON) {
            Ok(response) => response,
            Err(why) => {
                last = why;
                continue;
            },
        };
        let json: serde_json::Value = match serde_json::from_slice(&response.body) {
            Ok(json) => json,
            Err(error) => {
                last = error.to_string();
                continue;
            },
        };
        let Some(image) = json.get("image") else {
            continue;
        };
        let title = text(image.get("title")).replace("File:", "");
        let credit = Credit {
            title: pretty_title(&title),
            author: strip_markup(&text(image.pointer("/artist/text"))),
            licence: text(image.pointer("/license/type")),
            page: text(image.get("file_page")),
            source: "Wikimedia Commons".to_owned(),
        };

        // `Special:FilePath` resizes for us and redirects to the file store.
        // Failing that, the feed's own thumbnail is small but always there.
        let sized = file_path_url(&title);
        let fallback = text(image.pointer("/thumbnail/source"));
        for candidate in [sized, (!fallback.is_empty()).then_some(fallback)]
            .into_iter()
            .flatten()
        {
            match picture(&candidate) {
                Ok((bytes, from)) => return finish(credit, &bytes, &from),
                Err(why) => last = why,
            }
        }
    }
    Err(last)
}

/// Openverse, asked for a subject and given one of the answers.
fn openverse(subject: Subject) -> Result<Fetched, String> {
    // Openverse pages twenty at a time; a page picked at random out of the
    // first few is variety enough without asking for a page that is not there.
    let page = 1 + (random_u32(0) % 4);
    let query = format!(
        "https://api.openverse.org/v1/images/?q={}&license_type=all-cc\
         &aspect_ratio=wide&size=large&page_size=20&mature=false&page={page}",
        subject.query()
    );
    let response = crate::net::get(&query, "application/json", MAX_JSON)?;
    let json: serde_json::Value =
        serde_json::from_slice(&response.body).map_err(|error| error.to_string())?;
    let results = json
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Openverse returned nothing".to_owned())?;

    // Wide, large, small enough to be worth the transfer, and a format the
    // decoder actually has.
    let usable: Vec<&serde_json::Value> = results
        .iter()
        .filter(|item| {
            let width = item.get("width").and_then(serde_json::Value::as_u64);
            let size = item.get("filesize").and_then(serde_json::Value::as_u64);
            let kind = text(item.get("filetype")).to_lowercase();
            let url = text(item.get("url"));
            width.is_some_and(|width| width >= 1600)
                && size.is_none_or(|size| size as usize <= MAX_BYTES)
                && matches!(kind.as_str(), "jpg" | "jpeg" | "png")
                && url.starts_with("https://")
        })
        .collect();
    if usable.is_empty() {
        return Err(format!("nothing usable for {}", subject.label()));
    }

    let mut last = String::from("nothing downloaded");
    // Try a few of them: an index can outlive the file it points at.
    let start = random_u32(1) as usize % usable.len();
    for offset in 0..usable.len().min(4) {
        let item = usable[(start + offset) % usable.len()];
        let url = text(item.get("url"));
        let credit = Credit {
            // Openverse passes a provider's title through as it found it, and
            // some providers store HTML in that field. It lands in a credit
            // line drawn as plain text, so a `<div class='fn'>` arrives on
            // screen exactly as written unless it is taken out here.
            title: strip_markup(&text(item.get("title"))),
            author: strip_markup(&text(item.get("creator"))),
            licence: licence_name(
                &text(item.get("license")),
                &text(item.get("license_version")),
            ),
            page: text(item.get("foreign_landing_url")),
            source: "Openverse".to_owned(),
        };
        match picture(&url) {
            Ok((bytes, from)) => return finish(credit, &bytes, &from),
            Err(why) => last = why,
        }
    }
    Err(last)
}

/// Fetch something that had better be a photograph.
///
/// The content type is checked before the decoder sees it: a host that has
/// mislaid a file answers with an HTML error page, and "unsupported image
/// format" is a much worse thing to read than "that was not a picture".
fn picture(url: &str) -> Result<(Vec<u8>, String), String> {
    let response = crate::net::get(url, "image/jpeg,image/png", MAX_BYTES)?;
    if !response.content_type.starts_with("image/") {
        return Err(format!("{url} answered with {}", response.content_type));
    }
    Ok((response.body, response.url))
}

/// Cache the bytes, decode them, and hand both back.
fn finish(credit: Credit, bytes: &[u8], from: &str) -> Result<Fetched, String> {
    let image = decode(bytes)?;
    let cached = cache(bytes, from);
    Ok(Fetched {
        credit,
        image,
        cached,
    })
}

/// Decode and downscale, both on this thread. A six-megapixel photograph is
/// several hundred milliseconds of work; doing it where the frames are drawn
/// would be a visible stall for something nobody asked to wait for.
fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    // `load_from_memory` applies `Limits::default()`, which caps one allocation
    // at 512 MB but places no bound at all on the dimensions. That is a decode
    // this thread can spend an appreciable time on, for a picture whose only
    // qualification is that a stranger's search result pointed at it. Naming a
    // side bound rejects the pathological ones up front, off the header, before
    // any of the pixels are touched.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_SIDE);
    limits.max_image_height = Some(MAX_SOURCE_SIDE);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    // `limits` sets rather than returns, so it cannot join the chain above.
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| error.to_string())?;
    let (width, height) = (decoded.width().max(1), decoded.height().max(1));
    let fit = |longest_allowed: u32| {
        let longest = width.max(height);
        let scale = (longest_allowed as f32 / longest as f32).min(1.0);
        (
            ((width as f32 * scale) as u32).max(1),
            ((height as f32 * scale) as u32).max(1),
        )
    };

    // The blurred copy first, and from the original rather than from the
    // downscaled one: going straight to a small size with a decent filter is
    // most of the blur already, and doing it in one step is both faster and
    // cleaner than blurring two thousand pixels.
    //
    // Taking it first is also what lets the sharp copy below *move* the decoded
    // image when it is already small enough, rather than cloning it. That clone
    // was a second full-size buffer alongside the first, for a picture that had
    // just been allowed to be 512 MB.
    let (frost_w, frost_h) = fit(FROST_SIDE);
    let small = decoded
        .resize_exact(frost_w, frost_h, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let ceiling = frost_w.min(frost_h) as f32 * FROST_BLUR_CEILING;
    let sigma = (FROST_BLUR).min(ceiling).max(0.5);
    let blurred = image::imageops::fast_blur(&small, sigma);

    let (sharp_w, sharp_h) = fit(MAX_SIDE);
    let sharp = if sharp_w < width || sharp_h < height {
        decoded.resize_exact(sharp_w, sharp_h, image::imageops::FilterType::CatmullRom)
    } else {
        decoded
    };

    let as_image = |rgba: image::RgbaImage| {
        let size = [rgba.width() as usize, rgba.height() as usize];
        (size[0] > 0 && size[1] > 0).then(|| egui::ColorImage::from_rgba_unmultiplied(size, &rgba))
    };
    let sharp = as_image(sharp.to_rgba8()).ok_or("the picture has no pixels")?;
    let frost = as_image(blurred).ok_or("the picture has no pixels")?;
    Ok(Decoded { sharp, frost })
}

// ── The cache on disk ──────────────────────────────────────────────────────

fn cache_dir() -> Option<PathBuf> {
    Some(crate::settings::data_dir()?.join("wallpapers"))
}

/// Write the picture beside the settings, so the next launch has something to
/// show before the network has said anything.
fn cache(bytes: &[u8], from: &str) -> Option<PathBuf> {
    let extension = if from.to_lowercase().contains(".png") {
        "png"
    } else {
        "jpg"
    };
    let directory = cache_dir()?;
    std::fs::create_dir_all(&directory).ok()?;
    let path = directory.join(format!("current.{extension}"));
    // Whole or not at all: the manifest written next records this path, and a
    // half a JPEG on disk with a manifest pointing confidently at it is a
    // backdrop that fails to decode on every launch until something replaces it.
    crate::store::write_private(&path, bytes).ok()?;
    // The other extension is now a stale copy of a picture nobody will see.
    let stale = directory.join(if extension == "png" {
        "current.jpg"
    } else {
        "current.png"
    });
    let _ = std::fs::remove_file(stale);
    Some(path)
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    credit: Credit,
    path: String,
    /// Unix seconds, so the file stays readable and the cadence survives a
    /// restart.
    at: Option<i64>,
}

fn manifest_path() -> Option<PathBuf> {
    Some(cache_dir()?.join("current.json"))
}

fn write_manifest(credit: &Credit, path: &Path, at: Option<SystemTime>) {
    let Some(target) = manifest_path() else {
        return;
    };
    let manifest = Manifest {
        credit: credit.clone(),
        path: path.to_string_lossy().into_owned(),
        at: at
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
            .map(|since| since.as_secs() as i64),
    };
    let _ = crate::store::save(Some(target), &manifest);
}

fn read_manifest() -> Option<(Credit, PathBuf, Option<SystemTime>)> {
    let contents = std::fs::read_to_string(manifest_path()?).ok()?;
    let manifest: Manifest = serde_json::from_str(&contents).ok()?;
    let path = PathBuf::from(manifest.path);
    if !path.exists() {
        return None;
    }
    let at = manifest
        .at
        .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds.max(0) as u64));
    Some((manifest.credit, path, at))
}

// ── Small helpers ──────────────────────────────────────────────────────────

fn text(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Both sources hand back HTML in fields that are drawn as plain text: Commons
/// renders its artist field from wiki markup — a link, sometimes a whole
/// gallery — and Openverse passes a provider's title through untouched. Take
/// the first line and drop the tags: a credit is a name, not a document.
fn strip_markup(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for character in html.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            '\n' | '\r' if !inside => break,
            _ if !inside => out.push(character),
            _ => {},
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    crate::ui::ellipsize(
        out.split_whitespace().collect::<Vec<_>>().join(" ").trim(),
        70,
    )
}

/// Commons file names are the caption nobody wrote: underscores, a date, a
/// camera's serial number. Make them read like a title.
fn pretty_title(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    crate::ui::ellipsize(stem.replace('_', " ").trim(), 70)
}

/// Openverse reports a licence as a code and a version; the licences want
/// their names.
fn licence_name(code: &str, version: &str) -> String {
    let code = code.to_uppercase();
    let name = match code.as_str() {
        "" => return String::new(),
        "CC0" => "CC0".to_owned(),
        "PDM" => return "Public domain".to_owned(),
        other => format!("CC {other}"),
    };
    if version.is_empty() {
        name
    } else {
        format!("{name} {version}")
    }
}

/// `Special:FilePath` takes a file name and a width and redirects to the right
/// thumbnail, which saves knowing how the file store lays its paths out.
fn file_path_url(title: &str) -> Option<String> {
    let mut url = url::Url::parse("https://commons.wikimedia.org/wiki/Special:FilePath").ok()?;
    url.path_segments_mut().ok()?.push(title);
    url.set_query(Some(&format!("width={WANT_WIDTH}")));
    Some(url.to_string())
}

/// A day out of the last ten years, as (year, month, day).
///
/// Cheap and not uniform across month lengths — the 29th of a short month
/// simply has no picture and the caller tries again — but it needs no random
/// number generator and it never repeats within a session.
fn random_day(salt: u32) -> (i32, u32, u32) {
    let now = chrono::Local::now().date_naive();
    let span = 365 * 10;
    let back = (random_u32(salt) % span) as i64;
    let day = now - chrono::Duration::days(back + 1);
    use chrono::Datelike as _;
    (day.year(), day.month(), day.day())
}

/// Enough randomness to pick a picture with. Not enough for anything else.
fn random_u32(salt: u32) -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos() ^ since.as_secs() as u32)
        .unwrap_or(0);
    let mut value = nanos.wrapping_add(salt.wrapping_mul(2_654_435_761));
    // One round of a well-known integer mix, which spreads the low bits the
    // modulo above would otherwise be reading straight off the clock.
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sharp checkerboard, as PNG bytes — something for a blur to soften.
    ///
    /// Big squares on purpose. Fine ones are annihilated by any blur worth
    /// having, so every setting past the lightest measures the same flat grey
    /// and the test can no longer tell them apart. A photograph has structure
    /// at every scale; this has it at one, and that scale has to be coarse
    /// enough to survive the heaviest setting.
    fn checkerboard() -> Vec<u8> {
        let mut image = image::RgbaImage::new(512, 512);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let on = ((x / 64) + (y / 64)) % 2 == 0;
            let value = if on { 255 } else { 0 };
            *pixel = image::Rgba([value, value, value, 255]);
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encodes");
        bytes.into_inner()
    }

    /// How much local contrast is left, as the variance of the pixels. A blur
    /// takes contrast away, so a heavier one leaves less.
    fn contrast(image: &egui::ColorImage) -> f64 {
        // The middle of the picture only. Every blur clamps at the edges, so
        // the border carries an artefact of the blur rather than a measure of
        // it, and at large radii that artefact is the loudest thing there.
        let [width, height] = image.size;
        let (inset_x, inset_y) = (width / 5, height / 5);
        let values: Vec<f64> = (inset_y..height - inset_y)
            .flat_map(|y| (inset_x..width - inset_x).map(move |x| (x, y)))
            .map(|(x, y)| f64::from(image.pixels[y * width + x].r()))
            .collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64
    }

    /// The frost has to actually be blurred. It is a baked texture rather than
    /// something the material does at draw time, so "the material says 10.8"
    /// and "the picture is soft" are two different claims and only the second
    /// one is worth anything.
    #[test]
    fn the_frost_is_blurred() {
        let bytes = checkerboard();
        let decoded = decode(&bytes).expect("decodes");
        let sharp = contrast(&decoded.sharp);
        let frost = contrast(&decoded.frost);
        // Half is the bar, not a tuned figure: the point is that the frost is
        // demonstrably softer than the picture, not that it lands on any
        // particular number. A checkerboard with squares this coarse survives
        // a genuine blur better than a photograph would.
        assert!(
            sharp > frost * 2.0,
            "the frost kept too much of the picture: sharp {sharp:.1}, frost {frost:.1}"
        );
    }

    /// The sharp copy is not blurred at all, whatever the setting says.
    #[test]
    fn the_picture_itself_is_never_blurred() {
        let bytes = checkerboard();
        let decoded = decode(&bytes).expect("decodes");
        // A sharp checkerboard keeps nearly all of its contrast; anything that
        // softened the wallpaper itself would show up here as a collapse.
        assert!(contrast(&decoded.sharp) > 4000.0);
    }
}

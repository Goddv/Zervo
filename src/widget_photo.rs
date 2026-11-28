//! New Tab photos. Search terms go to Commons and downloaded images are cached
//! separately from the card's settings.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;

use egui::{Context, Id, TextureHandle};
use image::{DynamicImage, ImageDecoder as _, ImageReader};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::WakerEvent;
use crate::newtab_custom::{Block, CustomWidget};
use crate::state::TabId;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXTURE: u32 = 1024;
const API: &str = "https://commons.wikimedia.org/w/api.php";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Photo {
    pub title: String,
    pub source: String,
    pub asset: Option<Asset>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    id: String,
    pub width: u32,
    pub height: u32,
    pub credit: String,
    pub source_page: String,
}

impl Photo {
    pub fn clean(&mut self) -> bool {
        self.title = self.title.trim().chars().take(80).collect();
        self.source = self.source.trim().to_owned();
        if let Some(asset) = &mut self.asset {
            asset.credit = asset.credit.chars().take(160).collect();
            if asset.path().is_none()
                || asset.width == 0
                || asset.height == 0
                || asset.width > MAX_TEXTURE
                || asset.height > MAX_TEXTURE
                || direct_url(&asset.source_page).is_none()
            {
                self.asset = None;
            }
        }
        if self.source.contains("://") {
            direct_url(&self.source).is_some()
        } else {
            !self.source.is_empty() && self.source.chars().count() <= 120
        }
    }

    pub fn fetch(&mut self, used: &HashSet<String>) -> Result<(), String> {
        let asset = if let Some(url) = direct_url(&self.source) {
            Candidate {
                url: url.clone(),
                credit: url.host_str().unwrap_or_default().to_owned(),
                page: url,
            }
            .download()
        } else {
            let candidates = commons_candidates(&self.source)?;
            next_asset(&candidates, None, used)
                .or_else(|| next_asset(&candidates, None, &HashSet::new()))
        };
        self.asset = Some(asset.ok_or("No usable JPEG, PNG or WebP image was found")?);
        Ok(())
    }
}

#[derive(Default)]
pub struct Refresh {
    tab: Option<TabId>,
    pending: bool,
    loading: bool,
}

impl Refresh {
    pub fn show(&mut self, tab: Option<TabId>) {
        if self.tab != tab {
            self.tab = tab;
            self.pending = tab.is_some();
        }
    }

    pub fn request(&mut self) {
        self.pending = true;
    }

    pub fn start(
        &mut self,
        cards: &[CustomWidget],
        proxy: winit::event_loop::EventLoopProxy<WakerEvent>,
    ) {
        if self.loading || !std::mem::take(&mut self.pending) {
            return;
        }
        let requests: Vec<_> = cards
            .iter()
            .flat_map(|card| {
                card.blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(block, content)| {
                        let Block::Image(photo) = content else {
                            return None;
                        };
                        (!photo.source.contains("://")).then(|| Request {
                            card: card.id,
                            block,
                            topic: photo.source.clone(),
                            current: photo.asset.clone(),
                        })
                    })
            })
            .collect();

        if !requests.is_empty() {
            self.loading = std::thread::Builder::new()
                .name("widget-photo-refresh".into())
                .spawn(move || {
                    let mut searches = HashMap::new();
                    let mut used = HashSet::new();
                    let mut updates = Vec::new();
                    for request in requests {
                        let candidates =
                            searches.entry(request.topic.clone()).or_insert_with(|| {
                                commons_candidates(&request.topic).unwrap_or_default()
                            });
                        if let Some(asset) = next_asset(candidates, request.current.as_ref(), &used)
                        {
                            used.insert(asset.source_page.clone());
                            updates.push(Update { request, asset });
                        }
                    }
                    let _ = proxy.send_event(WakerEvent::WidgetPhotosRefreshed(updates));
                })
                .is_ok();
        }
    }

    pub fn finish(&mut self, cards: &mut [CustomWidget], updates: Vec<Update>) -> bool {
        self.loading = false;
        let mut changed = false;
        for Update { request, asset } in updates {
            let Some(Block::Image(photo)) = cards
                .iter_mut()
                .find(|card| card.id == request.card)
                .and_then(|card| card.blocks.get_mut(request.block))
            else {
                continue;
            };
            if photo.source == request.topic
                && photo.asset.as_ref().map(|previous| &previous.id)
                    == request.current.as_ref().map(|previous| &previous.id)
            {
                photo.asset = Some(asset);
                changed = true;
            }
        }
        changed
    }
}

#[derive(Debug)]
struct Request {
    card: u64,
    block: usize,
    topic: String,
    current: Option<Asset>,
}

#[derive(Debug)]
pub struct Update {
    request: Request,
    asset: Asset,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn shown_pages(cards: &[CustomWidget]) -> HashSet<String> {
    cards
        .iter()
        .flat_map(|card| &card.blocks)
        .filter_map(|block| match block {
            Block::Image(photo) => photo.asset.as_ref().map(|asset| asset.source_page.clone()),
            _ => None,
        })
        .collect()
}

impl Asset {
    fn path(&self) -> Option<PathBuf> {
        if self.id.len() != 64 || !self.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        Some(
            crate::settings::data_dir()?
                .join("widget-photos")
                .join(format!("{}.jpg", self.id)),
        )
    }

    pub fn texture(&self, ctx: &Context) -> Option<TextureHandle> {
        let key = Id::unique(("widget_photo", &self.id));
        if let Some(handle) = ctx.data(|data| data.get_temp::<TextureHandle>(key)) {
            return Some(handle);
        }

        let image = decode(std::fs::read(self.path()?).ok()?)?.into_rgba8();
        let texture = ctx.load_texture(
            &self.id,
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
        Some(texture)
    }
}

fn direct_url(source: &str) -> Option<Url> {
    if source.len() > 2048 {
        return None;
    }
    let url = Url::parse(source).ok()?;
    (url.scheme() == "https"
        && url.port().is_none()
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(url)
}

struct Candidate {
    url: Url,
    credit: String,
    page: Url,
}

impl Candidate {
    fn download(&self) -> Option<Asset> {
        let response = crate::net::get_with_timeout(
            self.url.as_str(),
            "image/jpeg, image/png, image/webp",
            MAX_BYTES,
            std::time::Duration::from_secs(10),
        )
        .ok()?;
        let image = decode(response.body)?;
        let width = image.width();
        let height = image.height();
        let rgb = image.into_rgb8();
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 89)
            .encode(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
            .ok()?;
        let asset = Asset {
            id: format!("{:x}", Sha256::digest(&bytes)),
            width,
            height,
            credit: self.credit.clone(),
            source_page: self.page.to_string(),
        };
        let path = asset.path()?;
        // Store writes are atomic. Two simultaneous previews can find the
        // same photo and must not write the same temporary filename at once.
        static STORE: Mutex<()> = Mutex::new(());
        let _guard = STORE.lock().ok()?;
        if !path.is_file() {
            crate::store::write_private(&path, &bytes).ok()?;
        }
        Some(asset)
    }
}

fn next_asset(
    candidates: &[Candidate],
    current: Option<&Asset>,
    used: &HashSet<String>,
) -> Option<Asset> {
    let page = current.map(|asset| asset.source_page.as_str());
    rotation(candidates, page, used)
        .take(3)
        .filter_map(Candidate::download)
        .find(|asset| current.is_none_or(|old| old.id != asset.id))
}

fn rotation<'a>(
    candidates: &'a [Candidate],
    current: Option<&'a str>,
    used: &'a HashSet<String>,
) -> impl Iterator<Item = &'a Candidate> {
    let start = current
        .and_then(|page| {
            candidates
                .iter()
                .position(|candidate| candidate.page.as_str() == page)
        })
        .map_or(0, |position| position + 1);
    candidates
        .iter()
        .cycle()
        .skip(start)
        .take(candidates.len())
        .filter(move |candidate| current != Some(candidate.page.as_str()))
        .filter(|candidate| !used.contains(candidate.page.as_str()))
}

fn decode(bytes: Vec<u8>) -> Option<DynamicImage> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    if image.width().max(image.height()) > MAX_TEXTURE {
        image = image.thumbnail(MAX_TEXTURE, MAX_TEXTURE);
    }
    Some(image)
}

#[derive(Deserialize)]
struct Search {
    query: Option<Results>,
}

#[derive(Deserialize)]
struct Results {
    pages: Vec<Page>,
}

#[derive(Deserialize)]
struct Page {
    index: Option<u32>,
    #[serde(default)]
    imageinfo: Vec<ImageInfo>,
}

#[derive(Deserialize)]
struct ImageInfo {
    thumburl: Option<String>,
    descriptionurl: String,
    extmetadata: Metadata,
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(rename = "Artist")]
    artist: Option<Meta>,
    #[serde(rename = "LicenseShortName")]
    license: Option<Meta>,
}

#[derive(Deserialize)]
struct Meta {
    value: String,
}

fn commons_candidates(subject: &str) -> Result<Vec<Candidate>, String> {
    let mut url = Url::parse(API).expect("fixed Wikimedia API address");
    let query = format!("filetype:bitmap {subject}");
    url.query_pairs_mut().extend_pairs([
        ("action", "query"),
        ("generator", "search"),
        ("gsrsearch", &query),
        ("gsrnamespace", "6"),
        ("gsrlimit", "8"),
        ("prop", "imageinfo"),
        ("iiprop", "url|extmetadata"),
        ("iiurlwidth", "640"),
        ("iiextmetadatafilter", "Artist|LicenseShortName"),
        ("format", "json"),
        ("formatversion", "2"),
    ]);
    let response = crate::net::get_with_timeout(
        url.as_str(),
        "application/json",
        512 * 1024,
        std::time::Duration::from_secs(10),
    )?;
    let results =
        serde_json::from_slice::<Search>(&response.body).map_err(|error| error.to_string())?;
    let mut pages = results.query.map(|query| query.pages).unwrap_or_default();
    pages.sort_by_key(|page| page.index.unwrap_or(u32::MAX));
    Ok(pages
        .into_iter()
        .filter_map(|page| {
            let info = page.imageinfo.into_iter().next()?;
            let mut url = direct_url(&info.thumburl?)?;
            if !matches!(
                url.host_str(),
                Some("upload.wikimedia.org" | "thumb.wikimedia.org")
            ) {
                return None;
            }
            url.set_query(None);
            let page = direct_url(&info.descriptionurl)?;
            if page.host_str() != Some("commons.wikimedia.org") {
                return None;
            }
            let license = info.extmetadata.license?.value;
            let artist = info
                .extmetadata
                .artist
                .map_or_else(String::new, |meta| plain_credit(&meta.value));
            let credit = match license.as_str() {
                value if value.starts_with("CC BY ") || value.starts_with("CC BY-SA ") => {
                    if artist.is_empty() {
                        return None;
                    }
                    format!("{artist} · {license}")
                },
                "CC0" | "CC0 1.0" | "Public domain" => {
                    if artist.is_empty() {
                        license
                    } else {
                        format!("{artist} · {license}")
                    }
                },
                _ => return None,
            };
            Some(Candidate { url, page, credit })
        })
        .collect())
}

fn plain_credit(html: &str) -> String {
    let mut in_tag = false;
    html.chars()
        .filter(|character| match character {
            '<' => {
                in_tag = true;
                false
            },
            '>' => {
                in_tag = false;
                false
            },
            _ => !in_tag,
        })
        .collect::<String>()
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(110)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_rotation_wraps_and_skips_photos_already_showing() {
        let candidates = ["a", "b", "c"].map(|suffix| {
            let page = Url::parse(&format!(
                "https://commons.wikimedia.org/wiki/File:{suffix}.jpg"
            ))
            .unwrap();
            Candidate {
                url: page.clone(),
                page,
                credit: String::new(),
            }
        });
        let exclude = HashSet::from([candidates[0].page.to_string()]);
        let order: Vec<_> = rotation(&candidates, Some(candidates[2].page.as_str()), &exclude)
            .map(|candidate| &candidate.page)
            .collect();
        assert_eq!(order, [&candidates[1].page]);
        let none = HashSet::new();
        assert_eq!(
            rotation(&candidates, Some(candidates[0].page.as_str()), &none)
                .map(|candidate| &candidate.page)
                .collect::<Vec<_>>(),
            [&candidates[1].page, &candidates[2].page]
        );
    }
}

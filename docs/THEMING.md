# The theme engine

The goal is that somebody can make Zervo look like Windows, like GTK or Qt,
like Android's Material, or like Apple's Liquid Glass, without touching any
drawing code. The engine is organised around that goal rather than around the
one theme that happens to be in it.

This document used to open by saying a theme was a Rust constant compiled into
the binary, with no file format and nothing to load one at runtime. That is no
longer true, and the way it stopped being true is worth stating, because it was
not by writing a loader.

Every value a material is built from is now a field of [`Appearance`], and
`Appearance` is an ordinary setting: it is on `zervo://settings` → Appearance
with one control per field, and it serialises into `settings.json` with
everything else. So the loader already existed — it is the settings file — and
the file format is whatever that panel writes out. `Appearance::as_json` hands
you it, and `Material::as_rust` hands you the same arrangement as the `const`
you would have had to write by hand.

[`Appearance`]: ../zervo-core/src/theme.rs

## The idea

A theme decides two things and they are kept apart.

**A `Palette` is what colour things are.** Background, surface, accent, text,
border, shadow. It comes out of [`theme::resolve`](../zervo-core/src/theme.rs), which takes
the mode (Auto/Light/Dark), what the system says, and the accent.

**A `Material` is how things are built.** Corner radii, how opaque a surface is,
whether it has a sheen, how far a shadow reaches, whether it frosts what is
behind it and by how much, row heights, control padding, animation time. One
constant — `Material::ZERVO` — carries all of it.

Nothing in the drawing code holds a radius or an opacity of its own. Every
surface is drawn by `glass::shapes`, which is handed the palette it needs
anyway, and reads the material off it. So a material reaches every surface in
the application without a single function signature changing.

```rust
// A card, a menu and a text field, each asking for its class rather than
// for a number.
glass::shapes(rect, palette, Glass::of(Surface::Card));
glass::shapes(rect, palette, Glass::of(Surface::Menu).radius(Tier::Panel));
glass::shapes(rect, palette, Glass::of(Surface::Input));
```

## Classes

Surfaces have a class, the way an element has one in CSS, and the class carries
the values rather than the call site.

| Class | What it is | Why it differs |
| --- | --- | --- |
| `Surface::Card` | A card, a widget, a tile | The default weight |
| `Surface::Menu` | A floating panel: menu, popup, tray | Never heavier than a card, or it stops reading as the same glass |
| `Surface::Input` | Anything you type into | Heaviest — what is behind a text field competes with what you are reading |

Corner radii are named too, so a call site asks for a role and the material
answers with a number:

`Tier::{Hairline, Control, Row, Card, Panel, Pill, Window}` → `Radii::of(tier)`.

`Window` is the newest rung and the reason the others hold together at the top
end: the content card used to borrow `Panel`, so at the one point where the
card's corner *is* the window's corner the two disagreed by four points.
`Radii::scaled` multiplies the whole ladder at once, which is what the
corner-scale control does — the tiers keep their relationship, so a ladder
tuned against itself does not have to be tuned again to be made rounder.

**The window's own corner is not on the ladder at all.** `theme::window_radius`
asks the platform: ten points on macOS, eight on Windows, and the `Window` rung
where the compositor rounds nothing. The window server draws that arc and the
chrome cannot argue with it — painting a different radius at the same corner
does not replace it, it puts a second one beside it, which is what a page
rounded to twenty-nine against a macOS window rounded to ten looked like. It is
therefore the one corner the corner scale does not move, which is exactly true
of the real window as well.

A page in full-page mode takes that corner on all four sides and no gap at all,
whatever the seam says: `Palette::fills_window` is stamped from the layout the
same way the translucency setting is, because there is no chrome there for a
gap to be between.

`Glass::tier(Tier::Card)` is the usual way to ask. `Glass::new(10)` still takes
a literal, for the handful of radii derived from something else — a pill whose
corners are half its height.

## Translucency

One reader-facing setting, `Translucency`, with two steps: **Solid** and
**Frosted**. It drives three things at once, so they cannot drift apart:

- what the *window* asks the platform for (`SystemBackdrop::Opaque` or
  `Frosted`),
- the tint over the window's own chrome,
- the tint on everything the material draws over that chrome.

A material can opt out (`translucency: false`) and then the setting does
nothing, which is what a flat desktop toolkit wants.

**Solid means opaque.** `glass::shapes` refuses to frost under it whatever else
it is told, and the window skips the backdrop capture entirely — the readback,
the blur and the upload all cost nothing rather than a little.

## Frosting

egui cannot blur a shape's backdrop as it draws it, and does not have to. The
palette carries a `Backdrop`: a texture that has already been blurred, the
rectangle it covers, the uv within it, how far it has arrived, a coarse
luminance map, and how far past its own edges it may be sampled.

Whoever draws something worth frosting against hands one to the palette, and
from that point every glass surface inside it samples the same copy through the
same mapping. Nothing at the call sites changes.

Three things supply one:

| Behind | Copy taken from | Where |
| --- | --- | --- |
| The web page | The framebuffer, right after the engine blits | `src/backdrop.rs` |
| The new tab page | The framebuffer, right after its backdrop is painted | `src/newtab.rs` |
| Settings | The framebuffer, after its base and nav column | `src/ui.rs` |

The moment matters more than the place. Shapes within a layer are drawn in the
order they were added, so a copy taken straight after the page's background —
and before anything sits on it — is the one thing that keeps a surface from
frosting against its own previous reflection.

### Reach

A backdrop can be sampled a stated distance beyond its own rectangle
(`Palette::reaching`). The widget shelf slides out of the top of the new tab
page and a hover card drops out of a toolbar button; both sit *beside* the page
rather than on it, and both should read as part of it. The sampler clamps, so
what a surface off the edge gets is the page's own edge carried out to it.

Reach is a distance, not a licence: a menu at the far end of the window still
finds nothing.

### Holding the theme

A tint thin enough to be glass is not thick enough to keep its identity. A dark
menu at a fifth of its own colour, opened over a white page, is four fifths
white — still frosted, still blurred, and no longer a dark menu.

`Palette::tint_over` thickens the tint exactly as far as it must to keep the
surface on its own side of the middle, and no further. Over a page the theme
already agrees with, it changes nothing.

Two numbers make a surface's weight and both are easy to miss: the material's
fill, which is the core's own alpha, and the class tint, which scales the whole
finished surface afterwards. `Palette::nominal_tint` names the product. Code
that reads only one of them is wrong by a factor of three, which is a mistake
this codebase has actually made.

### Text over a backdrop

`Palette::over(rect)` returns the palette with text chosen for whatever is
behind that rectangle, and `prefers_light_ink` answers the question directly.
It picks by WCAG contrast ratio rather than a brightness threshold — the
crossover is not at half — with hysteresis, so text does not flip as a page
scrolls past it.

It is a backstop rather than the main event. When the tint can hold the theme,
the theme's own text is right; the rule earns its place for materials that
cannot pull far enough, which is exactly what a mid-tone third-party theme will
be.

## The arrangement

`Appearance` is every value the reader can set about how Zervo is built, in one
place. `Appearance::material` turns it into a [`Material`], which is what the
drawing code has always asked. It uses `Material::ZERVO` as its base rather
than a blank one: the fields the panel does not offer — how much heavier a
surface gets at full strength, how far a shadow reaches, row height, control
padding — were tuned against each other, and there is nothing to be gained by
making each of them a slider nobody moves.

Three of them are derived rather than offered, because offering them separately
would only be a way to get them wrong:

- **Menu and Input fills** keep their ratio to the card's. About 0.62 and 1.13,
  taken from `Material::ZERVO` rather than written out, so an arrangement at
  the shipped fill reproduces exactly the three numbers the material has always
  had.
- **The light theme's sheen** keeps its ratio to the dark one's. The same wash
  that reads as a sheen over near-black is invisible over near-white.
- **`frosts`** is `translucency == Frosted && blur > 0`. Translucent and
  blurred are two different things, and the panel lets them come apart: a
  material may be see-through and refract nothing at all.

## Presets

Five named arrangements, which are the table this document used to describe in
prose. `Preset::appearance` is the whole definition of each.

| Preset | What it is |
| --- | --- |
| `Zervo` | The one that is finished: the chrome laid on the page, favicons down the spine, the shelf wherever the chrome is, every motion on, and the accent taken from the space you are in. **The colours are still the ones that shipped** — `candy` is the only field the palette reads for them and it has not moved — and there is a test pinning them for all ten accents in both themes. |
| `Candy` | The accent as a light source, no seam, everything glowing. The default. |
| `Flat` | Solid, a small radius, no sheen — a flat desktop toolkit. |
| `Liquid Glass` | Frosted with a blur of zero. Sheen and lift carry it instead. |
| `Material` | A generous radius, a longer shadow, no sheen, and glow standing in for the ripple's resting state. |

Beside them sit whatever the reader has kept: "Save as a preset…" names the
current arrangement and puts it in `Settings::saved`, and a saved one is
matched by `Appearance::same_look` rather than by a tag — an arrangement
somebody saved and then arrived at again by moving sliders is the same
arrangement, and a row that could not say so would show nothing selected while
the reader is plainly looking at it.

A preset sets the *material* and the chrome decisions that go with it. It
deliberately does not touch the theme or the accent: both have a section of
their own on the same page, `Auto` is a choice somebody made about their whole
machine, and a material has no business overruling either. Picking one replaces
the arrangement wholesale; moving any control afterwards relabels it `Custom`.

**Every arrangement keeps the Solid/Frosted switch**, including the two that
ship opaque. An arrangement whose card fill is 1.0 would otherwise offer a
control that changes nothing, so the moment Frosted is chosen the fill is held
at `THICKEST_TINT` — the same ceiling `tint_over` already refuses to go past,
for the same reason: beyond it the blur has stopped showing through and it is
not glass any more. `every_preset_can_be_frosted` is the test.

[`Material`]: ../zervo-core/src/theme.rs

## Beyond the material

`Appearance` also carries the handful of chrome decisions that are not about
what a surface is made of, because they belong to the same choice and the same
panel:

| Field | What it decides |
| --- | --- |
| `seam` | Where the chrome ends and the page begins: `Card`, `Frosted`, `EdgeToEdge`, `OneSurface`. See below. |
| `gap` | `CONTENT_MARGIN`, in points. Ignored once the seam closes it. |
| `candy` | How far the accent reaches into the chrome, measured against the 0.045 that shipped. |
| `workspace_accent` | Take the accent from the active workspace instead of one global choice. |
| `spine` | What a hidden sidebar leaves at the window's edge: nothing, tab ticks, or favicons. |
| `shelf` | Where the widget shelf is reachable from. |
| `align_nav` | Centre the sidebar's nav row on the macOS window controls. |
| `sweep`, `liquid`, `pill_progress` | Three pieces of motion, each described where it is declared. |

### The seam

Four steps rather than a toggle, because the distance between "a card on a
tray" and "one continuous surface" is not one decision. The page has to stop
painting a background of its own before closing the gap means anything, and the
gap has to close before the chrome can be laid *on* the page rather than beside
it.

`theme::{content_margin, content_radius, content_corners, page_ground}` are
where it lands, and they replaced two compile-time constants that were a
snapshot of one material. Past `Seam::Card` the page stops painting a base and
lays down only `page_veil` at `MIN_VEIL`, so what shows through is the window's
own backdrop rather than a second grey that never quite agreed with the first.
At `OneSurface` the sidebar is drawn as `Surface::Menu` glass laid on the page,
and the page reaches back underneath it.

One limit worth knowing: that last step applies to the new tab page only. A web
page arrives as a blit into the rectangle the engine was given, and there is no
way to ask it to leave a margin down its left side, so over a web page the
sidebar sits beside the content exactly as it does one step earlier — in the
same place, so nothing jumps when the kind of page changes.

## What is not there yet

- **Palettes are not themeable.** `resolve` builds the two palettes from
  hardcoded colours plus the accent and the candy ratio. An arrangement can
  change how surfaces are *built*, and how far the accent reaches into them,
  but not what colour they start from.
- **Themes for the *palette*.** An arrangement decides how a surface is built
  and how far the accent reaches into it. The two base palettes are still
  built from hardcoded colours.
- **Some numbers are still literals.** `LIGHT_INK`/`DARK_INK`, the tint targets
  and the flip margin belong to the material and are constants in `theme.rs`.
- **Platform backdrops are macOS only.** `SystemBackdrop::Frosted` maps to
  `NSVisualEffectView`; on Linux and Windows it does nothing. The rest of the
  material works everywhere, because it is all drawn by egui.

## macOS-specific pieces

Two things in the pipeline are not portable and are worth knowing about before
porting the rest.

**The window's own backdrop** is an `NSVisualEffectView` behind a transparent
framebuffer. The chrome paints a thin tint over it. That is why the chrome's
blur is the system's rather than ours, and why it is at full strength however
opaque the tint is.

**The content card's corners are cut out of the framebuffer**, not painted
over. The page arrives as a square blit, so its corners have to be
rounded by something; anything painted over them has to be opaque, and the
chrome beside them is a thin tint over a backdrop the window server composites
outside our framebuffer — unreadable, unreproducible, unmatchable. So the
corners are erased instead, with an antialiased destination-out pass, and the
chrome is drawn back over the hole at its own tint. Same paint, same backdrop,
no seam.

All four are cut. The top two used to keep an opaque mask instead, on the
reasoning that what sits above the card is opaque toolbar furniture and an
opaque patch would therefore match it. It does not: with a translucent
material the chrome up there is the same thin tint as everywhere else, so the
mask read as a dark notch hugging the arc — visible on a web page, where the
card is a blit that needs masking, and never on `zervo://settings` or the new
tab page, which round themselves and are never masked at all.

The window's own backdrop covers the whole frame view, not just the area below
the toolbar, so a hole at the top shows the same frosted backdrop as a hole at
the bottom.

On a platform with no system backdrop, neither piece is needed — the chrome is
opaque and a plain rounded rect does the job.

## Where to look

| File | What it holds |
| --- | --- |
| [`zervo-core/src/theme.rs`](../zervo-core/src/theme.rs) | `Appearance`, `Preset`, `Palette`, `Material`, `Surface`, `Tier`, `Edge`, `Seam`, `Spine`, `ShelfHome`, `Translucency`, `Backdrop` |
| [`zervo-core/src/glass.rs`](../zervo-core/src/glass.rs) | `glass::shapes` — every surface is drawn here, the bevel included |
| [`src/ui/settings_page.rs`](../src/ui/settings_page.rs) | The Appearance page: the presets, the pinned specimen, one control per field |
| [`src/backdrop.rs`](../src/backdrop.rs) | The framebuffer copy, and the per-corner erase |
| [`src/ui/`](../src/ui/) | The chrome; `ui/frame.rs` holds the card's frame and corner masks |

The unit tests in `theme.rs` and `glass.rs` are the contract: they pin the
class hierarchy, the radius tiers, the frost geometry, the tint arithmetic and
the text rule. If a theme change breaks one, read what it says before changing
it — several of them exist because the obvious answer was wrong.

Three of them are worth naming, because they are the ones a change to the
arrangement is most likely to break without meaning to:

- `the_classic_arrangement_resolves_to_what_shipped` pins `Preset::Zervo` to
  the exact colours that shipped, for all ten accents in both themes, against a
  spelled-out copy of the old arithmetic rather than against a reference to it.
  The preset's *shape* moved in 0.5.0 — the seam, the spine, the shelf and the
  motion switches all changed — and this is why that could be done without
  anybody's greys moving: `candy` is the only field the colours depend on.
- `every_preset_can_be_frosted` pins the Solid/Frosted control to doing
  something on every arrangement.
- `the_corner_scale_keeps_the_ladder_in_order` pins the radius ladder to
  staying a ladder at every scale, so a pill never ends up rounder than the
  window it is in.

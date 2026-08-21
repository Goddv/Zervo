# The theme engine

**Work in progress.** The seam described here is real and every surface in Zervo
is drawn through it, but a theme is still a Rust constant compiled into the
binary. There is no file format and nothing loads one at runtime. What follows
is what exists, and what the shape of a loadable theme would have to be.

The goal is that somebody can write a theme that makes Zervo look like Windows,
like GTK or Qt, like Android's Material, or like Apple's Liquid Glass, without
touching any drawing code. The engine is organised around that goal rather than
around the one theme currently in it.

## The idea

A theme decides two things and they are kept apart.

**A `Palette` is what colour things are.** Background, surface, accent, text,
border, shadow. It comes out of [`theme::resolve`](../src/theme.rs), which takes
the mode (Auto/Light/Dark), what the system says, and the accent.

**A `Material` is how things are built.** Corner radii, how opaque a surface is,
whether it has a sheen, how far a shadow reaches, whether it frosts what is
behind it and by how much, row heights, control padding, animation time. One
constant — `Material::GLASS` — carries all of it.

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

`Tier::{Hairline, Control, Row, Card, Panel, Pill}` → `Radii::of(tier)`.

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

## What a theme would change

A `Material` with `frosts: false`, no sheen, a heavier edge and square corners
is a flat desktop toolkit, and none of the drawing code changes. Concretely:

| Want | Set |
| --- | --- |
| Windows-style flat chrome | `frosts: false`, `translucency: false`, `radius` small, `edge_*` up, `sheen_* : 0.0` |
| GTK/Qt | as above, with `row_height` and `control_padding` to taste |
| Android Material | `radius` generous, `shadow_reach` up, `sheen_*: 0.0`, `glow` for the ripple's resting state |
| Liquid Glass | `frosts: true`, `blur: 0.0`, `sheen_*` and `lift_*` up — a material may be translucent without blurring |

The full field list is on `Material` in [src/theme.rs](../src/theme.rs); each
field is documented where it is declared, which is the authoritative list.

## What is not there yet

- **No file format and no loader.** `Material::GLASS` is a `const` and the only
  one. A theme is a recompile.
- **Palettes are not themeable.** `resolve` builds the two palettes from
  hardcoded colours plus the accent. A theme can change how surfaces are
  *built* but not what colour they start from.
- **`Material::GLASS` is named but not chosen.** There is a `name` field and
  nothing reads it.
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

**The content card's bottom corners are cut out of the framebuffer**, not
painted over. The page arrives as a square blit, so its corners have to be
rounded by something; anything painted over them has to be opaque, and the
chrome beside them is a thin tint over a backdrop the window server composites
outside our framebuffer — unreadable, unreproducible, unmatchable. So the
corners are erased instead, with an antialiased destination-out pass, and the
chrome is drawn back over the hole at its own tint. Same paint, same backdrop,
no seam.

The top corners are *not* cut: a hole along the top of the window shows what is
behind the window rather than the backdrop. They keep an opaque mask, which is
correct there because what sits above the card is opaque toolbar furniture.

On a platform with no system backdrop, neither piece is needed — the chrome is
opaque and a plain rounded rect does the job.

## Where to look

| File | What it holds |
| --- | --- |
| [`src/theme.rs`](../src/theme.rs) | `Palette`, `Material`, `Surface`, `Tier`, `Translucency`, `Backdrop` |
| [`src/glass.rs`](../src/glass.rs) | `glass::shapes` — every surface is drawn here |
| [`src/backdrop.rs`](../src/backdrop.rs) | The framebuffer copy, and the corner erase |
| [`src/ui.rs`](../src/ui.rs) | The chrome, and `theme::apply` feeding egui's own styling |

The unit tests in `theme.rs` and `glass.rs` are the contract: they pin the
class hierarchy, the radius tiers, the frost geometry, the tint arithmetic and
the text rule. If a theme change breaks one, read what it says before changing
it — several of them exist because the obvious answer was wrong.

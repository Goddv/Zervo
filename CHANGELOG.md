# Changelog

## 0.4.0 — 21 August 2026

**A macOS release first.** Everything below is built and used on macOS. The
Linux and Windows packages are produced from the same tag and the material
itself is portable — it is all drawn by egui — but the window's frosted
backdrop is an AppKit feature, and the corner work behind it has not been tried
anywhere else. Patches for the other two are v0.4.1. Reports from either are
genuinely useful in the meantime.

**A theme engine, documented.** [docs/THEMING.md](docs/THEMING.md) describes the
seam a theme for another platform is written against: what a `Material` decides,
what a class carries, how frosting is supplied, and what a Windows, GTK, Android
Material or Liquid Glass theme would set. It is honest about what is not there —
a theme is still a Rust constant, there is no file format, and nothing loads one
at runtime.

**Surfaces have a class, and the class carries the material.** Card, Menu and
Input, the way an element has one in CSS. A call site asks for what a thing *is*
and the material answers with the numbers, so a menu can never be heavier than a
card by accident and a text field can be heavier than both on purpose. Corner
radii are named on the same principle — Hairline through Pill — and the material
says what each comes to.

**Everything frosts against the page, not just the wallpaper.** A panel over a
web page had nothing to sample: the page is opaque pixels the engine has drawn,
and no amount of translucency turns those into a blur. The window now takes a
small blurred copy of whatever the page is showing and hands it to the palette,
so the downloads card, the favourites card and every menu are made of the same
glass over a website as over a photograph. The new tab page and Settings supply
one too, so a menu opened over either frosts against it rather than falling flat.

**A surface belongs to its theme first.** A dark menu at a fifth of its own
colour, opened over a white page, is four fifths white — still frosted, still
blurred, and no longer a dark menu. The tint now thickens exactly as far as it
must to keep a surface on its own side of the middle and no further; over a page
the theme already agrees with, nothing changes. Two numbers make a surface's
weight and only one of them was being adjusted, which is how a dark-mode card
over a bright wallpaper came out pale enough to lose its text.

**Text follows what it lands on.** Cards on the new tab page, and the greeting,
clock and photo credit beside them, each ask about their own patch of the
picture rather than taking one answer for the page. A photograph is dark sky at
the top and bright water at the bottom, and pale text set for the sky disappears
into the water. It picks by contrast ratio rather than a brightness threshold —
the crossover is not at half — with hysteresis, so text does not flip as a page
scrolls past it.

**The content card's corners.** They had been a few per cent off their
surroundings at every zoom, because the mask rounding them has to be opaque
while the chrome beside it is a thin tint over a backdrop the window server
composites outside the application's own framebuffer. The bottom corners are now
cut out of the framebuffer instead — antialiased, with a destination-out pass —
and the chrome is drawn back over the hole at its own tint: the same paint on
the same backdrop, so there is nothing left to match. Measured after, all four
corners are within one part in 255 of the chrome an inch away from them.

**The card's edge is yours.** Outline, shadow and halo are three toggles in
Appearance, with an amount for the last two. The outline is on, the other two
off. The halo used to be drawn unconditionally to hide the corner seam; with the
seam gone it is a decision rather than a repair.

**Fixes.** The widget shelf clipped its own widgets' shadows off along every edge
it touched. The new tab page did the same to the top and bottom rows of cards.
The release bundle would not start on a machine with GStreamer installed — its
build scripts put the framework on the link path whether or not the media
feature asked for them, and `-lz` found a zlib there with no rpath to resolve it.
The new tab page blinked eight times a second, because the backdrop capture
turned the scissor test off and never turned it back on.

**Two steps, and every surface answers to them.** Appearance offers Solid and
Frosted; Frosted is the default. The Sheer step and the blur control have gone
— Sheer needed hand-tuning to be worth anything, and how far a material blurs
is something the material already knows. The blur is pitched to match what the
system's own backdrop does to the desktop, so a card on the wallpaper and the
window on the desktop read as the same glass.

And it really is every surface now. egui draws its own popups, menus, tooltips
and dropdowns from a style that never saw the material, so the add-widget menu
and every dropdown in Settings were opaque slabs in a window made of glass.
They come from the material too, along with the fill behind anything you type
into — pitched heavier than a card, because a text field is the one place where
what is behind it competes with what you are reading.

**Cards are made of the same glass as the window.** They were not, and it
showed: the window's blur comes from the system, which blurs what is behind it
at full strength however opaque the chrome is, and paints a tint over the top.
A card was thinning its *blur* along with its tint, so it mixed a blurred
backdrop with the sharp one underneath — a smear rather than glass, and visibly
a different material from the window holding it. A card's blur is full strength
now and only its tint answers to the setting, exactly as the chrome does. The
two also drew from separately tuned numbers, which is how they drifted; there
is one number now.

**One glass setting, not two.** Chrome opacity has gone as a separate control:
it and the material setting were two answers to the same question, and keeping
them in agreement was your job. The three steps drive both, and they are
pitched against the range that slider offered rather than against the value it
happened to default to: **Frosted** is the frosted-glass window Zervo is for,
with the desktop legible behind it, now applied to everything the material
draws rather than to the window alone; **Sheer** takes that as far as it goes
while a sidebar is still a surface rather than a hole; **Solid** is for anyone
who never wanted any of it.

**Blur levels did not reach the pixels.** They were baked at a radius that ran
to a fifth of the blurred copy's own width, where the passes reach past the
edges and what they clamp against dominates — so Deep came back with *more*
local contrast than Medium rather than less, and moving the setting appeared to
do nothing. The copy is larger, the radius smaller, and it is capped relative
to the copy's own size. The frost is markedly lighter throughout as a result.

**Text reads better in both themes.** Muted text is brighter in the dark theme
and darker in the light one. It is the caption colour on every list row and
every explanatory line in Settings, and it now has to survive being read off a
translucent surface with a photograph behind it.

**Three steps of glass instead of a slider.** Card opacity is gone. It ran from
"a card" to "not there", and almost everything in between is a surface that has
stopped holding text up. Appearance offers Solid, Frosted and Sheer, and they
reach every surface the material draws rather than a hand-picked few — the
window's own chrome, the cards, the menus, the shelf, the new tab page. Only the fill is
thinned; the hairline and the shadow keep their strength at every step, because
they are what say where a surface ends.

**Blur, in three steps too**, and only offered by a material that blurs. It
scales the material's own radius rather than replacing it, so a material that
blurs gently and one that blurs hard keep their character. Changing it re-bakes
the frost from the picture already on disk — no refetch. A material built on
refraction rather than blur, or a flat toolkit one, sets `frosts: false` and the
setting stops being offered.

**An accent colour of your own.** First in the row, marked with a pencil rather
than pretending to be another swatch, opening a picker seeded from whatever is
in force. Five more presets behind it: Coral, Teal, Violet, Lime and Graphite. A
colour you mixed is kept as you mixed it in both themes.

**The new tab page's backdrop is chosen on the new tab page.** All eight of them
— Plain, Gradient, Grid, Mesh, Aurora, Waves, Particles and a photograph — are
in the Backdrop menu in the page's own header, grouped into Static, Animated and
Wallpaper provider, with the Openverse subjects and "Choose a file…" under the
last of those. It is a decision about *that* page, taken while looking at it;
making it from another window and coming back to see what it did is two steps
and a memory test. The Settings page no longer carries a copy.

## 0.3.6 — 21 August 2026

A new tab page you arrange yourself, tabs you can drag, trackpad swipes,
and fixes to things that looked finished and were not.

### Surfaces are made of a material

Zervo's chrome was glass by repetition rather than by design: `glass::shapes`
knew the recipe, and every one of seventy-seven call sites knew a corner
radius. That had two costs. Frosted glass could not actually frost — the
material had nothing behind it to work with, so each caller papered over the
gap with an opaque backing of its own, which on a wallpaper is a flat slab and
not frosted glass. And a second look, for any other platform, would have meant
editing all seventy-seven.

So the recipe is a thing now. A **`Material`** holds every number a surface is
built from — its fill, its sheen, its edge, how far it lifts off the page, how
far its shadow reaches, whether it frosts what is behind it and how far that is
blurred — along with the corner-radius tier, the row height, the control
padding and the animation time. It hangs off the palette, the way card opacity
already did, so it reaches every surface in the application without a single
signature changing. `Material::GLASS` is Zervo's own, and every value in it is
exactly what was previously written in place, so nothing looks different except
where it now frosts.

**Corners are named rather than numbered.** `Hairline`, `Control`, `Row`,
`Card`, `Panel` and `Pill` are sizes; the material decides what each comes to.
Those six cover seventy of the seventy-seven radii in the tree, which is what
made a tier the right shape — the numbers were already a system, just an
undocumented one. egui's own widgets are styled from the material too, so a
theme reaches the stock buttons and combo boxes instead of leaving them looking
bolted on.

Nothing here changes what Zervo looks like. What it changes is that a Fluent,
GTK, Qt or Material 3 theme is now a struct somebody writes rather than a fork
of the drawing code — a material with `frosts: false`, no sheen, a heavier edge
and square corners is a flat desktop toolkit, and not one line of the chrome
has to know.

**And the frost is real.** egui cannot blur what is behind a shape while it
draws it, and it turns out not to need to: the only thing ever behind the
chrome is a wallpaper, which is a still image. It is blurred once, when it is
decoded, and the material samples that blurred copy through the same mapping
the sharp one is drawn with. A caller puts a picture behind the chrome, hands
the palette a blurred copy of it, and every card, pill and menu on top is
frosted against it — with no change at any call site at all.

A surface frosts over the part of it that is actually on the picture, rather
than only when all of it is. Asking for the whole surface is the obvious rule
and the wrong one: a card scrolled half off the top of the page, or carried
past its edge under the pointer, would stop frosting all at once — and since
the fill recipe follows the frost, it would not merely lose its blur, it would
change material between one frame and the next. It is the overlap now, so it
degrades continuously, and the first automated tests in the repository cover
the four directions a surface can hang off an edge in.

### The new tab page

**It is a dashboard, and you lay it out.** The old page was a column down the
middle — a clock, a greeting, a mark, a search box, a row of pinned tabs — and
five checkboxes in Settings to turn each of them off. Everything is a card on a
twelve-column grid now: drag one where you want it, drag its corner to resize
it by whole cells, and take it off with the ✕. Press **Customise** in the
page's header to arrange it and **Done** when you have finished; outside edit
mode the cards are ordinary, so clicking a link opens it rather than picking
the card up.

Dragging only works in edit mode on purpose. A page where every card is always
draggable has to guess whether a press on a link meant *open that* or *pick
this up*, and it guesses wrong often enough to be worse than a button.

Twelve columns, always. The cells narrow with the window rather than the grid
losing columns, so a card at column nine is at column nine in any window and an
arrangement survives a resize. If you fill more rows than the window shows, the
page scrolls.

**Thirteen kinds of card**, and most of them show something the browser already
knew: your pinned tabs, the sites you visit most, what you looked at recently,
your favourites, your downloads, your workspaces, and what the page is playing.
The rest are a search box, a clock, world clocks, the Zervo mark, a note, and a
to-do list. Every one of them has something to say when it is empty, because a
blank rectangle looks broken and "nothing here yet" does not.

**Wallpapers, from places that let you have them.** The page can put a
photograph behind the cards, fetched from Wikimedia Commons' picture of the day
— a day picked at random out of the last ten years, so it is the archive rather
than today's — or from Openverse, searched for one of eight subjects. Neither
wants an account, both publish under licences that allow this, and you can
point it at a file of your own instead. Change it manually, once a launch,
hourly or daily.

The credit line along the bottom is not decoration. Most of what comes back is
CC BY or CC BY-SA, which are licences with a condition attached, so the title,
the photographer and the licence travel with the picture and are drawn with it.
Clicking it opens the page it came from. There is no setting to turn it off.

Fetching, decoding and downscaling all happen on a thread of their own — a
wallpaper is never worth stalling a frame for — and the last one is cached
beside your settings, so a launch has something to show before the network has
said anything.

**The photograph holds still when the widget shelf opens.** Dragging the
navigation bar taller takes space off the top of the page. Refitting the
picture to what is left rescales it on every frame of the drag, which reads as
the wallpaper breathing; it is cropped instead, and allowed to drift down at
half the rate the page does, so the shelf reads as a bar sliding over a
wallpaper behind it rather than the whole page moving as one sheet.

**A veil at the edges rather than over everything.** The two things on the page
not drawn inside a card — the header controls and the credit line — sit at the
top and bottom edges, and a veil strong enough to carry white text everywhere
is strong enough to throw the photograph away. Only the strips holding text are
darkened, so the middle of the picture survives. The four header controls sit
on small pills of their own besides, because a bare word on somebody else's
photograph is a control nobody can find.

**A note and a to-do list**, kept in the library beside your favourites rather
than in your settings, because they are your writing and not your preferences.

**World clocks that know about summer time.** Pick cities in Settings › Layout;
each face reads its zone from the compiled-in IANA table, so London says BST in
August without anyone maintaining it.

**Tabs move.** Drag one up or down its list to reorder it, or into another
workspace entirely — the page comes with it, still loaded, rather than being
closed and reopened. Drop a tab *onto* another instead of between two and the
pair become a workspace of their own, which opens asking what to call it. It
guesses at the two tabs' shared host first, since grouping two pages from the
same site is the common case.

**Trackpad swipes**, bound in Settings to back, forward, the sidebar, the
widget shelf, the next or previous workspace, or a new tab. A quick straight
flick is a gesture; a slow or wandering one is a scroll and is left alone.
Two-finger vertical is only read over the bar above the page, because
everywhere else it is how you scroll.

**Card opacity**, a second slider beside chrome opacity. It reaches the
favourites and downloads cards, the widget shelf and the new tab page's cards,
and it goes all the way to nothing. It deliberately does not reach the chrome's
own furniture — the tab rows, the address bar, the settings sections — which
are not cards in the sense the setting means.

### Fixes, mostly to things that looked finished and were not

**The wallpaper never appeared at all in some cases, and never faded in in
any of them.** The fade-in was keyed on the texture's own id, and epaint never
reuses one — so egui, asked to animate an id it had not seen, returned the
finished value immediately and every picture arrived at full strength. The
worse half was underneath: when the ramp did start at zero, the page returned
early without asking for another frame, so nothing advanced it and the
wallpaper stayed invisible for the life of the tab. A frame that draws nothing
still has to ask for the next one.

Fixing that woke a guard the bug had kept dead, and for one frame on every
picture-to-picture swap the page went somewhere else entirely — cards on their
opaque backings instead of frosted glass, the accent gradient over the top, the
text off white onto the palette, and the board twenty points taller because the
credit line had gone. The guard is not needed now that a backdrop can carry an
arrival of zero and simply draw nothing, so the frame is allowed through and
the page holds still.

**Two of the four internal pages could not be typed in.** Every one of them
shows its address in the address bar — `zervo://settings`, `zervo://newtab`,
`zervo://history`, `zervo://downloads` — and putting either of the middle two
back into it opened Settings, because the routing tested for "downloads" and
took everything else as the settings page.

**The extensions button had somebody else's icon.** It drew the sliders glyph,
with a comment explaining that Phosphor's puzzle piece "is not in the vendored
subset". The vendored font is the complete Phosphor regular face — 1544 glyphs
— and the puzzle piece was in it all along; the subset the comment meant was
the list of constants, not the font.

**Overlays swallow their own clicks.** The favourites card, the downloads card
and anything else floating over a page relied on a hand-maintained list of
rectangles to decide whether a click belonged to the chrome or the page. Every
overlay anyone forgot to add to that list sent its clicks straight through, so
the favourites card opened, showed you your favourites, and then handed your
click to the web page underneath. Asking egui which layer is under the pointer
covers all of them, including ones added later.

**Favourite renames save.** A rename committed only on Enter and on losing
focus, so an edit and a click elsewhere lost it. There is a ✓ to keep it and an
✗ to drop it, and Enter and Escape still do the same.

**Downloads have a card of their own**, opening on hover like the favourites
one: stop, start again, reveal in the file manager, open. Pause and continue are
drawn and disabled, with a note saying why — Servo streams a response to disk
with no way to suspend it, so a pause button that silently cancelled would be a
lie. Right-click gives copy link, copy file name, show in folder, open, start
again and remove.

**Every setting is in the category it belongs to.** Several appearance settings
were sitting under General; Customize is now Layout; the downloads and
compatibility toggles were only reachable by editing the file. There is a reset
that puts the navigation bar, the widgets and the sidebar back the way they
started.

**Cards have one shadow and one edge.** Each floating card painted an opaque
backing and then the glass material over it, which put the material's drop
shadow *inside* the card, showing through as a dark rim — worst at the corners.
They also stacked four rounded rects at the same radius, so antialiased corner
pixels composited four times and a corner came out harder than the edges beside
it. The material does both jobs now, and the shadow is the content card's: a
ring with a quadratic falloff rather than epaint's edge feathering, which is not
a drop shadow and was the banding.

**Changing a setting no longer jolts the window.** Every settings write
reapplied the whole theme — restyle egui, reset the window appearance, retune
the frosted material, and tell every webview its `prefers-color-scheme` had
changed, which makes the page relayout — plus a fresh Dock icon. For "outline
around content", and once per frame while dragging a slider. It now happens when
the theme actually changes.

## 0.3.3 — 19 August 2026

Windows, and with it a build for every platform Zervo targets.

### Windows

There is a Windows x64 build now: one self-contained `Zervo.exe`, since almost
all of Servo links statically there. It is a GUI application rather than a
console one, so launching it does not leave a terminal sitting behind the
window — debug builds keep the console, which is where the logs go.

Two things stood between here and a build, and neither was Zervo's code. Cargo
could not check the engine out at all: the Servo repository carries
web-platform-tests, whose test262 paths run past the 260 characters Windows
allows by default, so the build failed ten minutes in with "path too long".
Long paths are enabled in the registry and in git now, and cargo fetches with
the command line git — its own git library ignores `core.longpaths`, so setting
the config without that change looks right and fails identically. And
SpiderMonkey needs moztools staged under the target directory, which it does not
fetch for itself; without it the build panics rather than explaining.

Everything that differs by platform outside macOS now goes through one module,
because the non-macOS paths had quietly assumed Linux. The file dialog is the
system one through PowerShell, Explorer gets a real reveal — which no other
desktop offers — and saved passwords use DPAPI. Windows has no keychain command
that hands a password back, but it does have per-user encryption, which is the
property that matters.

No media on Windows yet: GStreamer there means an MSI and a bundling step, which
is its own piece of work. Downloads are in.

### Every platform

| Platform | Artifact |
| --- | --- |
| macOS (Apple Silicon) | `.dmg`, GStreamer bundled |
| Debian and Ubuntu | `.deb`, dependencies derived from the binary |
| Fedora | `.rpm`, built inside Fedora |
| Windows x64 | `.exe`, self-contained |

Still true of all three of the new ones: they compile, package and install.
Nobody has run Zervo on Linux or Windows, and whether it opens a window there is
untested.

## 0.3.2 — 19 August 2026

The navigation bar became something you can arrange, and Linux got its first
packages.

### The bar is yours to arrange

The wand beside the add-widget button turns the row into things to move rather
than things to press. Drag items within a group or across to the other side of
the address pill, with a caret showing where a drop lands; the x takes an item
off; whatever is off waits in a tray underneath where a click puts it back. Both
groups are remembered.

This needed the bar to stop being a fixed sequence of calls — it is a list of
items now, each carrying its own icon, tooltip, enabled state and action. A
sequence of calls cannot be reordered by definition.

### Widgets

Cells scale with the window. The column count is fixed at twelve and the cells
narrow, rather than the grid losing columns — which used to shove everything on
the right into the same place on a small window. A widget at column six is at
column six in any window, and positions are never rewritten behind your back.

Widgets can be dropped anywhere on the shelf, including a row with nothing above
it, with a thin line showing the cell they would snap to. They resize by
dragging a corner in whole cells, or from a list of sizes behind the same
corner. The shelf's own add tile is gone: the bar's plus does the same job
without spending shelf space on it.

### Fixes

Hover-only controls could not be clicked. Remove and resize on widgets, and
remove and rename on favourites, were shown only while their card reported
itself hovered — but registering those controls on top of the card is exactly
what stops the card counting as hovered, so they blinked at the pointer and were
unreachable. Hover comes from the pointer position now.

The favourites card was unusable for a related reason: it grew outward from the
star's centre, so while animating it sat on top of the star and swallowed the
click that had opened it. It grows downward from underneath now, and takes no
input until it has arrived. Favourites can be renamed, removed, and shown as
tiles instead of a list.

The shelf can reach every row it offers. The grid allowed four and the bar's
maximum height allowed two, so it stopped at two however hard it was dragged.

### Linux

First `.deb` and `.rpm` builds. The `.rpm` is built inside Fedora and the `.deb`
on Ubuntu, because a binary linked against one distribution's libraries will not
reliably run on the other, and `rpmbuild` derives its requirements from the
binary. Neither package carries a hand-written dependency list.

Worth being straight about: these are the first Linux builds that have ever
existed, and nobody has run one. They compile and package. Whether Zervo opens a
window on Linux is untested — the rendering context, X11 versus Wayland and the
GL setup have never been exercised.

## 0.3.1 — 19 August 2026

Two fixes on top of 0.3.0, one of which is why pages looked wrong.

### Sites rendered as though on a phone

`window.screen` was 0x0. Zervo never implemented `screen_geometry`, and the
engine's fallback gives nothing, so any site that sizes itself against the screen
rather than the viewport concluded it was on a very small device. Google served
its mobile layout into a desktop window, which is what made this obvious, but it
would have affected anything doing the same thing.

The monitor size and window rectangle are reported now, which also fixes
`window.outerWidth`, `outerHeight`, `screenX` and `screenY`. `availWidth` and
`availHeight` are still the full screen rather than minus the menu bar and Dock.

### Telling people how to open the app

macOS refuses to open anything downloaded from the internet unless it is signed
with a paid Developer ID, and says the app "is damaged and can't be opened",
which sounds like a broken download rather than a policy. The disk image now
contains the instructions, and `docs/PACKAGING.md` has been corrected: it no
longer suggests Control-click → Open, which
[Apple removed in Sequoia](https://developer.apple.com/news/?id=saqachfa), and it
warns off `spctl`, which reports `rejected` for this app whether or not it is
quarantined and so tells you nothing about what a user sees. Apple's own `gktool`
says the build is "allowed by system policy" once the download flag is gone.

### Not fixed: streaming video

YouTube and friends still say they cannot play video. Servo has no Media Source
Extensions, and adaptive streaming is built on them; it also reports no H.264
support to `canPlayType`, even though the bundled GStreamer decodes H.264 fine
from a local file. Both are engine-side. Written up in
[docs/PARITY.md](docs/PARITY.md).

### Commits

```
ee2d944  app: report the real screen geometry
4318905  packaging: tell people how to open the app
```

## 0.3.0 — 19 August 2026

The release where it starts behaving like a browser rather than a viewer.

### Pages can ask you things now

`show_embedder_control` wrote `alert()` to the log and dropped everything else on
the floor. Dropping one of these controls takes its default, which is "the user
cancelled", so a whole category of ordinary web behaviour quietly did nothing at
all. That one function was the reason:

- `<select>` dropdowns never opened
- file uploads were impossible
- `confirm()` was always false and `prompt()` always null
- `<input type=color>` did nothing
- right-click did nothing, anywhere
- there was no IME, so no way to type Chinese, Japanese or Korean

All of those work now. Dialogs come up over the page labelled with the origin
that raised them, because the text in them is written by the site and shouldn't
read like Zervo asking. `<select>` handles optgroups and multi-select. The file
picker is the real macOS open panel rather than something homegrown, since that's
one thing the OS does better than we would. The context menu is built from the
items Servo hands over, so back/forward/reload, copy link, open in new tab and
the rest all work off one code path. Input methods follow servoshell's approach,
including the fiddly part where dismissing an IME because focus moved must not be
reported to the page, or it blurs the element that just got focus.

### It remembers you

Cookies, logins, localStorage and the HSTS list are kept between launches. Two
separate things were stopping that, and fixing either one alone would have
changed nothing.

Servo writes its cookie jar into a config directory and skips writing entirely
when it hasn't been given one, which it hadn't. And it only writes at all when
the engine is told to shut down, which happens when the Servo handle is dropped —
except every webview holds a reference to the shared delegate, which is a
reference back to the thing that owns the engine, so nothing was ever dropped.
Both ends fixed.

One caveat worth knowing: that flush only happens on a clean quit. Servo has no
incremental save and no flush API, so a force quit still loses the session.

⌘Q also exists now. There's no menu bar to carry the standard Quit item, so
before this there was no way to quit that ran any of the shutdown at all.

### Downloads, in the builds you actually download

Servo has no download support, so this needs a patched engine, and until now that
meant building one yourself. Released builds are now compiled against a
[fork](https://github.com/Goddv/servo/tree/zervo-downloads) carrying the patch,
pinned to a revision. A plain `git clone` still builds against the published
crate with no setup, because CI appends the patch line rather than it being
committed.

### Sites stop turning you away

Servo's user agent already claims Firefox 140, but it keeps a `Servo/0.5.0` token
and leaves out `Gecko/20100101`, and enough sites match on exactly those to serve
a "browser not supported" page. Zervo now presents the plain Firefox string. It's
a setting if you'd rather be honest about it.

### Audio and video

`<video>` and `<audio>` play, in the builds you download. This needs GStreamer,
which on macOS means the official framework rather than anything you can `brew
install`, so `scripts/bundle-gstreamer.py` copies the 92 libraries and 38 plugins
it needs into the .app and rewrites their install names. The bundle grows by
about 150 MB, which is the price of not requiring everyone to install a framework
first.

Three separate things had to be right before a single frame decoded, and each one
hid the next: the binary needed an rpath of its own, `@rpath/…` references had to
be rewritten rather than left alone, and the rewritten paths had to be *shorter*
than the originals or `install_name_tool` refuses to write them at all. Build it
with `--features media`.

### Local files open

`file://` URLs went through the "looks like a hostname" branch of the address bar
and came out as `https://file:///…`, so opening a local file was impossible —
from the address bar or the command line. Absolute paths and `~/` work too.

### Page console output

There are no devtools, so `console.log` went nowhere at all. It goes to the
terminal now, which makes debugging a page in Zervo enormously less annoying.

### Commits

```
17e4900  app: show page console output
28978fc  ui: open local files
17cce01  media: make the bundled GStreamer actually load
d7357f3  media: bundle GStreamer so audio and video work
9f9d8a8  ui: let a popup survive its own opening click
51823c9  ui: wire up input methods
a07646e  ci: build releases against the patched engine
58bf8a8  ui: answer what the page asks for
636ac09  engine: keep cookies, and present as plain Firefox
8b55b80  docs: write down what is missing before this is a usable browser
184bba1  README: show the screenshots properly
```

## 0.2.0 — 19 August 2026

A round of fixes to the chrome. The autohide sidebar was the big one: it looked
fine and was completely unusable.

### Sidebar autohide

The hidden sidebar slid out when you put the pointer against the left edge, then
went away again the moment you moved toward anything in it. You could see it, you
just couldn't click it. The reveal was worked out fresh every frame from "is the
cursor within 14px of the window edge", so walking the mouse over to a tab took
you out of that strip and the sidebar was swapped straight back out for the
collapsed handle.

Now the edge only opens it. What keeps it open is the pointer being anywhere over
the sidebar itself, with a bit of slack past its right edge and a quarter second
of grace on the way out. It also won't vanish halfway through a drag, or while a
menu is open. If it's already sliding away when you go back for it, you get it
back.

Two more things fell out of that one:

- The reveal used to be a real panel, so it took up layout space. Every peek
  shrank the content area and resized the webview, which means Servo relaid out
  the entire page, just because the pointer brushed the window edge. It floats
  over the content now and nothing underneath it moves.
- Clicks and scrolls over the revealed sidebar were going through to the page as
  well, since "is the cursor over web content" was a plain rectangle test against
  the content card, and the sidebar sits inside that rectangle.

The width it opens at is whatever you last dragged the sidebar to. That's saved
between runs now (`sidebar_width` in settings.json) rather than resetting to 248
points on every launch.

⌘L did nothing at all with the sidebar hidden. The address box lives in the
sidebar, so there was nothing on screen to focus, and worse, the pending focus
request sat around and fired later at whatever random moment you next happened to
brush the window edge. It opens the sidebar now.

There was also no toggle for autohide anywhere in Settings, despite it being on
by default. It's under Appearance.

### The content card

- The shadow around the card was three concentric outlines. Each one has a hard
  edge, so together they read as bands rather than as a shadow. It's a single
  mesh with a proper falloff now.
- The rounded corners looked stair-stepped. Two things were doing it: the arcs
  were a fixed 12 segments, which works out to roughly two physical pixels per
  facet on a Retina display, and epaint doesn't antialias meshes at all. The
  segment count follows DPI now, and each arc gets retraced with an antialiased
  line on top.
- With translucent chrome you could just make out the card's square bounding box
  running past the rounded corner. The corner masks have to be fully opaque,
  since hiding the square corners of the page underneath is their whole job, so
  against 85% chrome they left a faint solid patch with a straight edge on it.
  The chrome now ramps up to opaque along the card's rounded edge and fades back
  over the next few pixels, which puts the transition on the curve where it
  belongs instead of on the box.

### Commits

```
3474c85  ui: make a revealed sidebar usable
1494d45  ui: hide the content card's square bounding box
992700c  ui: stop the content card's corners looking stair-stepped
a5430f2  ui: smooth the card shadow
```

## 0.1.0

First release. Sidebar-first chrome on top of Servo 0.5.0, with workspaces,
pinned essentials, an internal settings page and a download manager. macOS only.

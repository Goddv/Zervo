# Living with the Servo engine

Zervo depends on the published `servo` crate:

```toml
servo = { version = "0.5.0", default-features = false, features = [...] }
```

Servo releases to crates.io roughly monthly, with every dependency versioned
and no git sources. Using the registry release rather than a git dependency
keeps clones small (the Servo monorepo is ~1.9 GB), keeps `Cargo.lock` stable,
and makes CI caching work.

If you need something newer than the latest release, a git dependency pinned to
an exact `rev` also works — never a bare branch, since Servo lands dozens of
commits a day and the embedding API is not stable:

```toml
servo = { git = "https://github.com/servo/servo", rev = "<40-char sha>", ... }
```

## Updating to a newer Servo

1. Bump the version (or `rev`) in `Cargo.toml`.
3. Check `rust-toolchain.toml` against Servo's — they bump it every few months,
   and the build fails confusingly if they disagree.
4. `cargo build` and fix the fallout. The embedder API drifts; typical breakage
   is renamed types, new trait methods, and changed enum variants. Recent
   examples: `base::id` became `servo_base::id`; `FetchTaskTarget` lost
   `process_request_eof` and gained `process_response_length_hint`.
5. If you use `--features engine-downloads`, re-apply the patch (below).
6. Run through the smoke checklist in [TESTING.md](TESTING.md).

Keep the update as its own commit, with the Servo revision and date in the
message, so a regression can be bisected to an engine bump rather than to
chrome changes.

## The downloads patch

Servo has no download support: `Content-Disposition: attachment` is ignored,
`<a download>` is an open TODO, and a response the parser cannot render becomes
a page reading *"Unknown content type (application/zip)."*
Tracking issue: [servo#40210](https://github.com/servo/servo/issues/40210).

`patches/servo/0001-embedder-file-downloads.patch` adds:

- `DownloadHandling::{WhenUnrenderable, Always}` and a suggested filename on
  the navigation request.
- Detection in `main_fetch` using `MimeClassifier::get_media_type` — the
  parser's *own* notion of what it can render, so the two cannot drift — plus
  `Content-Disposition: attachment`.
- Three `WebViewDelegate` methods: `notify_unsupported_response`,
  `notify_response_chunk`, `notify_response_eof`. The engine keeps performing
  the transfer, so cookies, auth, redirects and cache all still apply; the
  embedder only chooses where the bytes land.
- `<a download>` support, including the spec's same-origin rule for the
  suggested filename, and the `download` IDL attribute.

The design follows what Servo's maintainers settled on in their Zulip
discussion, and builds on a prototype branch by jdm
(`jdm/servo:download-integration`).

### Applying it

Cargo cannot patch a dependency in place, so build against a patched checkout:

```bash
# once
gh repo fork servo/servo --clone   # or fork in the web UI and clone
cd servo
git checkout -b zervo-downloads bd220a152bc…
git apply /path/to/zervo/patches/servo/0001-embedder-file-downloads.patch
git commit -am "embedder: offer unrenderable responses to the embedder"
git push -u origin zervo-downloads
```

Zervo already points at the fork: `.cargo/config.toml` carries a
`[patch.crates-io]` entry pinned to a revision of `zervo-downloads`. Bumping the
engine means editing that one line — it used to live in three workflow files as
well, which is exactly the sort of thing that drifts.

To work against a local checkout instead, replace the `git`/`rev` entry with a
path:

```toml
[patch.crates-io]
servo = { path = "/absolute/path/to/your/servo/components/servo" }
```

Note that patching `servo` alone is not always enough: because the patch
touches `servo-net` and `servo-script` too, you may need to redirect those
sibling crates to the same checkout.

and build with `--features engine-downloads`.

The patch is *not* optional at the moment, even without that feature. Servo
renamed `MouseButton`'s variants to `Primary`/`Secondary`/`Auxiliary` on
21 August 2026 and `src/main.rs` follows the new names, while the newest `servo`
on crates.io is still 0.5.0 with the old ones. Until a release carries the
rename, the registry crate does not compile Zervo at all.

### Upstreaming

Don't send this patch upstream. The work is already assigned on servo#40210
and grant-funded, and Servo's contribution policy forbids AI-generated
contributions. If you want to help, test their implementation and report what
you find, in your own words.

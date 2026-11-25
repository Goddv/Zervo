# Codex Session Notes — Zervo

> Last updated: 2026-11-25
> Previous codex session context (autoupdate fix): See https://github.com/Goddv/Zervo

---

## Recent changes made in the last session

### 1. Merged 0.6.1.3 release back into `main`

The 0.6.1.3 release (translations + Fluent updates) was built from a
separate release branch (`codex/zervo-0.6.1.3`) that was **never merged
back to `main`**. This meant users on 0.6.1.2 could never receive it.

**Fixed by:**
- Merged `codex/zervo-0.6.1.3` (3 commits) into `main`
- Updated `CHANGELOG.md` — 0.6.1.2 section now references **0.6.1.3** DMG
- Updated `v0.6.1.3` tag to point to `ad7abcf` (correct release commit)
- Pushed `main` + `v0.6.1.3` tag to `origin`

### 2. Version is now `0.6.1+3`

Both `Cargo.toml` and `zervo-core/Cargo.toml` now read `version = "0.6.1+3"`.

### 3. Git state

```
6675dc0 Update 0.6.1.2 section to reference 0.6.1.3 DMG  ← HEAD, main
ad7abcf Release Zervo 0.6.1.3  ← v0.6.1.3 (tagged)
b847870 Add French, Russian, and Ukrainian translations
0325218 Complete Fluent coverage for browser messages and updates
ebd48af Prepare Zervo 0.6.1.2 updater enrollment release
```

---

## What the codex session should know

- The updater infrastructure (Sparkle) is already on `main` — it was part
  of 0.6.1.2 (`ebd48af`). The only problem was that 0.6.1.3 was **never
  reachable** from `main` because it was a separate branch.
- After the merge, users on 0.6.1.2 will now see 0.6.1.3 through the
  updater (Sparkle appcast).
- The 0.6.1.3 branch also added French, Russian, and Ukrainian locale
  files (`zervo-core/locales/fr.ftl`, `zervo-core/locales/ru.ftl`,
  `zervo-core/locales/uk.ftl`).

## Stale test branch (can be discarded)

`bionic/issue-14-flicker-fix` was used to test render-flicker fixes and
was already deleted/merged back. No longer needed.

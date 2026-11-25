# Translate Zervo

Zervo uses [Fluent](https://projectfluent.org/fluent/guide/) for its interface.
Translations are ordinary UTF-8 text files in
[`zervo-core/locales/`](../zervo-core/locales/). English is the reference
catalogue. Japanese is a starter translation open to corrections and additions.
Every language file is discovered automatically at build time, and an
incomplete translation safely falls back to English.

1. To add a language, create a file such as `fr.ftl` or `pt-BR.ftl`. Use a
   [BCP 47 language tag](https://www.w3.org/International/articles/language-tags/)
   as the filename. Set `language-name` to the language’s name as its speakers
   write it, such as `Français`. Builds discover new catalogues automatically.
2. Copy the entries you want to translate from `en-US.ftl`. Change only the
   text after the equals sign. Keep each message ID and every variable such as
   `{ $title }`. A message may move its variables or use the language’s own
   plural forms. Leave unfinished entries out. Zervo displays those in English.
3. Try the language in first-run setup or **Settings → General → Language**.
   Check that longer labels still fit at a narrow window size.

A minimal `fr.ftl` looks like this.

```ftl
language-name = Français
common-settings = Réglages
sidebar-new-space-name = Espace { $number }
```

## Where to start

Work down the section headings in [the English catalogue](../zervo-core/locales/en-US.ftl).
Start with shared controls, setup, Settings, and the command bar. Continue
with New Tab, History, Downloads, site prompts, error pages, and software
updates. Settings and New Tab share the same city and calendar labels. A
language can give each site permission prompt its own wording and word order.

Zervo supplies the copy on its Updates page and in its own update reminders.
Sparkle supplies the installer dialogs. macOS supplies system permission
dialog controls. Website text, saved names, external diagnostics, and release
notes use the text supplied by their authors.

## Preview without rebuilding

A recent build can preview a catalogue without compiling the browser. Put
your `.ftl` file in a separate folder and use an absolute path to that folder.

```sh
# macOS, after dragging Zervo.app to Applications
ZERVO_LOCALES_DIR=/absolute/path/to/catalogues /Applications/Zervo.app/Contents/MacOS/Zervo
```

Quit Zervo before launching the preview. Restart it after editing the file.
If you already contribute code, `cargo test -p zervo-core --locked l10n`
checks catalogue syntax, duplicate IDs, placeholders, formatting, and English
fallback.

## Use a translation to test an incremental update

1. Preview your catalogue from disk. Pick a recognizable label and check it
   in Settings. Quit and relaunch normally to clear the preview override.
2. Merge the translation and include it in a new staging or release build.
   The build embeds all the catalogues in the executable. A locale-only
   change therefore works as a binary delta to the app bundle.
3. For a public test, use an unmodified copy of the previous updater-enabled
   release for your architecture. Follow the [release pipeline](UPDATES.md)
   to generate a signed appcast and a delta for that build. Review the draft
   and publish it when ready. Public clients retrieve the published feed.
4. From the previous app, open **Settings → About → Check for Updates**.
   Check the target version and incremental download size on the Updates
   page. Install the update, then verify the new label in the same app
   without `ZERVO_LOCALES_DIR`. Confirm that your existing profile is intact.

For a private end-to-end test, use separate test keys, an HTTPS test feed, and
a disposable copy of the previous app built to trust that feed. Use the
official v0.6.1.2 installer as the baseline when testing against the official
feed.

## Open a translation pull request

1. Create a branch and add only the new or corrected `.ftl` file under
   `zervo-core/locales/`.
2. In the pull request, name the language and regional variant, for example
   `Portuguese (Brazil) — pt-BR`, and say whether you previewed it in Zervo.
3. Keep message IDs, variable names, URLs, and shortcut keys unchanged.
   Translate the text in each plural form and keep the Fluent syntax intact.
4. Run `cargo test -p zervo-core --locked l10n` when the Rust toolchain is
   available. Maintainers will run the same check in CI.

Small pull requests are welcome. You can add a single missing sentence,
correct an existing translation, or submit a complete catalogue. Native
speakers reviewing wording are especially helpful.

Zervo includes [Noto Sans JP](https://github.com/google/fonts/tree/66a36c8c94b1a5d992ee4e7f392fccfe4945767c/ofl/notosansjp)
under the [SIL Open Font License](../assets/licenses/NotoSansJP-OFL.txt).
A translation for another script may need an additional font or right-to-left
layout work.

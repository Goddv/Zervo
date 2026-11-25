//! Fluent catalogues for the browser's own interface.

use std::{
    collections::HashMap,
    path::Path,
    sync::{LazyLock, RwLock},
};

use fluent_bundle::FluentArgs;
use fluent_bundle::{FluentResource, FluentValue, concurrent::FluentBundle};
use fluent_syntax::ast;
use unic_langid::LanguageIdentifier;

const ENGLISH: &str = "en-US";
const SOURCES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/locales.rs"));
const NAME_KEY: &str = "language-name";

struct Catalogue {
    bundle: FluentBundle<FluentResource>,
    labels: HashMap<String, String>,
}

impl Catalogue {
    fn new(locale: &str, source: String) -> Result<Self, String> {
        let locale = locale
            .parse::<LanguageIdentifier>()
            .map_err(|err| err.to_string())?;
        let resource =
            FluentResource::try_new(source).map_err(|(_, errors)| format!("{errors:?}"))?;
        let ids: Vec<_> = resource
            .entries()
            .filter_map(|entry| match entry {
                ast::Entry::Message(message) => Some(message.id.name.to_owned()),
                _ => None,
            })
            .collect();
        let mut bundle = FluentBundle::new_concurrent(vec![locale]);
        bundle
            .add_resource(resource)
            .map_err(|errors| format!("{errors:?}"))?;
        let mut catalogue = Self {
            bundle,
            labels: HashMap::new(),
        };
        for id in ids {
            if let Some(label) = catalogue.format(&id, None) {
                catalogue.labels.insert(id, label);
            }
        }
        if !catalogue.labels.contains_key(NAME_KEY) {
            return Err(format!("missing {NAME_KEY}"));
        }
        Ok(catalogue)
    }

    fn format(&self, key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
        let pattern = self.bundle.get_message(key)?.value()?;
        let mut errors = Vec::new();
        let value = self.bundle.format_pattern(pattern, args, &mut errors);
        let value = value.trim();
        (errors.is_empty() && !value.is_empty()).then(|| value.to_owned())
    }
}

struct Catalogues {
    bundled: HashMap<String, Catalogue>,
    preview: HashMap<String, Catalogue>,
}

impl Catalogues {
    fn get(&self, locale: &str) -> impl Iterator<Item = &Catalogue> {
        [
            self.preview.get(locale),
            self.bundled.get(locale),
            self.bundled.get(ENGLISH),
        ]
        .into_iter()
        .flatten()
    }

    fn resolve(&self, requested: &str) -> Option<&str> {
        let locale = requested
            .replace('_', "-")
            .parse::<LanguageIdentifier>()
            .ok()?;
        let exact = locale.to_string();
        if let Some((key, _)) = self
            .preview
            .get_key_value(&exact)
            .or_else(|| self.bundled.get_key_value(&exact))
        {
            return Some(key);
        }
        let language = locale.language.as_str();
        // Favor a language-only catalogue, then the English default for en-GB
        // and other English variants. Other variants fall back to English.
        if let Some((key, _)) = self
            .preview
            .get_key_value(language)
            .or_else(|| self.bundled.get_key_value(language))
        {
            return Some(key);
        }
        (language == "en").then_some(ENGLISH)
    }

    fn read_preview(&mut self, directory: &Path) -> std::io::Result<()> {
        for path in std::fs::read_dir(directory)? {
            let path = path?.path();
            if path.extension().is_none_or(|ext| ext != "ftl") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            match std::fs::read_to_string(&path)
                .map_err(|err| err.to_string())
                .and_then(|source| Catalogue::new(name, source))
            {
                Ok(catalogue) => {
                    let locale = catalogue.bundle.locales[0].to_string();
                    self.preview.insert(locale, catalogue);
                },
                Err(err) => log::warn!("ignoring translation {}: {err}", path.display()),
            }
        }
        Ok(())
    }
}

static CATALOGUES: LazyLock<Catalogues> = LazyLock::new(|| {
    let bundled = SOURCES
        .iter()
        .map(|&(locale, source)| {
            let catalogue = Catalogue::new(locale, source.to_owned())
                .unwrap_or_else(|error| panic!("invalid bundled translation {locale}: {error}"));
            (catalogue.bundle.locales[0].to_string(), catalogue)
        })
        .collect();
    let mut catalogues = Catalogues {
        bundled,
        preview: HashMap::new(),
    };
    if let Some(directory) = std::env::var_os("ZERVO_LOCALES_DIR")
        && let Err(err) = catalogues.read_preview(Path::new(&directory))
    {
        log::warn!("could not read Zervo translation preview directory: {err}");
    }
    catalogues
});

static CURRENT: RwLock<&'static str> = RwLock::new(ENGLISH);

/// Empty preference follows the system's language order.
pub fn select(preference: &str) {
    let locale = if preference.is_empty() {
        sys_locale::get_locales().find_map(|locale| CATALOGUES.resolve(&locale))
    } else {
        CATALOGUES.resolve(preference)
    };
    *CURRENT.write().expect("language selection lock") = locale.unwrap_or(ENGLISH);
}

fn current() -> &'static str {
    *CURRENT.read().expect("language selection lock")
}

/// Locale codes and their names as written by each translation team.
pub fn languages() -> Vec<(&'static str, &'static str)> {
    let mut languages: Vec<_> = CATALOGUES
        .bundled
        .keys()
        .chain(CATALOGUES.preview.keys())
        .map(String::as_str)
        .collect();
    languages.sort_unstable();
    languages.dedup();
    languages
        .into_iter()
        .map(|locale| {
            let catalogue = CATALOGUES.get(locale).next().expect("listed translation");
            (locale, catalogue.labels[NAME_KEY].as_str())
        })
        .collect()
}

/// Preformatted labels keep the existing drawing paths allocation-free.
pub fn text(key: &'static str) -> &'static str {
    CATALOGUES
        .get(current())
        .find_map(|catalogue| catalogue.labels.get(key).map(String::as_str))
        .unwrap_or(key)
}

pub fn message<'a>(
    key: &'static str,
    values: impl IntoIterator<Item = (&'static str, FluentValue<'a>)>,
) -> String {
    let mut args = FluentArgs::new();
    for (name, value) in values {
        args.set(name, value);
    }
    CATALOGUES
        .get(current())
        .find_map(|catalogue| catalogue.format(key, Some(&args)))
        .unwrap_or_else(|| key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    fn variables(pattern: &ast::Pattern<&str>) -> BTreeSet<String> {
        let mut result = BTreeSet::new();
        for element in &pattern.elements {
            if let ast::PatternElement::Placeable { expression } = element {
                collect_variables(expression, &mut result);
            }
        }
        result
    }

    fn collect_variables(expression: &ast::Expression<&str>, names: &mut BTreeSet<String>) {
        match expression {
            ast::Expression::Inline(inline) => collect_inline(inline, names),
            ast::Expression::Select { selector, variants } => {
                collect_inline(selector, names);
                for variant in variants {
                    names.extend(variables(&variant.value));
                }
            },
        }
    }

    fn collect_inline(inline: &ast::InlineExpression<&str>, names: &mut BTreeSet<String>) {
        match inline {
            ast::InlineExpression::VariableReference { id } => {
                names.insert(id.name.to_owned());
            },
            ast::InlineExpression::FunctionReference { arguments, .. }
            | ast::InlineExpression::TermReference {
                arguments: Some(arguments),
                ..
            } => {
                for argument in &arguments.positional {
                    collect_inline(argument, names);
                }
                for argument in &arguments.named {
                    collect_inline(&argument.value, names);
                }
            },
            ast::InlineExpression::Placeable { expression } => collect_variables(expression, names),
            _ => {},
        }
    }

    fn schema(source: &str) -> HashMap<String, BTreeSet<String>> {
        FluentResource::try_new(source.to_owned())
            .expect("Fluent syntax")
            .entries()
            .filter_map(|entry| {
                if let ast::Entry::Message(message) = entry {
                    let value = message.value.as_ref().expect("value for UI message");
                    Some((message.id.name.to_owned(), variables(value)))
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn catalogues_keep_english_ids_and_variables() {
        let source = SOURCES
            .iter()
            .find(|(locale, _)| *locale == ENGLISH)
            .unwrap()
            .1;
        let english = schema(source);
        for &(locale, source) in SOURCES {
            let catalogue = Catalogue::new(locale, source.to_owned())
                .unwrap_or_else(|err| panic!("{locale}: {err}"));
            for (key, variables) in schema(source) {
                assert_eq!(english.get(&key), Some(&variables), "{locale}: {key}");
                if variables.is_empty() {
                    assert!(
                        catalogue.labels.contains_key(&key),
                        "{locale}: {key} fails to format as a label"
                    );
                } else {
                    // Exercise zero, singular and plural forms in addition
                    // to checking that translators kept each variable.
                    for count in [0, 1, 2] {
                        let mut args = FluentArgs::new();
                        for name in &variables {
                            if name == "count" {
                                args.set(name.as_str(), count);
                            } else {
                                args.set(name.as_str(), "Example");
                            }
                        }
                        assert!(
                            catalogue.format(&key, Some(&args)).is_some(),
                            "{locale}: {key} fails to format with count={count}"
                        );
                    }
                }
            }
        }
    }

    fn fixture() -> Catalogues {
        let english = Catalogue::new(
            ENGLISH,
            "language-name = English
settings = Settings
downloads = Downloads
count = { $count ->
    [one] 1 notification
   *[other] { $count } notifications
}
"
            .to_owned(),
        )
        .unwrap();
        let japanese = Catalogue::new(
            "ja",
            "language-name = 日本語
settings = 設定
count = { $count } 件の通知
"
            .to_owned(),
        )
        .unwrap();
        Catalogues {
            bundled: HashMap::from([(ENGLISH.to_owned(), english), ("ja".to_owned(), japanese)]),
            preview: HashMap::new(),
        }
    }

    #[test]
    fn negotiates_system_variants_and_falls_back_to_english() {
        let catalogues = fixture();
        assert_eq!(catalogues.resolve("ja-JP"), Some("ja"));
        assert_eq!(catalogues.resolve("fr_FR"), None);
        assert_eq!(catalogues.resolve("en-GB"), Some(ENGLISH));
        let translated = catalogues
            .get("ja")
            .find_map(|catalogue| catalogue.labels.get("settings"));
        assert_eq!(translated.map(String::as_str), Some("設定"));
        let missing = catalogues
            .get("ja")
            .find_map(|catalogue| catalogue.labels.get("downloads"));
        assert_eq!(missing.map(String::as_str), Some("Downloads"));
    }

    #[test]
    fn formats_plural_messages_with_provided_values() {
        let catalogues = fixture();
        let en = &catalogues.bundled[ENGLISH];
        let ja = &catalogues.bundled["ja"];
        let mut args = FluentArgs::new();
        args.set("count", 1);
        assert_eq!(
            en.format("count", Some(&args)).as_deref(),
            Some("1 notification")
        );
        args.set("count", 4);
        let english = en.format("count", Some(&args)).unwrap();
        let japanese = ja.format("count", Some(&args)).unwrap();
        // Fluent wraps interpolated numbers with bidi isolation marks.
        assert_eq!(
            english.replace(['\u{2068}', '\u{2069}'], ""),
            "4 notifications"
        );
        assert_eq!(japanese.replace(['\u{2068}', '\u{2069}'], ""), "4 件の通知");
    }

    #[test]
    fn preview_overrides_only_available_messages() {
        let mut catalogues = fixture();
        catalogues.preview.insert(
            "ja".to_owned(),
            Catalogue::new(
                "ja",
                "language-name = 日本語
settings = 環境設定
"
                .to_owned(),
            )
            .unwrap(),
        );
        let get = |key| {
            catalogues
                .get("ja")
                .find_map(|catalogue| catalogue.labels.get(key))
        };
        assert_eq!(get("settings").map(String::as_str), Some("環境設定"));
        assert_eq!(get("downloads").map(String::as_str), Some("Downloads"));
    }
}

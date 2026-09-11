//! UI language: German or English, with the texts in Fluent files
//! (`locales/*.ftl`, compiled into the binary).
//!
//! One process-wide setting, chosen once at startup (`language` in
//! config.toml, else the locale) and switchable from the sidebar. Being
//! global, it reaches the SSH worker threads and the parsers without
//! threading a parameter through everything.
//!
//! Code asks for a message by id, with named arguments:
//! `t!("common-save")`, `t!("app-shell-start-failed", shell = name, err = err.to_string())`.
//! Grammar -- plurals, "(3 set)" -- is up to each language's file.
//! Arguments are strings or numbers (numbers select plural variants).
//! Terminal escape codes, and spaces or line breaks around a whole
//! message, stay in the code.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    German,
    English,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::German, Language::English];

    /// The value in config.toml, and the file name in `locales/`.
    pub fn code(self) -> &'static str {
        match self {
            Self::German => "de",
            Self::English => "en",
        }
    }

    /// `de`, `de_AT.UTF-8`, `German`, `english`, ... -- `None` for anything
    /// else (including `auto`).
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_lowercase();
        if value.starts_with("de") || value == "german" {
            Some(Self::German)
        } else if value.starts_with("en") {
            Some(Self::English)
        } else {
            None
        }
    }

    /// Always in its own language, so it can be found in either.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::German => "Deutsch",
            Self::English => "English",
        }
    }

    /// German for a German locale, English for everything else.
    pub fn from_locale() -> Self {
        from_locale_vars(|name| std::env::var(name).ok())
    }

    fn index(self) -> usize {
        match self {
            Self::German => 0,
            Self::English => 1,
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::German => include_str!("../locales/de.ftl"),
            Self::English => include_str!("../locales/en.ftl"),
        }
    }
}

/// The locale that decides the language of messages, as glibc sees it:
/// the first of `LC_ALL`, `LC_MESSAGES`, `LANG` that is set -- unless
/// GNU's `LANGUAGE` priority list names one first (ignored for the `C`
/// locale, as by gettext).
fn from_locale_vars(get: impl Fn(&str) -> Option<String>) -> Language {
    let set = |name: &str| get(name).filter(|value| !value.is_empty());
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(set);
    let preferred = locale
        .as_deref()
        .filter(|locale| !matches!(*locale, "C" | "POSIX") && !locale.starts_with("C."))
        .and(set("LANGUAGE"))
        .and_then(|list| list.split(':').find(|entry| !entry.is_empty()).map(str::to_string));
    match preferred.or(locale) {
        Some(value) if value.to_lowercase().starts_with("de") => Language::German,
        _ => Language::English,
    }
}

const UNSET: u8 = 0;
const GERMAN: u8 = 1;
const ENGLISH: u8 = 2;

static CURRENT: AtomicU8 = AtomicU8::new(UNSET);

pub fn set(language: Language) {
    let value = match language {
        Language::German => GERMAN,
        Language::English => ENGLISH,
    };
    CURRENT.store(value, Ordering::Relaxed);
}

pub fn current() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        GERMAN => Language::German,
        ENGLISH => Language::English,
        // Tests never call `set` (they run in parallel) and check the
        // German texts.
        _ if cfg!(test) => Language::German,
        _ => {
            let language = Language::from_locale();
            set(language);
            language
        }
    }
}

/// One bundle per language, in [`Language::ALL`] order; built on first use.
static BUNDLES: LazyLock<[FluentBundle<FluentResource>; 2]> = LazyLock::new(|| Language::ALL.map(bundle));

fn bundle(language: Language) -> FluentBundle<FluentResource> {
    let file = format!("locales/{}.ftl", language.code());
    let resource = FluentResource::try_new(language.source().to_string()).unwrap_or_else(|(resource, errors)| {
        log::error!("{file}: {errors:?}");
        resource
    });
    let mut bundle = FluentBundle::new_concurrent(vec![language.code().parse().expect("valid language tag")]);
    // No Unicode isolation marks around arguments: egui would draw them,
    // and they'd end up in the terminal.
    bundle.set_use_isolating(false);
    if let Err(errors) = bundle.add_resource(resource) {
        log::error!("{file}: {errors:?}");
    }
    bundle
}

/// Message `id` in the current language -- use [`t!`]. A missing message
/// shows as its id, rather than failing.
pub fn text(id: &str, args: Option<&FluentArgs>) -> String {
    let language = current();
    let bundle = &BUNDLES[language.index()];
    let Some(pattern) = bundle.get_message(id).and_then(|message| message.value()) else {
        log::warn!("no message {id} in locales/{}.ftl", language.code());
        return id.to_string();
    };
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args, &mut errors);
    if !errors.is_empty() {
        log::warn!("message {id}: {errors:?}");
    }
    text.into_owned()
}

/// `t!("id")` or `t!("id", name = value, ...)`: the message in the current
/// language, as a `String`. Values: `&str`, `String`, `&String` or numbers.
macro_rules! t {
    ($id:literal $(,)?) => {
        $crate::i18n::text($id, None)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = fluent_bundle::FluentArgs::new();
        $(args.set(stringify!($name), $value);)+
        $crate::i18n::text($id, Some(&args))
    }};
}

pub(crate) use t;

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

    use super::*;

    fn detect(vars: &[(&str, &str)]) -> Language {
        from_locale_vars(|name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string()))
    }

    #[test]
    fn language_follows_the_message_locale() {
        assert_eq!(detect(&[]), Language::English);
        assert_eq!(detect(&[("LANG", "de_DE.UTF-8")]), Language::German);
        assert_eq!(detect(&[("LANG", "de_AT.UTF-8")]), Language::German);
        assert_eq!(detect(&[("LANG", "en_US.UTF-8")]), Language::English);
        assert_eq!(detect(&[("LANG", "fr_FR.UTF-8")]), Language::English);
        // LC_ALL over LC_MESSAGES over LANG; empty counts as unset.
        assert_eq!(detect(&[("LANG", "de_DE.UTF-8"), ("LC_MESSAGES", "en_GB.UTF-8")]), Language::English);
        assert_eq!(detect(&[("LANG", "en_US.UTF-8"), ("LC_ALL", "de_CH.UTF-8")]), Language::German);
        assert_eq!(detect(&[("LANG", "de_DE.UTF-8"), ("LC_ALL", "")]), Language::German);
        // LANGUAGE's first entry wins -- except for the C locale.
        assert_eq!(detect(&[("LANG", "en_US.UTF-8"), ("LANGUAGE", "de:en")]), Language::German);
        assert_eq!(detect(&[("LANG", "de_DE.UTF-8"), ("LANGUAGE", "en_US")]), Language::English);
        assert_eq!(detect(&[("LANG", "C"), ("LANGUAGE", "de")]), Language::English);
    }

    #[test]
    fn parses_config_values() {
        assert_eq!(Language::parse("de"), Some(Language::German));
        assert_eq!(Language::parse(" DE_de.utf-8 "), Some(Language::German));
        assert_eq!(Language::parse("Deutsch"), Some(Language::German));
        assert_eq!(Language::parse("english"), Some(Language::English));
        assert_eq!(Language::parse("auto"), None);
        assert_eq!(Language::parse("fr"), None);
        for language in Language::ALL {
            assert_eq!(Language::parse(language.code()), Some(language));
        }
    }

    /// Message id → the variables it uses, from a language's file.
    fn messages(language: Language) -> BTreeMap<String, BTreeSet<String>> {
        fn walk_pattern(pattern: &Pattern<&str>, vars: &mut BTreeSet<String>) {
            for element in &pattern.elements {
                if let PatternElement::Placeable { expression } = element {
                    walk_expression(expression, vars);
                }
            }
        }
        fn walk_inline(inline: &InlineExpression<&str>, vars: &mut BTreeSet<String>) {
            match inline {
                InlineExpression::VariableReference { id } => drop(vars.insert(id.name.to_string())),
                InlineExpression::Placeable { expression } => walk_expression(expression, vars),
                _ => {}
            }
        }
        fn walk_expression(expression: &Expression<&str>, vars: &mut BTreeSet<String>) {
            match expression {
                Expression::Inline(inline) => walk_inline(inline, vars),
                Expression::Select { selector, variants } => {
                    walk_inline(selector, vars);
                    for variant in variants {
                        walk_pattern(&variant.value, vars);
                    }
                }
            }
        }

        let resource = fluent_syntax::parser::parse(language.source())
            .unwrap_or_else(|(_, errors)| panic!("locales/{}.ftl: {errors:?}", language.code()));
        let mut messages = BTreeMap::new();
        for entry in &resource.body {
            if let Entry::Message(message) = entry {
                let mut vars = BTreeSet::new();
                let value = message.value.as_ref().unwrap_or_else(|| panic!("{} has no value", message.id.name));
                walk_pattern(value, &mut vars);
                assert!(
                    messages.insert(message.id.name.to_string(), vars).is_none(),
                    "{} is defined twice in locales/{}.ftl",
                    message.id.name,
                    language.code()
                );
            }
        }
        messages
    }

    #[test]
    fn languages_have_the_same_messages_and_arguments() {
        let german = messages(Language::German);
        let english = messages(Language::English);
        let ids = |messages: &BTreeMap<String, _>| messages.keys().cloned().collect::<BTreeSet<_>>();
        let (de, en) = (ids(&german), ids(&english));
        assert!(de == en, "only in de.ftl: {:?}; only in en.ftl: {:?}", &de - &en, &en - &de);
        for (id, vars) in &german {
            assert_eq!(vars, &english[id], "arguments of {id} differ between de.ftl and en.ftl");
        }
        for language in Language::ALL {
            // Loads without errors, too.
            assert!(BUNDLES[language.index()].has_message("common-save"));
        }
    }

    /// Every `t!("…")` in the sources names a message that exists.
    #[test]
    fn every_used_message_exists() {
        fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        let mut files = Vec::new();
        sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let known = messages(Language::English);
        let mut used = 0;
        // Comments are skipped: docs show made-up ids.
        for (file, line) in files
            .iter()
            .flat_map(|(file, text)| text.lines().map(move |line| (file, line)))
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
        {
            for (at, _) in line.match_indices("t!(") {
                // Not `format!(`, `print!(`, ...
                if line[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let Some(rest) = line[at + 3..].trim_start().strip_prefix('"') else { continue };
                // Only a complete `"id"` -- not this test's own `"t!("`.
                let Some(id) = rest.split_once('"').map(|(id, _)| id) else { continue };
                if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    continue;
                }
                assert!(known.contains_key(id), "{file}: no message {id:?}");
                used += 1;
            }
        }
        assert!(used > 100, "only found {used} uses of t!");
    }

    #[test]
    fn formats_arguments_and_plurals() {
        let mut args = FluentArgs::new();
        args.set("count", 1);
        assert_eq!(text("ssh-more-logins", Some(&args)), "+1 Login");
        args.set("count", 3);
        assert_eq!(text("ssh-more-logins", Some(&args)), "+3 Logins");
        args.set("count", 0);
        assert_eq!(text("host-advanced", Some(&args)), "Erweitert");
        args.set("count", 2);
        assert_eq!(text("host-advanced", Some(&args)), "Erweitert (2 gesetzt)");
        // No isolation marks around arguments.
        assert_eq!(t!("common-deleted", name = "ll"), "„ll“ gelöscht.");
        assert_eq!(text("no-such-message", None), "no-such-message");
    }
}

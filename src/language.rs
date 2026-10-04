//! Your language and formats, over the machine's: `Users\<SID>\Locale`,
//! which the kernel routes `CurrentUser\Locale` to, and which is yours to
//! write. Each session reads it as it starts.

use libgxwi::Fields;
use libgxwi::settings::{self, Glyph, Tile};
use libsession::locale::{self, Available};
use peios::registry::Data;

use crate::reg;

pub const OWN: &str = "CurrentUser\\Locale";
pub const FORMATS: [&str; 5] = ["LC_TIME", "LC_NUMERIC", "LC_MONETARY", "LC_MEASUREMENT", "LC_PAPER"];

#[derive(Debug, Clone, PartialEq)]
pub struct Language {
    pub lang: Option<String>,
    /// The formats' locale where every format category names the same one;
    /// `Some("")` where they differ.
    pub formats: Option<String>,
    pub machine_lang: String,
    pub may: Result<(), String>,
}

pub fn read() -> Language {
    let formats: Vec<Option<String>> = FORMATS.iter().map(|c| reg::text(OWN, c)).collect();
    let formats = match formats.first().cloned().flatten() {
        None if formats.iter().all(Option::is_none) => None,
        Some(first) if formats.iter().all(|f| f.as_deref() == Some(first.as_str())) => Some(first),
        _ => Some(String::new()),
    };
    Language {
        lang: reg::text(OWN, "LANG"),
        formats,
        machine_lang: reg::text(locale::MACHINE_KEY, "LANG").unwrap_or_else(|| locale::DEFAULT.into()),
        may: reg::may_change(OWN, "your language"),
    }
}

pub fn fill(language: &Language, fields: &mut Fields) {
    fields.set("lang", language.lang.as_deref().unwrap_or(""));
    fields.set("formats", language.formats.as_deref().unwrap_or(""));
}

fn describe(available: &Available) -> String {
    match (&available.language, &available.territory) {
        (Some(language), Some(territory)) => format!("{language} ({territory}) — {}", available.name),
        (Some(language), None) => format!("{language} — {}", available.name),
        _ if available.name == locale::DEFAULT => format!("Plain English, international conventions — {}", available.name),
        _ => available.name.clone(),
    }
}

/// What the side says of this section.
pub fn now(language: &Language, installed: &[Available]) -> String {
    match &language.lang {
        None => "System Default".into(),
        Some(lang) => match installed.iter().find(|a| &a.name == lang) {
            Some(Available { language: Some(name), territory: Some(place), .. }) => format!("{name} ({place})"),
            Some(Available { language: Some(name), .. }) => name.clone(),
            _ => lang.clone(),
        },
    }
}

pub fn render(language: &Language, installed: &[Available]) -> String {
    let may = language.may.is_ok();
    let options = |first: String| {
        let mut options = vec![(String::new(), first)];
        options.extend(installed.iter().map(|a| (a.name.clone(), describe(a))));
        options
    };
    let rows = format!(
        "{}{}",
        settings::row(
            "Language",
            "The language programs use, where they support it.",
            &settings::select("lang", "Language", &options("System Default".into()), may),
        ),
        settings::row(
            "Formats",
            "How dates, numbers, currency, measurements and paper sizes are written.",
            &settings::select("formats", "Formats", &options("Match Language".into()), may),
        ),
    );
    let foot = match &language.may {
        Err(why) => settings::locked(why),
        Ok(()) => settings::hint("Applies at your next sign-in. An administrator can add languages by installing language packs."),
    };
    format!(
        "{}{}",
        settings::head(Glyph::Globe, Tile::Violet, "Language & Formats", "Your language, over the System Default."),
        settings::group("Language & Formats", &rows, &foot)
    )
}

pub fn save(fields: &Fields) -> Result<String, String> {
    let lang = fields.get("lang").trim();
    if !lang.is_empty() && !locale::usable("LANG", lang) {
        return Err(format!("{lang} isn't installed in full, so your session would ignore it."));
    }
    let formats = fields.get("formats").trim();
    if !formats.is_empty() && !FORMATS.iter().all(|c| locale::usable(c, formats)) {
        return Err(format!("{formats} isn't installed, so your session would ignore it."));
    }
    let mut values = vec![("LANG", (!lang.is_empty()).then(|| Data::Sz(lang.to_string())))];
    for category in FORMATS {
        values.push((category, (!formats.is_empty()).then(|| Data::Sz(formats.to_string()))));
    }
    reg::set(OWN, "your language", &values)?;
    Ok("Saved. Applies at your next sign-in.".into())
}

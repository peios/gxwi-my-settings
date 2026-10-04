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
        None => "As this machine has it".into(),
        Some(lang) => match installed.iter().find(|a| &a.name == lang) {
            Some(Available { language: Some(name), territory: Some(place), .. }) => format!("{name} ({place})"),
            Some(Available { language: Some(name), .. }) => name.clone(),
            _ => lang.clone(),
        },
    }
}

pub fn render(language: &Language, installed: &[Available]) -> String {
    let may = language.may.is_ok();
    let machine = installed.iter().find(|a| a.name == language.machine_lang).map_or_else(|| language.machine_lang.clone(), describe);
    let options = |first: String| {
        let mut options = vec![(String::new(), first)];
        options.extend(installed.iter().map(|a| (a.name.clone(), describe(a))));
        options
    };
    let rows = format!(
        "{}{}",
        settings::row(
            "Language",
            "What programs speak to you in, where they can.",
            &settings::select("lang", "Language", &options(format!("As this machine has it: {machine}")), may),
        ),
        settings::row(
            "Formats",
            "How dates, numbers, money, measures and paper sizes are written.",
            &settings::select("formats", "Formats", &options("As the language writes them".into()), may),
        ),
    );
    let foot = match &language.may {
        Err(why) => settings::locked(why),
        Ok(()) => settings::hint("Each applies the next time you sign in. A language that isn't listed is added by an administrator, by installing its language pack."),
    };
    format!(
        "{}{}",
        settings::head(Glyph::Globe, Tile::Violet, "Language & formats", "Yours, over this machine's."),
        settings::group("Language and formats", &rows, &foot)
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
    Ok("Saved. It applies the next time you sign in.".into())
}

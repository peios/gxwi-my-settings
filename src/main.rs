//! My Settings: the person's own settings that belong to the system rather
//! than to a desktop — their name, password, SSH keys and language — under
//! any shell.
//!
//! Everything here acts on whoever is looking, and on nobody else: the
//! sockets it talks to take the person from their token and are told no
//! name. The desktop's own settings are Desktop Settings'; the machine's,
//! System Settings'.

use std::sync::{Arc, Weak};
use std::time::Duration;

use libgxwi::settings::{self, Glyph, Nav, Section, Tile};
use libgxwi::{App, Facts, Fields, Live, Surface, Value};
use libsession::locale::{self, Available};

mod account;
mod language;
mod reg;

use account::{Account, Removing};
use language::Language;

libgxwi::icon!(b"dev.peios.gxwi-my-settings");

/// How often the account is read again: nothing says when it changes.
const LOOK_AGAIN: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Account,
    Password,
    Keys,
    Language,
}

impl View {
    const ALL: [(View, &'static str); 4] = [(View::Account, "account"), (View::Password, "password"), (View::Keys, "keys"), (View::Language, "language")];

    fn by(name: &str) -> Option<View> {
        View::ALL.iter().find(|(_, by)| *by == name).map(|(view, _)| *view)
    }

    fn id(self) -> &'static str {
        View::ALL.iter().find(|(view, _)| *view == self).map(|(_, by)| *by).unwrap_or("account")
    }
}

struct Settings {
    window: Weak<Surface<Settings>>,
    view: View,
    account: Account,
    language: Language,
    installed: Vec<Available>,
    removing: Option<Removing>,
    /// The form to add a key is open.
    adding: bool,
    said: Option<Result<String, String>>,
}

impl Settings {
    fn fill(&self, fields: &mut Fields) {
        account::fill(&self.account, fields);
        language::fill(&self.language, fields);
    }

    fn reread(&mut self, fields: &mut Fields) {
        self.account = account::read();
        self.language = language::read();
        self.fill(fields);
    }

    fn nav(&self) -> Vec<Nav> {
        let section = |view: View, title, now: String, glyph, tile| Nav::Section(Section { id: view.id(), title, now, glyph, tile });
        vec![
            section(View::Account, "Your account", account::now_account(&self.account), Glyph::User, Tile::Orange),
            section(View::Password, "Password", account::now_password(&self.account), Glyph::Lock, Tile::Green),
            section(View::Keys, "SSH keys", account::now_keys(&self.account), Glyph::Key, Tile::Teal),
            section(View::Language, "Language & formats", language::now(&self.language, &self.installed), Glyph::Globe, Tile::Violet),
        ]
    }
}

impl Live for Settings {
    fn render(&self, facts: &Facts) -> String {
        let fields = facts.fields;
        let page = match self.view {
            View::Account => account::render_account(&self.account, fields),
            View::Password => account::render_password(&self.account, fields),
            View::Keys => account::render_keys(&self.account, fields, self.removing.as_ref(), self.adding),
            View::Language => language::render(&self.language, &self.installed),
        };
        let aside = match self.view {
            View::Language => "Choices apply at once",
            _ => "",
        };
        settings::window(&self.nav(), self.view.id(), &page, &settings::status(self.said.as_ref(), aside))
    }

    fn input(&mut self, name: &str, fields: &mut Fields) {
        if name != "lang" && name != "formats" {
            return;
        }
        let done = language::save(fields);
        if done.is_err() {
            // What couldn't be done is shown as it still is.
            language::fill(&self.language, fields);
        } else {
            self.language = language::read();
        }
        self.said = Some(done);
    }

    fn event(&mut self, name: &str, value: &Value, fields: &mut Fields) {
        let done = match name {
            "section" => {
                if let Some(view) = value.get("section").and_then(Value::as_str).and_then(View::by) {
                    self.view = view;
                    self.said = None;
                    self.removing = None;
                    self.adding = false;
                    account::forget_secrets(fields);
                }
                return;
            }
            "open-add" => {
                self.adding = true;
                self.removing = None;
                self.said = None;
                return;
            }
            "cancel-add" => {
                self.adding = false;
                fields.set("key", "");
                account::forget_secrets(fields);
                return;
            }
            "remove-key" => {
                let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                self.removing = Some(Removing { fingerprint: text("fingerprint"), label: text("label") });
                self.adding = false;
                self.said = None;
                return;
            }
            "cancel-remove" => {
                self.removing = None;
                account::forget_secrets(fields);
                return;
            }
            "save-name" => account::save_name(fields),
            "change-password" => account::change_password(fields),
            "add-key" => account::add_key(fields),
            "confirm-remove" => match self.removing.take() {
                Some(removing) => {
                    let done = account::remove_key(&removing, fields);
                    // Refused, it is still asked, for the password again.
                    if done.is_err() {
                        self.removing = Some(removing);
                    }
                    done
                }
                None => return,
            },
            _ => return,
        };
        // What was typed into a password field is not kept a moment longer
        // than it was needed, whatever came of it.
        account::forget_secrets(fields);
        let succeeded = done.is_ok();
        self.said = Some(done);
        if succeeded {
            if name == "add-key" {
                fields.set("key", "");
                self.adding = false;
            }
            self.reread(fields);
        }
    }
}

fn look_again(window: Weak<Surface<Settings>>) {
    loop {
        std::thread::sleep(LOOK_AGAIN);
        let (account, language) = (account::read(), language::read());
        let Some(shown) = window.upgrade() else { return };
        if shown.look(|s, _, _| s.account != account || s.language != language) {
            shown.update(|s, fields| {
                // A field still holding what was set follows what is set now.
                let mut before = Fields::default();
                s.fill(&mut before);
                s.account = account;
                s.language = language;
                let mut after = Fields::default();
                s.fill(&mut after);
                for name in ["display-name", "lang", "formats"] {
                    if fields.get(name) == before.get(name) && before.get(name) != after.get(name) {
                        fields.set(name, after.get(name));
                    }
                }
            });
        }
    }
}

fn main() {
    if std::env::args().nth(1).is_some() {
        eprintln!("gxwi-my-settings: usage: gxwi-my-settings");
        std::process::exit(64);
    }
    let mut app = match App::connect() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("gxwi-my-settings: no desktop to open on: {e}");
            eprintln!("gxwi-my-settings: on a terminal, passwd changes your password and lps key your keys");
            std::process::exit(1);
        }
    };
    settings::stylesheet(&mut app);
    let settings = Settings {
        window: Weak::new(),
        view: View::Account,
        account: account::read(),
        language: language::read(),
        installed: locale::available(),
        removing: None,
        adding: false,
        said: None,
    };
    let window = app.live("My Settings", settings);
    let aside = Arc::downgrade(&window);
    window.update(|s, fields| {
        s.window = aside.clone();
        s.fill(fields);
    });
    std::thread::spawn(move || look_again(aside));
    if let Err(e) = app.run() {
        eprintln!("gxwi-my-settings: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_section_names_a_view() {
        for (view, by) in View::ALL {
            assert_eq!(View::by(by), Some(view));
            assert_eq!(view.id(), by);
        }
    }
}

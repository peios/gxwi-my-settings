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

use libgxwi::{App, Facts, Fields, Live, Surface, Value, escape};
use libsession::locale::{self, Available};

mod account;
mod language;
mod reg;

use account::{Account, Removing};
use language::Language;

libgxwi::icon!(b"dev.peios.gxwi-my-settings");

/// How often the account is read again: nothing says when it changes.
const LOOK_AGAIN: Duration = Duration::from_secs(5);

struct Settings {
    window: Weak<Surface<Settings>>,
    account: Account,
    language: Language,
    installed: Vec<Available>,
    removing: Option<Removing>,
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
}

impl Live for Settings {
    fn render(&self, facts: &Facts) -> String {
        let said = match &self.said {
            None => String::new(),
            Some(Ok(done)) => format!("<p class=\"said\" role=\"status\">{}</p>", escape(done)),
            Some(Err(why)) => format!("<p class=\"said bad\" role=\"alert\">{}</p>", escape(why)),
        };
        format!(
            "{said}<div class=\"body\"><div class=\"cards\">{}{}</div></div>",
            account::render(&self.account, facts.fields, self.removing.as_ref()),
            language::render(&self.language, &self.installed, facts.fields),
        )
    }

    fn event(&mut self, name: &str, value: &Value, fields: &mut Fields) {
        let done = match name {
            "save-name" => account::save_name(fields),
            "change-password" => account::change_password(fields),
            "add-key" => account::add_key(fields),
            "remove-key" => {
                let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                self.removing = Some(Removing { fingerprint: text("fingerprint"), label: text("label") });
                self.said = None;
                return;
            }
            "cancel-remove" => {
                self.removing = None;
                account::forget_secrets(fields);
                return;
            }
            "confirm-remove" => match self.removing.take() {
                Some(removing) => account::remove_key(&removing, fields),
                None => return,
            },
            "save-language" => language::save(fields),
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
    app.stylesheet("/gxwi-my-settings.css", include_str!("gxwi-my-settings.css"));
    let settings = Settings {
        window: Weak::new(),
        account: account::read(),
        language: language::read(),
        installed: locale::available(),
        removing: None,
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

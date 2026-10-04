//! Your account: your name as others see it, your password, and the SSH keys
//! you sign in with.
//!
//! The account is read, and the display name set, on lpsd's self socket,
//! which only ever acts on whoever is asking. The password and keys change
//! through authd's logon socket, in a conversation that asks for your current
//! password first: a desktop left signed in is not proof enough that the
//! person at it is you. An account with no password to give — one that signs
//! in with keys only — can't prove itself that way, so its keys are an
//! administrator's to change.

use libauthd::Secret;
use libauthd::credential::Policy;
use libauthd_client::credential::{Abandon, Collector, Credentials, Refusal, Round};
use libauthd_client::own::{Own, OwnAccount};
use libgxwi::settings::{self, Glyph, Kind, More, Tile, Tone, Width};
use libgxwi::{Fields, escape};

/// The account as lpsd has it, or why it can't be read.
pub type Account = Result<OwnAccount, String>;

pub fn read() -> Account {
    Own::new().show().map_err(|refusal| refusal.reason)
}

pub fn fill(account: &Account, fields: &mut Fields) {
    fields.set("display-name", account.as_ref().map_or("", |a| a.display_name.as_str()));
}

/// The fields that hold passwords, emptied as soon as they have been used:
/// what is typed is part of the window, on every screen it is on.
pub const SECRETS: [&str; 5] = ["current", "new", "confirm", "key-password", "remove-password"];

pub fn forget_secrets(fields: &mut Fields) {
    for name in SECRETS {
        fields.set(name, "");
    }
}

/// Whether this account can prove itself with a password, which every
/// change to a password or key asks for first.
pub fn can_prove(account: &OwnAccount) -> bool {
    account.has_password && matches!(account.policy, Policy::Password | Policy::PasswordOrKey)
}

fn policy_words(policy: Policy) -> &'static str {
    match policy {
        Policy::Password => "with a password",
        Policy::SshPublicKey => "with an SSH key only",
        Policy::PasswordOrKey => "with a password or an SSH key",
        Policy::NoCredential => "without a password",
        Policy::Denied => "nowhere: signing in is turned off",
    }
}

/// A key being taken away, while the window asks for the password.
#[derive(Debug, Clone, PartialEq)]
pub struct Removing {
    pub fingerprint: String,
    pub label: String,
}

/// What the side says of the account.
pub fn now_account(account: &Account) -> String {
    match account {
        Err(_) => "Can't be read".into(),
        Ok(a) if a.display_name.is_empty() => format!("{} · signs in {}", a.name, policy_short(a.policy)),
        Ok(a) => format!("{} · signs in {}", a.display_name, policy_short(a.policy)),
    }
}

/// What the side says of the password.
pub fn now_password(account: &Account) -> String {
    match account {
        Ok(a) if can_prove(a) => "Change it".into(),
        Ok(_) => "None to change".into(),
        Err(_) => String::new(),
    }
}

/// What the side says of the keys.
pub fn now_keys(account: &Account) -> String {
    match account.as_ref().map(|a| a.keys.len()) {
        Ok(0) => "None yet".into(),
        Ok(1) => "1 key".into(),
        Ok(n) => format!("{n} keys"),
        Err(_) => String::new(),
    }
}

fn policy_short(policy: Policy) -> &'static str {
    match policy {
        Policy::Password => "with a password",
        Policy::SshPublicKey => "with a key",
        Policy::PasswordOrKey => "with a password or key",
        Policy::NoCredential => "without a password",
        Policy::Denied => "nowhere",
    }
}

/// The account couldn't be read: what each section shows instead.
fn unreadable(why: &str) -> String {
    settings::banner(why)
}

pub fn render_account(account: &Account, fields: &Fields) -> String {
    let head = settings::head(Glyph::User, Tile::Orange, "Your account", "Your name, and how you sign in.");
    let account = match account {
        Err(why) => return format!("{head}{}", unreadable(why)),
        Ok(account) => account,
    };
    let (shown, under) = if account.display_name.is_empty() {
        (account.name.as_str(), "No name of your own yet: the desktop calls you by your account name.".to_string())
    } else {
        (account.display_name.as_str(), format!("Signed in as {}", account.name))
    };
    let hero = settings::hero(
        &settings::hero_title(Glyph::User, Tile::Orange, shown, &under),
        &settings::pill(&format!("Signs in {}", policy_short(account.policy)), Tone::Plain),
    );
    let mut control = settings::text("display-name", "Your name as others see it", "text", Width::Normal, true, r#"maxlength="256" autocomplete="name" placeholder="Your name""#);
    if fields.get("display-name").trim() != account.display_name {
        control.push_str(&settings::submit("Apply", Kind::Primary, true));
    }
    let name = format!(
        r#"<form fx-submit="save-name">{}</form>"#,
        settings::group(
            "Name",
            &settings::row("Your name as others see it", "What the desktop calls you, from the next time you sign in.", &control),
            ""
        )
    );
    let facts = settings::group(
        "Account",
        &format!("{}{}", settings::fact("Account name", &account.name, false), settings::fact("Signs in", policy_words(account.policy), false)),
        &settings::hint("The account name you sign in with, and how you sign in, are an administrator's to change, in Principals Manager."),
    );
    format!("{head}{hero}{name}{facts}")
}

pub fn render_password(account: &Account, fields: &Fields) -> String {
    let head = settings::head(Glyph::Lock, Tile::Green, "Password", "What you sign in with, here and over SSH.");
    let account = match account {
        Err(why) => return format!("{head}{}", unreadable(why)),
        Ok(account) => account,
    };
    if !can_prove(account) {
        return format!(
            "{head}{}",
            settings::banner(&format!(
                "Your account signs in {}, so there is no password to change here. An administrator can give it one, in Principals Manager.",
                policy_words(account.policy)
            ))
        );
    }
    let field = |name: &str, label: &str, complete: &str| {
        settings::row(label, "", &settings::text(name, label, "password", Width::Normal, true, &format!(r#"autocomplete="{complete}""#)))
    };
    let ready = !fields.get("current").is_empty() && !fields.get("new").is_empty() && !fields.get("confirm").is_empty();
    let rows = format!(
        "{}{}{}",
        field("current", "Current password", "current-password"),
        field("new", "New password", "new-password"),
        field("confirm", "New password again", "new-password"),
    );
    let foot = format!(
        "{}{}",
        settings::actions(&settings::submit("Change password", Kind::Primary, ready)),
        settings::hint("It takes effect the next time you sign in; you stay signed in now.")
    );
    format!(r#"{head}<form fx-submit="change-password">{}</form>"#, settings::group("Change your password", &rows, &foot))
}

pub fn render_keys(account: &Account, fields: &Fields, removing: Option<&Removing>, adding: bool) -> String {
    let head = settings::head(Glyph::Key, Tile::Teal, "SSH keys", "Sign in from another computer without typing your password.");
    let account = match account {
        Err(why) => return format!("{head}{}", unreadable(why)),
        Ok(account) => account,
    };
    let proves = can_prove(account);
    let mut rows = String::new();
    for key in &account.keys {
        let added = jiff::Timestamp::from_second(key.created as i64).map(|t| t.strftime("%-d %b %Y").to_string()).unwrap_or_default();
        let this = removing.is_some_and(|r| r.fingerprint == key.fingerprint);
        let remove = if proves && !this && removing.is_none() && !adding {
            settings::button("Remove", "remove-key", &[("fingerprint", &key.fingerprint), ("label", &key.label)], Kind::Plain, true)
        } else {
            String::new()
        };
        let kind = format!("{} · added {added}", key.algorithm.trim_start_matches("ssh-").replace("ed25519", "Ed25519").replace("rsa", "RSA"));
        rows.push_str(&settings::item(
            &settings::icon(Glyph::Key, Tile::Teal),
            if key.label.is_empty() { "A key" } else { &key.label },
            &[(&key.fingerprint, true), (&kind, false)],
            &remove,
        ));
        if this {
            rows.push_str(&settings::more(
                More::Asking,
                &format!(
                    r#"<form fx-submit="confirm-remove"><p>Remove <b>{what}</b>? You won't be able to sign in with it. Enter your password to confirm.</p>
                       <div class="fields"><label>Your password<input class="st-input wide" type="password" name="remove-password" autocomplete="current-password" fx-autofocus></label></div>{actions}</form>"#,
                    what = escape(if key.label.is_empty() { &key.fingerprint } else { &key.label }),
                    actions = settings::actions(&format!(
                        "{}{}",
                        settings::button("Keep it", "cancel-remove", &[], Kind::Plain, true),
                        settings::submit("Remove key", Kind::Danger, true)
                    )),
                ),
            ));
        }
    }
    if account.keys.is_empty() {
        let about = if proves { "Add the public key of each computer you sign in from." } else { "" };
        rows.push_str(&settings::row("No keys yet", about, ""));
    }
    if proves {
        if adding {
            let ready = !fields.get("key").trim().is_empty() && !fields.get("key-password").is_empty();
            rows.push_str(&settings::row("Add a key", "The one line of your .pub file, from the computer you sign in from.", ""));
            rows.push_str(&settings::more(
                More::Form,
                &format!(
                    r#"<form fx-submit="add-key"><textarea class="st-input" name="key" rows="3" spellcheck="false" aria-label="Public key" placeholder="ssh-ed25519 AAAA… you@laptop" fx-autofocus></textarea>
                       <p>Ed25519 keys are taken, and RSA keys of 3072 bits or more. Your password is asked for, since a key is a way into your account.</p>
                       <div class="fields"><label>Your password<input class="st-input wide" type="password" name="key-password" autocomplete="current-password"></label></div>{}</form>"#,
                    settings::actions(&format!(
                        "{}{}",
                        settings::button("Cancel", "cancel-add", &[], Kind::Plain, true),
                        settings::submit("Add key", Kind::Primary, ready)
                    ))
                ),
            ));
        } else {
            rows.push_str(&settings::row(
                "Add a key",
                "The one line of your .pub file, from the computer you sign in from.",
                &settings::button("Add key…", "open-add", &[], Kind::Plain, removing.is_none()),
            ));
        }
    }
    let mut foot = String::new();
    if !proves {
        foot.push_str(&settings::locked(
            "Changing your keys asks for your password first, and your account has none to give, so they are an administrator's to change, in Principals Manager.",
        ));
    } else if matches!(account.policy, Policy::Password) && !account.keys.is_empty() {
        foot.push_str(&settings::note(
            "Your account signs in with a password only, so these keys aren't used until an administrator lets it sign in with a key too.",
        ));
    }
    format!("{head}{}", settings::group("Your keys", &rows, &foot))
}

pub fn save_name(fields: &Fields) -> Result<String, String> {
    let name = fields.get("display-name").trim().to_string();
    Own::new().set_display_name(&name).map_err(|refusal| refusal.reason)?;
    Ok(if name.is_empty() { "Your name is taken away.".into() } else { format!("Your name is {name}.") })
}

/// Answers each round from what the form holds, in order, and keeps every
/// message the authority sent, in order, for the window to show with the
/// outcome (PGSS §2.8: a client displays what it is sent, and doesn't read
/// meaning into it). The form was filled before the conversation began, so
/// they are shown after it rather than before each prompt.
struct Form {
    answers: std::collections::VecDeque<Secret>,
    said: Vec<String>,
}

impl Collector for Form {
    fn round(&mut self, round: &Round) -> Result<Vec<Secret>, Abandon> {
        self.said.extend(round.messages.iter().map(|notice| notice.text.clone()));
        if self.answers.len() < round.prompts.len() {
            return Err(Abandon::new("It asked for more than the form holds."));
        }
        Ok(round.prompts.iter().filter_map(|_| self.answers.pop_front()).collect())
    }
}

impl Form {
    /// What the authority said, then the window's own words, a line each.
    fn with(&self, ours: &str) -> String {
        self.said.iter().map(String::as_str).chain([ours]).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n")
    }
}

fn form(answers: &[&str]) -> Form {
    Form { answers: answers.iter().map(|a| Secret::from_slice(a.as_bytes())).collect(), said: Vec::new() }
}

/// The outcome of a conversation, in words: what the authority said, and
/// what the refusal's code means. The code is branched on, never the text.
fn outcome(result: Result<(), Refusal>, form: &Form, done: &str, what: &str) -> Result<String, String> {
    let refusal = match result {
        Ok(()) => return Ok(form.with(done)),
        Err(refusal) => refusal,
    };
    Err(if refusal.wrong_password() {
        form.with("That isn't your current password.")
    } else if refusal.credential_rejected() {
        form.with(&format!("That key can't be added: {refusal}."))
    } else if refusal.outcome_unknown() {
        form.with(&format!("It isn't known whether {what} happened: the authority stopped answering. Look again in a moment."))
    } else if form.said.is_empty() {
        refusal.to_string()
    } else {
        // A refusal's own words repeat the last message it was sent.
        form.with("")
    })
}

pub fn change_password(fields: &Fields) -> Result<String, String> {
    let (current, new, confirm) = (fields.get("current"), fields.get("new"), fields.get("confirm"));
    if new != confirm {
        return Err("The new password and the one again aren't the same.".into());
    }
    let mut collector = form(&[current, new, confirm]);
    let result = Credentials::new().change_password(&mut collector);
    outcome(result, &collector, "Your password is changed. Use it the next time you sign in.", "the change")
}

pub fn add_key(fields: &Fields) -> Result<String, String> {
    let line = fields.get("key").trim().to_string();
    if line.lines().count() != 1 {
        return Err("A public key is one line.".into());
    }
    let mut collector = form(&[fields.get("key-password")]);
    let result = Credentials::new().add_key(&line, &mut collector);
    outcome(result, &collector, "The key is added.", "adding it")
}

pub fn remove_key(removing: &Removing, fields: &Fields) -> Result<String, String> {
    let mut collector = form(&[fields.get("remove-password")]);
    let result = Credentials::new().remove_key(&removing.fingerprint, &mut collector);
    outcome(result, &collector, "The key is taken away.", "taking it away")
}

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
use libauthd_client::credential::{Abandon, Collector, Credentials, MessageSeverity, Refusal, Round};
use libauthd_client::own::{Own, OwnAccount};
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

pub fn render(account: &Account, fields: &Fields, removing: Option<&Removing>) -> String {
    let account = match account {
        Err(why) => return format!("<section class=\"card\"><h2>Your account</h2><p class=\"note bad\">{}</p></section>", escape(why)),
        Ok(account) => account,
    };
    let proves = can_prove(account);

    let name_changed = fields.get("display-name").trim() != account.display_name;
    let name_card = format!(
        "<section class=\"card\" aria-label=\"Your name\"><h2>Your name</h2>\
         <dl class=\"facts\"><dt>Signed in as</dt><dd>{name}</dd><dt>Signs in</dt><dd>{how}</dd></dl>\
         <form class=\"edit\" fx-submit=\"save-name\">\
         <label>Your name as others see it<input name=\"display-name\" maxlength=\"256\" autocomplete=\"name\"></label>\
         <p class=\"hint\">What the desktop calls you, from the next time you sign in. The account name you sign in with is an administrator's to change.</p>\
         <div class=\"actions\"><button class=\"primary\"{off}>Save</button></div></form></section>",
        name = escape(&account.name),
        how = policy_words(account.policy),
        off = if name_changed { "" } else { " disabled" },
    );

    let password_card = if proves {
        let ready = !fields.get("current").is_empty() && !fields.get("new").is_empty() && !fields.get("confirm").is_empty();
        format!(
            "<section class=\"card\" aria-label=\"Password\"><h2>Password</h2>\
             <form class=\"edit\" fx-submit=\"change-password\">\
             <label>Current password<input type=\"password\" name=\"current\" autocomplete=\"current-password\"></label>\
             <label>New password<input type=\"password\" name=\"new\" autocomplete=\"new-password\"></label>\
             <label>New password again<input type=\"password\" name=\"confirm\" autocomplete=\"new-password\"></label>\
             <p class=\"hint\">It takes effect the next time you sign in; you stay signed in now.</p>\
             <div class=\"actions\"><button class=\"primary\"{off}>Change password</button></div></form></section>",
            off = if ready { "" } else { " disabled" },
        )
    } else {
        format!(
            "<section class=\"card\" aria-label=\"Password\"><h2>Password</h2>\
             <p class=\"note\">Your account signs in {}, so there is no password to change here. An administrator can give it one, in Principals Manager.</p></section>",
            policy_words(account.policy)
        )
    };

    let mut rows = String::new();
    for key in &account.keys {
        let added = jiff::Timestamp::from_second(key.created as i64).map(|t| t.strftime("%-d %b %Y").to_string()).unwrap_or_default();
        let remove = if proves {
            format!(
                "<button class=\"small\" fx-click=\"remove-key\" fx-value-fingerprint=\"{f}\" fx-value-label=\"{l}\">Remove</button>",
                f = escape(&key.fingerprint),
                l = escape(&key.label)
            )
        } else {
            String::new()
        };
        rows.push_str(&format!(
            "<li><span class=\"key\"><strong>{label}</strong><code>{fp}</code><small>{alg}, added {added}</small></span>{remove}</li>",
            label = escape(if key.label.is_empty() { "A key" } else { &key.label }),
            fp = escape(&key.fingerprint),
            alg = escape(&key.algorithm),
        ));
    }
    let list = if rows.is_empty() { "<p class=\"note\">You have no SSH keys.</p>".to_string() } else { format!("<ul class=\"keys\">{rows}</ul>") };
    let asking = match removing {
        Some(removing) if proves => format!(
            "<form class=\"edit asking\" fx-submit=\"confirm-remove\"><p>Take away {what}? You won't be able to sign in with it.</p>\
             <label>Your password<input type=\"password\" name=\"remove-password\" autocomplete=\"current-password\" fx-autofocus></label>\
             <div class=\"actions\"><button class=\"primary danger\">Remove it</button><button type=\"button\" fx-click=\"cancel-remove\">Keep it</button></div></form>",
            what = escape(if removing.label.is_empty() { &removing.fingerprint } else { &removing.label })
        ),
        _ => String::new(),
    };
    // One password field at a time: adding waits while removing is asked.
    let add = if removing.is_some() && proves {
        String::new()
    } else if proves {
        let ready = !fields.get("key").trim().is_empty() && !fields.get("key-password").is_empty();
        format!(
            "<form class=\"edit\" fx-submit=\"add-key\">\
             <label>Add a public key<textarea name=\"key\" rows=\"3\" spellcheck=\"false\" placeholder=\"ssh-ed25519 AAAA… you@laptop\"></textarea></label>\
             <p class=\"hint\">The one line of your <code>.pub</code> file, from the computer you sign in from. Ed25519 keys are taken, and RSA keys of 3072 bits or more.</p>\
             <label>Your password<input type=\"password\" name=\"key-password\" autocomplete=\"current-password\"></label>\
             <div class=\"actions\"><button class=\"primary\"{off}>Add key</button></div></form>",
            off = if ready { "" } else { " disabled" },
        )
    } else {
        "<p class=\"note\">Changing your keys asks for your password first, and your account has none to give, so they are an administrator's to change, in Principals Manager.</p>".into()
    };
    let note = if matches!(account.policy, Policy::Password) && !account.keys.is_empty() {
        "<p class=\"hint\">Your account signs in with a password only, so these keys aren't used until an administrator lets it sign in with a key too.</p>"
    } else {
        ""
    };
    let keys_card = format!("<section class=\"card\" aria-label=\"SSH keys\"><h2>SSH keys</h2>{list}{note}{asking}{add}</section>");
    format!("{name_card}{password_card}{keys_card}")
}

pub fn save_name(fields: &Fields) -> Result<String, String> {
    let name = fields.get("display-name").trim().to_string();
    Own::new().set_display_name(&name).map_err(|refusal| refusal.reason)?;
    Ok(if name.is_empty() { "Your name is taken away.".into() } else { format!("Your name is {name}.") })
}

/// Answers each round from what the form holds, in order, and keeps what
/// the authority said, for the window to show.
struct Form {
    answers: std::collections::VecDeque<Secret>,
    said: Vec<String>,
}

impl Collector for Form {
    fn round(&mut self, round: &Round) -> Result<Vec<Secret>, Abandon> {
        for notice in &round.messages {
            if notice.severity == MessageSeverity::Error {
                self.said.push(notice.text.clone());
            }
        }
        if self.answers.len() < round.prompts.len() {
            return Err(Abandon::new(self.said.last().cloned().unwrap_or_else(|| "It asked for more than was given.".into())));
        }
        Ok(round.prompts.iter().filter_map(|_| self.answers.pop_front()).collect())
    }
}

fn form(answers: &[&str]) -> Form {
    Form { answers: answers.iter().map(|a| Secret::from_slice(a.as_bytes())).collect(), said: Vec::new() }
}

/// What a refusal means, in words.
fn refused(refusal: &Refusal, form: &Form, what: &str) -> String {
    if refusal.wrong_password() {
        return "That isn't your current password.".into();
    }
    if refusal.outcome_unknown() {
        return format!("It isn't known whether {what} happened: the authority stopped answering. Look again in a moment.");
    }
    match form.said.last() {
        Some(said) => said.clone(),
        None => refusal.to_string(),
    }
}

pub fn change_password(fields: &Fields) -> Result<String, String> {
    let (current, new, confirm) = (fields.get("current"), fields.get("new"), fields.get("confirm"));
    if new != confirm {
        return Err("The new password and the one again aren't the same.".into());
    }
    let mut collector = form(&[current, new, confirm]);
    match Credentials::new().change_password(&mut collector) {
        Ok(()) => Ok("Your password is changed. Use it the next time you sign in.".into()),
        Err(refusal) => Err(refused(&refusal, &collector, "the change")),
    }
}

pub fn add_key(fields: &Fields) -> Result<String, String> {
    let line = fields.get("key").trim().to_string();
    if line.lines().count() != 1 {
        return Err("A public key is one line.".into());
    }
    let mut collector = form(&[fields.get("key-password")]);
    match Credentials::new().add_key(&line, &mut collector) {
        Ok(()) => Ok("The key is added.".into()),
        Err(refusal) if refusal.credential_rejected() => Err(format!("That key can't be added: {refusal}.")),
        Err(refusal) => Err(refused(&refusal, &collector, "adding it")),
    }
}

pub fn remove_key(removing: &Removing, fields: &Fields) -> Result<String, String> {
    let mut collector = form(&[fields.get("remove-password")]);
    match Credentials::new().remove_key(&removing.fingerprint, &mut collector) {
        Ok(()) => Ok("The key is taken away.".into()),
        Err(refusal) => Err(refused(&refusal, &collector, "taking it away")),
    }
}

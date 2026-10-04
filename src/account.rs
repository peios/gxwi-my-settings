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

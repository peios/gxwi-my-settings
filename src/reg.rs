//! The registry, as these settings use it: read a value, find out whether
//! this person may change a key, and change it.
//!
//! Whether a key may be changed is asked of the kernel, by opening it for
//! what a change needs, and never worked out from who the person is. A key
//! that isn't there yet is asked about through its parent, which is where
//! it would be made.

use std::io::ErrorKind;

use peios::registry::{CreateFlags, Data, Key, KeyAccess, OpenFlags, Transaction};

/// A value as it is, or `None` where it isn't there, or can't be read.
pub fn get(path: &str, name: &str) -> Option<Data> {
    let key = Key::open(None, path, KeyAccess::QUERY_VALUE, OpenFlags::empty()).ok()?;
    let value = key.query_value(name.as_bytes(), None).ok()?;
    Some(Data::decode(value.ty, &value.data))
}

pub fn text(path: &str, name: &str) -> Option<String> {
    match get(path, name)? {
        Data::Sz(text) | Data::ExpandSz(text) => Some(text),
        _ => None,
    }
}

/// Whether this person may change the values of the key at `path`: asked by
/// opening it to set values, or, where it isn't there yet, its parent to
/// make it in. `what` says what is being changed, for the reason.
pub fn may_change(path: &str, what: &str) -> Result<(), String> {
    match Key::open(None, path, KeyAccess::SET_VALUE, OpenFlags::empty()) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let parent = path.rsplit_once('\\').map_or("Machine", |(parent, _)| parent);
            Key::open(None, parent, KeyAccess::CREATE_SUB_KEY, OpenFlags::empty())
                .map(|_| ())
                .map_err(|error| refused(&error, what, path))
        }
        Err(error) => Err(refused(&error, what, path)),
    }
}

/// Why a change was refused, in words.
pub fn refused(error: &peios::Error, what: &str, _path: &str) -> String {
    match error.kind() {
        // Every key these settings write is the person's own, so a refusal
        // means someone has narrowed it on purpose.
        ErrorKind::PermissionDenied => format!("You can look, but you can't change {what}: someone has narrowed who may."),
        _ => format!("{what} can't be changed: {error}."),
    }
}

/// Values to set, `None` to remove one, all made at once or not at all.
pub fn set(path: &str, what: &str, values: &[(&str, Option<Data>)]) -> Result<(), String> {
    let txn = Transaction::begin().map_err(|e| format!("Couldn't change {what}: {e}."))?;
    let (key, _) = Key::create(None, path, KeyAccess::SET_VALUE, CreateFlags::empty(), None, Some(&txn))
        .map_err(|e| refused(&e, what, path))?;
    for (name, value) in values {
        match value {
            Some(data) => key
                .set_value(name.as_bytes(), data.ty(), &data.encode())
                .in_txn(&txn)
                .call()
                .map_err(|e| refused(&e, what, path))?,
            None => match key.delete_value(name.as_bytes(), None, Some(&txn)) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(refused(&e, what, path)),
            },
        }
    }
    txn.commit().map_err(|e| format!("Couldn't change {what}: {e}."))
}

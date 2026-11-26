//! Where Tor keeps what it remembers, by the bridges that are set.
//!
//! Arti writes the bridges it has used into its state, and what it writes
//! there does not survive a change of bridges. A bridge that is taken out
//! of the configuration is marked as no longer listed. Put back, it loses
//! the mark only if Arti never learned more about it than its line says.
//! If Arti had read the bridge's own description, which it does before it
//! uses one, it wants that description again before it takes the mark off,
//! and it asks for the description only of bridges that have no mark
//! (tor-guardmgr 0.48: `Guard::update_from_universe`, the `Uncertain` arm,
//! and `GuardSet::descriptors_to_request`). So the bridges that worked
//! before are the ones never tried again, for the twenty days Arti keeps
//! such an entry, and Tor is left with the bridges that never answered.
//! That is how it was found: on a phone, with obfs4 set, then Snowflake,
//! then obfs4 again.
//!
//! So what Tor remembers with bridges belongs to one set of bridges. It is
//! kept in a folder of its own inside the state folder, beside a note of
//! the bridge lines it belongs to, and it starts empty when the lines are
//! others. What Tor remembers without bridges is in the state folder
//! itself, as it always was, and none of this touches it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::bridges::Bridges;
use crate::service::TorError;

/// The folder inside the state folder for what Tor remembers with bridges.
const WITH_BRIDGES: &str = "bridged";
/// The note beside it of the bridge lines that folder belongs to. It says
/// nothing that Arti's own state in the folder does not say.
const BELONGS_TO: &str = "bridged.lines";

/// The folder Tor is to keep its state in with these bridges. Makes and
/// changes nothing.
pub(crate) fn folder(state_dir: &Path, bridges: &Bridges) -> PathBuf {
    if bridges.is_empty() {
        state_dir.to_path_buf()
    } else {
        state_dir.join(WITH_BRIDGES)
    }
}

/// Sees to it that the folder for these bridges holds nothing remembered
/// with other bridges: when the lines are not the ones it was last used
/// with, it is emptied. `state_dir` has to exist. Without bridges this
/// does nothing.
pub(crate) fn keep_for(state_dir: &Path, bridges: &Bridges) -> Result<(), TorError> {
    if bridges.is_empty() {
        return Ok(());
    }
    let note = state_dir.join(BELONGS_TO);
    let lines = note_of(bridges);
    // A note that is not there or cannot be read is a note of other lines.
    if fs::read_to_string(&note).is_ok_and(|kept| kept == lines) {
        return Ok(());
    }

    let folder = state_dir.join(WITH_BRIDGES);
    let failed = |path: &Path| {
        let path = path.to_path_buf();
        move |source: io::Error| TorError::Directory { path, source }
    };
    // The folder goes before the note is written: ended in between, the
    // note still names other lines and the folder is emptied again.
    match fs::remove_dir_all(&folder) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(failed(&folder)(error)),
    }
    // Written beside itself and then put in place, so that half a note is
    // never read as a note.
    let unfinished = state_dir.join(format!("{BELONGS_TO}.new"));
    fs::write(&unfinished, &lines).map_err(failed(&unfinished))?;
    fs::rename(&unfinished, &note).map_err(failed(&note))
}

/// The lines as the note holds them: each once, in an order of their own,
/// so that the same bridges written in another order are the same bridges.
fn note_of(bridges: &Bridges) -> String {
    let mut lines: Vec<&str> = bridges.lines.iter().map(|line| line.trim()).collect();
    lines.sort_unstable();
    lines.dedup();
    let mut note = lines.join("\n");
    note.push('\n');
    note
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str =
        "obfs4 192.0.2.1:443 0123456789ABCDEF0123456789ABCDEF01234567 cert=abc iat-mode=0";
    const TWO: &str =
        "obfs4 192.0.2.2:443 89ABCDEF0123456789ABCDEF0123456789ABCDEF cert=def iat-mode=0";
    const OTHER: &str = "snowflake 192.0.2.3:80 2B280B23E1107BB62ABFC40DDCC8824814F80A72";

    fn bridges(lines: &[&str]) -> Bridges {
        Bridges {
            lines: lines.iter().map(|line| line.to_string()).collect(),
            transports: Vec::new(),
        }
    }

    fn state_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("veil-tor-state-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// What a session with these bridges does to the state folder before
    /// Tor starts; then something is left there as Tor would leave it.
    fn a_session(state_dir: &Path, with: &Bridges) -> PathBuf {
        keep_for(state_dir, with).expect("the folder can be kept");
        let folder = folder(state_dir, with);
        fs::create_dir_all(folder.join("state")).unwrap();
        folder.join("state").join("guards.json")
    }

    #[test]
    fn without_bridges_tor_remembers_where_it_always_did() {
        let dir = state_dir("direct");
        let none = Bridges::none();
        assert_eq!(folder(&dir, &none), dir);
        keep_for(&dir, &none).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "nothing is made");
        let _ = fs::remove_dir_all(dir);
    }

    /// The way it went wrong on a phone: bridges, then others, then the
    /// first again. Each change starts from nothing; no change keeps what
    /// there is.
    #[test]
    fn what_tor_remembers_goes_with_the_bridges_it_was_for() {
        let dir = state_dir("bridged");
        // What Tor remembers without bridges, which nothing here may touch.
        fs::create_dir_all(dir.join("state")).unwrap();
        let direct = dir.join("state").join("guards.json");
        fs::write(&direct, "direct").unwrap();

        let first = bridges(&[ONE, TWO]);
        let remembered = a_session(&dir, &first);
        assert_ne!(remembered, direct, "bridges have a folder of their own");
        assert!(!remembered.exists());
        fs::write(&remembered, "first").unwrap();

        // The same bridges again, written in another order and with
        // spaces around them: kept.
        let same = bridges(&[&format!("  {TWO} "), ONE, ONE]);
        assert_eq!(a_session(&dir, &same), remembered);
        assert_eq!(fs::read_to_string(&remembered).unwrap(), "first");

        // Other bridges: nothing of the first is left.
        assert_eq!(a_session(&dir, &bridges(&[OTHER])), remembered);
        assert!(!remembered.exists(), "other bridges start from nothing");
        fs::write(&remembered, "other").unwrap();

        // The first again: nothing of the others, and nothing of the
        // first time either.
        a_session(&dir, &first);
        assert!(!remembered.exists(), "bridges put back start from nothing");
        fs::write(&remembered, "first again").unwrap();

        // One bridge fewer, one bridge more: each is another set.
        a_session(&dir, &bridges(&[ONE]));
        assert!(!remembered.exists(), "a bridge taken away is another set");
        fs::write(&remembered, "one").unwrap();
        a_session(&dir, &first);
        assert!(!remembered.exists(), "a bridge added is another set");

        // A session without bridges in between changes nothing for them.
        fs::write(&remembered, "first once more").unwrap();
        keep_for(&dir, &Bridges::none()).unwrap();
        a_session(&dir, &first);
        assert_eq!(fs::read_to_string(&remembered).unwrap(), "first once more");

        assert_eq!(fs::read_to_string(&direct).unwrap(), "direct");
        let _ = fs::remove_dir_all(dir);
    }

    /// A note that was cut short, or is no note at all, is a note of other
    /// bridges.
    #[test]
    fn a_note_that_cannot_be_read_starts_from_nothing() {
        let dir = state_dir("note");
        let set = bridges(&[ONE, TWO]);
        let remembered = a_session(&dir, &set);
        fs::write(&remembered, "kept").unwrap();

        let note = dir.join(BELONGS_TO);
        let whole = fs::read(&note).unwrap();
        fs::write(&note, &whole[..whole.len() / 2]).unwrap();
        a_session(&dir, &set);
        assert!(!remembered.exists(), "half a note is not this set's note");
        assert_eq!(fs::read(&note).unwrap(), whole, "and is written anew");

        fs::write(&remembered, "kept").unwrap();
        fs::write(&note, [0xff, 0xfe, 0x00]).unwrap();
        a_session(&dir, &set);
        assert!(!remembered.exists());
        let _ = fs::remove_dir_all(dir);
    }
}

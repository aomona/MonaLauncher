//! Discard the official, regeneratable profile-key cache before starting a game.
//! Never parse, print, copy, or back up old private-key material into a readable location.
use std::{fs, io, path::Path};

pub(super) fn clear(game: &Path) -> io::Result<()> {
    // The caller has validated the managed game directory. remove_dir_all does not follow
    // links inside the cache (including the cache entry itself) on our supported platforms.
    // Do not ignore other errors: launching with an uncleared old secret is unsafe.
    match fs::remove_dir_all(game.join("profilekeys")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_all_accounts_cache_and_preserves_other_game_data() {
        let root = tempfile::tempdir().unwrap();
        let game = root.path();
        fs::create_dir(game.join("profilekeys")).unwrap();
        fs::write(game.join("profilekeys/account-a.json"), "old synthetic key").unwrap();
        fs::write(game.join("profilekeys/account-b.json"), "old synthetic key").unwrap();
        fs::write(game.join("options.txt"), "preserve").unwrap();
        clear(game).unwrap();
        clear(game).unwrap();
        assert!(!game.join("profilekeys").exists());
        assert_eq!(
            fs::read_to_string(game.join("options.txt")).unwrap(),
            "preserve"
        );
    }

    #[test]
    fn unexpected_file_is_preserved_and_blocks_launch() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("profilekeys"), "unexpected").unwrap();
        assert!(clear(root.path()).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("profilekeys")).unwrap(),
            "unexpected"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unlinks_cache_links_without_deleting_the_target() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside");
        let game = root.path().join("game");
        fs::create_dir(&outside).unwrap();
        fs::create_dir(&game).unwrap();
        fs::write(outside.join("preserve"), "outside").unwrap();
        symlink(&outside, game.join("profilekeys")).unwrap();
        clear(&game).unwrap();
        assert!(fs::symlink_metadata(game.join("profilekeys")).is_err());
        fs::create_dir(game.join("profilekeys")).unwrap();
        symlink(&outside, game.join("profilekeys/nested")).unwrap();
        clear(&game).unwrap();
        assert_eq!(
            fs::read_to_string(outside.join("preserve")).unwrap(),
            "outside"
        );
    }
}

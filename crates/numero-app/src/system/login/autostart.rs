//! Linux: an XDG autostart entry, `$XDG_CONFIG_HOME/autostart/dev.soldunov.numero.desktop`,
//! which every desktop that follows the XDG Autostart spec starts at login.
//! The entry exists exactly while the setting is on; one the user turned
//! off in their desktop's settings (`Hidden=true`, or GNOME's
//! `X-GNOME-Autostart-enabled=false`) reads as off.
//!
//! The entry starts the binary that wrote it: the AppImage when there is
//! one, else the path on `PATH` that leads to this binary (a Nix profile's
//! link outlives the store path behind it), else the binary itself.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::app::{APP_ID, ICON_NAME};

/// The entry's place under the XDG config directory.
fn entry_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("autostart").join(format!("{APP_ID}.desktop")))
}

/// Whether the entry is there and on.
pub(in crate::system) fn is_on() -> bool {
    entry_path().is_some_and(|path| match fs::read_to_string(&path) {
        Ok(content) => enabled(&content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "couldn't read the autostart entry");
            false
        }
    })
}

/// Writes the entry, or removes it.
pub(in crate::system) fn set(on: bool) -> Result<(), String> {
    let path = entry_path().ok_or("there is no config directory to put the autostart entry in")?;
    if on {
        let program = program().ok_or("couldn't find this program's own path")?;
        let content = entry(&program)?;
        write(&path, &content)
            .map_err(|error| format!("couldn't write {}: {error}", path.display()))?;
        tracing::info!(path = %path.display(), program = %program.display(), "Numero opens at login");
    } else {
        match fs::remove_file(&path) {
            Ok(()) => tracing::info!(path = %path.display(), "Numero no longer opens at login"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("couldn't remove {}: {error}", path.display())),
        }
    }
    Ok(())
}

fn write(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("desktop.tmp");
    fs::write(&temporary, content)?;
    fs::rename(&temporary, path)
}

/// The program the entry starts.
fn program() -> Option<PathBuf> {
    if let Some(appimage) = env::var_os("APPIMAGE").filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(appimage));
    }
    let exe = env::current_exe().ok()?;
    let on_path = env::var_os("PATH").and_then(|path| on_path(&exe, &path));
    Some(on_path.unwrap_or(exe))
}

/// The entry of `path`, a `PATH`-style list, that leads to the same file as
/// `exe`, by `exe`'s name.
fn on_path(exe: &Path, path: &OsStr) -> Option<PathBuf> {
    let name = exe.file_name()?;
    let real = fs::canonicalize(exe).ok()?;
    env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| fs::canonicalize(candidate).is_ok_and(|found| found == real))
}

/// The autostart entry that starts `program`.
fn entry(program: &Path) -> Result<String, String> {
    let program = program
        .to_str()
        .ok_or("the program's path isn't UTF-8, which a desktop entry needs")?;
    if program.contains(['\n', '\r']) {
        return Err("the program's path has a line break in it".to_owned());
    }
    Ok(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Numero\n\
         Comment=Text calculator\n\
         Exec={}\n\
         Icon={ICON_NAME}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_argument(program)
    ))
}

/// `argument` quoted for an `Exec` key: in double quotes, with `"`, `` ` ``,
/// `$` and `\` escaped, `%` doubled, and then every backslash escaped once
/// more, as the string type of the value asks.
fn exec_argument(argument: &str) -> String {
    let mut quoted = String::with_capacity(argument.len() + 2);
    quoted.push('"');
    for character in argument.chars() {
        match character {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

/// Whether the entry `content` starts the app: not hidden, nor turned off
/// for GNOME.
fn enabled(content: &str) -> bool {
    let mut in_entry = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let off = match key.trim() {
            "Hidden" => value.trim() == "true",
            "X-GNOME-Autostart-enabled" => value.trim() == "false",
            _ => false,
        };
        if off {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn the_entry_starts_the_program_and_reads_back_as_on() {
        let content = entry(Path::new("/home/me/.nix-profile/bin/numero-app")).unwrap();
        assert!(content.starts_with("[Desktop Entry]\nType=Application\n"));
        assert!(content.contains("\nExec=\"/home/me/.nix-profile/bin/numero-app\"\n"));
        assert!(content.contains("\nName=Numero\n"));
        assert!(enabled(&content));
    }

    #[test]
    fn a_path_with_spaces_and_specials_is_quoted_and_escaped() {
        assert_eq!(
            exec_argument("/opt/My Apps/numero"),
            "\"/opt/My Apps/numero\""
        );
        assert_eq!(exec_argument("/a/100%/b"), "\"/a/100%%/b\"");
        assert_eq!(exec_argument("/a/$HOME"), "\"/a/\\\\$HOME\"");
        assert_eq!(exec_argument("/a\\b"), "\"/a\\\\\\\\b\"");
        assert!(entry(Path::new("/bad\npath")).is_err());
    }

    #[test]
    fn an_entry_turned_off_by_the_desktop_reads_as_off() {
        assert!(!enabled("[Desktop Entry]\nType=Application\nHidden=true\n"));
        assert!(!enabled(
            "[Desktop Entry]\nExec=numero\nX-GNOME-Autostart-enabled=false\n"
        ));
        // Keys of another group don't count.
        assert!(enabled(
            "[Desktop Entry]\nExec=numero\n[Desktop Action new]\nHidden=true\n"
        ));
    }

    #[test]
    fn writing_and_removing_the_entry_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("autostart").join("numero.desktop");
        let content = entry(Path::new("/usr/bin/numero-app")).unwrap();

        write(&path, &content).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        assert!(enabled(&fs::read_to_string(&path).unwrap()));
        fs::remove_file(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn the_entry_prefers_the_link_on_path_to_the_store_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("store");
        let profile = dir.path().join("profile");
        fs::create_dir_all(&store).unwrap();
        fs::create_dir_all(&profile).unwrap();
        let exe = store.join("numero-app");
        fs::write(&exe, "").unwrap();
        symlink(&exe, profile.join("numero-app")).unwrap();
        let path = env::join_paths([dir.path().join("elsewhere"), profile.clone()]).unwrap();

        assert_eq!(on_path(&exe, &path), Some(profile.join("numero-app")));
        assert_eq!(on_path(&exe, OsStr::new("")), None);
    }
}

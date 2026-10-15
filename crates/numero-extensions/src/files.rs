//! The extensions folder on disk: the seed `Default.js` and the sorted,
//! recursive list of `*.js` files to run.

use std::io;
use std::path::{Path, PathBuf};

/// The file seeded into an empty extensions folder.
pub const DEFAULT_FILE: &str = "Default.js";

/// What `Default.js` starts as: Numi's three examples, all commented out.
pub const DEFAULT_SOURCE: &str = r#"// Numero extensions
// Every .js file in this folder (and its subfolders) runs once when Numero
// starts, so restart Numero after changing one. Remove the leading "//" to
// try an example.

// A variable every document can use:
// numi.setVariable("budget", { "double": 250, "unitId": "USD" });
// numi.setVariable("answer", 42);

// A new unit: 1 horse is 2.4 metres, so "3 horses in m" gives 7.2 m.
// numi.addUnit({
//     "id": "horse",
//     "phrases": "horse, horses, hrs",
//     "baseUnitId": "m",
//     "format": "hrs",
//     "ratio": 2.4,
// });

// A new function: zum(2; 3) adds its two arguments and gives 5.
// numi.addFunction({ "id": "zum", "phrases": "zum" }, function (values) {
//     return { "double": values[0].double + values[1].double };
// });

// log("anything") writes a line to the extensions log.
"#;

/// Create `dir` if needed and write `Default.js` when it is missing.
pub fn seed(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let default = dir.join(DEFAULT_FILE);
    if !default.exists() {
        std::fs::write(default, DEFAULT_SOURCE)?;
    }
    Ok(())
}

/// A script to run: its path relative to the folder (with `/`) and where
/// it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFile {
    pub name: String,
    pub path: PathBuf,
}

/// Every `*.js` file under `dir`, in nested folders too, sorted by
/// relative path. Unreadable folders become errors in the second list.
pub fn scripts(dir: &Path) -> (Vec<ScriptFile>, Vec<String>) {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    walk(dir, dir, &mut found, &mut errors);
    found.sort_by(|a, b| a.name.cmp(&b.name));
    (found, errors)
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<ScriptFile>, errors: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("Error reading {}: {error}", dir.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `file_type` does not follow symlinks, so a linked folder cannot
        // loop; a linked file still counts.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(root, &path, found, errors);
        } else if is_script(&path) && (kind.is_file() || path.is_file()) {
            found.push(ScriptFile {
                name: relative_name(root, &path),
                path,
            });
        }
    }
}

fn is_script(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "js")
}

fn relative_name(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

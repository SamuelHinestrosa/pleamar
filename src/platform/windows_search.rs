//! Bounded filename search without a shell, an indexer or file-content reads.
use super::SysValue;
use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};
use std::os::windows::fs::MetadataExt;

pub fn query(args: &[SysValue]) -> Result<SysValue, String> {
    let [SysValue::Text(root), SysValue::Text(query)] = args else {
        return Err("search.files takes an absolute folder and a filename fragment".into());
    };
    let root = Path::new(root);
    if !root.is_absolute() || !root.is_dir() { return Err("search root must be an existing absolute folder".into()); }
    if query.chars().count() < 3 || query.len() > 256 || query.contains('\0') {
        return Err("search needs between 3 and 256 characters".into());
    }
    search(root, query, 12, 40_000, Duration::from_millis(250))
}

fn search(root: &Path, query: &str, limit: usize, budget: usize, timeout: Duration) -> Result<SysValue, String> {
    let needle = query.to_lowercase();
    let mut folders = VecDeque::from([(root.to_owned(), 0)]);
    let mut found = Vec::new();
    let mut visited = 0;
    let start = Instant::now();
    let mut truncated = false;
    'walk: while let Some((folder, depth)) = folders.pop_front() {
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(e) if depth == 0 => return Err(e.to_string()),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if visited >= budget || start.elapsed() >= timeout || found.len() >= limit {
                truncated = true;
                break 'walk;
            }
            visited += 1;
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(name.to_lowercase().as_str(), ".git" | "node_modules" | ".cache" | "appdata" | "$recycle.bin" | "target") { continue; }
            let Ok(meta) = entry.metadata() else { continue; };
            // Junctions can leave the chosen root or lead back to an ancestor.
            if meta.file_attributes() & 0x400 != 0 { continue; }
            let directory = meta.is_dir();
            let path = entry.path();
            if name.to_lowercase().contains(&needle) {
                found.push(SysValue::Map(vec![
                    ("name".into(), SysValue::Text(name)),
                    ("path".into(), SysValue::Text(path.to_string_lossy().replace('\\', "/"))),
                    ("directory".into(), SysValue::Bool(directory)),
                ]));
            }
            if directory && depth < 3 { folders.push_back((path, depth + 1)); }
        }
    }
    Ok(SysValue::Map(vec![
        ("items".into(), SysValue::List(found)),
        ("truncated".into(), SysValue::Bool(truncated)),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_literal_names_and_search_budget() {
        let root = std::env::temp_dir().join(format!("pleamar search ñ {}", std::process::id()));
        std::fs::create_dir_all(root.join("folder")).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
        let _cleanup = Cleanup(root.clone());
        std::fs::write(root.join("folder/Canción [2026].txt"), "test").unwrap();
        let result = search(&root, "CANCIÓN [", 12, 100, Duration::from_secs(1)).unwrap();
        let SysValue::Map(fields) = result else { panic!() };
        let SysValue::List(items) = &fields[0].1 else { panic!() };
        assert_eq!(items.len(), 1);
        assert_eq!(fields[1].1, SysValue::Bool(false));
        let SysValue::Map(fields) = search(&root, "absent", 12, 0, Duration::from_secs(1)).unwrap() else { panic!() };
        assert_eq!(fields[1].1, SysValue::Bool(true));
        assert!(query(&[SysValue::Text("relative".into()), SysValue::Text("word".into())]).is_err());
    }
}

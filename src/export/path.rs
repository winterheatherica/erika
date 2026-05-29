use std::path::{Path, PathBuf};

pub fn build_dynamic_path(user_input: &str, default_dir: &str, default_ext: &str) -> PathBuf {
    let parts = parse_user_input(user_input, default_dir, default_ext);
    let (y, m, d) = today_local_ymd();
    pick_next_available(&parts, y, m, d)
}

struct UserPathParts {
    dir: PathBuf,
    stem: String,
    ext: String,
}

fn parse_user_input(user_input: &str, default_dir: &str, default_ext: &str) -> UserPathParts {
    let p = Path::new(user_input);
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("output")
        .to_string();
    let ext = p
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(default_ext)
        .to_string();
    let dir = p
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(default_dir));
    UserPathParts { dir, stem, ext }
}

fn pick_next_available(parts: &UserPathParts, y: i32, m: u32, d: u32) -> PathBuf {
    for id in 1..=9999u32 {
        let candidate = parts
            .dir
            .join(formatted_filename(&parts.stem, &parts.ext, y, m, d, id));
        if !candidate.exists() {
            return candidate;
        }
    }
    parts
        .dir
        .join(formatted_filename(&parts.stem, &parts.ext, y, m, d, 9999))
}

fn formatted_filename(stem: &str, ext: &str, y: i32, m: u32, d: u32, id: u32) -> String {
    format!("{stem}-{y:04}-{m:02}-{d:02}-{id:04}.{ext}")
}

fn today_local_ymd() -> (i32, u32, u32) {
    use chrono::Datelike;
    let now = chrono::Local::now();
    (now.year(), now.month(), now.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_path() {
        let p = parse_user_input("./export/png/output.png", "./default", "png");
        assert_eq!(p.stem, "output");
        assert_eq!(p.ext, "png");
        assert_eq!(p.dir, PathBuf::from("./export/png"));
    }

    #[test]
    fn parse_missing_extension_uses_default() {
        let p = parse_user_input("./export/png/myart", "./default", "png");
        assert_eq!(p.stem, "myart");
        assert_eq!(p.ext, "png");
    }

    #[test]
    fn parse_bare_stem_uses_defaults() {
        let p = parse_user_input("myart", "./default", "png");
        assert_eq!(p.stem, "myart");
        assert_eq!(p.ext, "png");
        assert_eq!(p.dir, PathBuf::from("./default"));
    }

    #[test]
    fn parse_empty_input_uses_all_defaults() {
        let p = parse_user_input("", "./default", "png");
        assert_eq!(p.stem, "output");
        assert_eq!(p.ext, "png");
        assert_eq!(p.dir, PathBuf::from("./default"));
    }

    #[test]
    fn formatted_filename_padding() {
        assert_eq!(
            formatted_filename("myart", "png", 2026, 5, 28, 1),
            "myart-2026-05-28-0001.png"
        );
        assert_eq!(
            formatted_filename("output", "svg", 2026, 12, 9, 42),
            "output-2026-12-09-0042.svg"
        );
        assert_eq!(
            formatted_filename("a", "tex", 2026, 1, 1, 9999),
            "a-2026-01-01-9999.tex"
        );
    }

    #[test]
    fn pick_next_available_returns_id_one_when_dir_empty() {
        let tmp = std::env::temp_dir().join(format!("erika_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let parts = UserPathParts {
            dir: tmp.clone(),
            stem: "uniquestem".into(),
            ext: "txt".into(),
        };
        let chosen = pick_next_available(&parts, 2026, 5, 28);
        assert_eq!(
            chosen,
            tmp.join("uniquestem-2026-05-28-0001.txt"),
            "first file should use id 0001"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pick_next_available_skips_existing_ids() {
        let tmp = std::env::temp_dir().join(format!("erika_test2_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let parts = UserPathParts {
            dir: tmp.clone(),
            stem: "skiptest".into(),
            ext: "txt".into(),
        };
        std::fs::write(tmp.join("skiptest-2026-05-28-0001.txt"), b"").unwrap();
        std::fs::write(tmp.join("skiptest-2026-05-28-0002.txt"), b"").unwrap();
        let chosen = pick_next_available(&parts, 2026, 5, 28);
        assert_eq!(chosen, tmp.join("skiptest-2026-05-28-0003.txt"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// contenteditable 里「空的」往往存成 `<br>`、`<div><br></div>`，
/// 光看字符串非空会把空便签误判成有内容。这里去标签后再判断。
pub fn is_blank(body: &str) -> bool {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in body.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let cleaned = text
        .replace("&nbsp;", " ")
        .replace('\u{200B}', "")
        .replace('\u{00A0}', " ");
    cleaned.trim().is_empty()
}

fn default_color() -> String {
    "paper".into()
}
fn default_font_size() -> u32 {
    13
}
fn default_w() -> f64 {
    260.0
}
fn default_h() -> f64 {
    170.0
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default = "default_w")]
    pub w: f64,
    #[serde(default = "default_h")]
    pub h: f64,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub ghost: bool,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub deleted_at: u64,
}

impl Note {
    pub fn new(settings: &Settings) -> Self {
        Self {
            id: format!("n{}", now_ms()),
            body: String::new(),
            color: settings.default_color.clone(),
            font_size: settings.default_font_size,
            x: 140.0,
            y: 140.0,
            w: 260.0,
            h: 170.0,
            collapsed: false,
            pinned: false,
            ghost: false,
            updated_at: now_ms(),
            deleted_at: 0,
        }
    }

    /// 显示器拔掉或分辨率变化后，把窗口钳回可见区域，避免便签永远找不到。
    pub fn sanitize(&mut self) {
        if !self.x.is_finite() || !self.y.is_finite() {
            self.x = 140.0;
            self.y = 140.0;
        }
        if self.x < -40.0 || self.x > 6000.0 || self.y < -40.0 || self.y > 4000.0 {
            self.x = 140.0;
            self.y = 140.0;
        }
        if self.w < 180.0 {
            self.w = 260.0;
        }
        if self.h < 120.0 {
            self.h = 170.0;
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_color")]
    pub default_color: String,
    #[serde(default = "default_font_size")]
    pub default_font_size: u32,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub auto_backup: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_color: default_color(),
            default_font_size: default_font_size(),
            auto_start: false,
            auto_backup: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub settings: Settings,
}

impl Store {
    fn path(dir: &Path) -> PathBuf {
        dir.join("notes.json")
    }

    pub fn load(dir: &Path) -> Self {
        let path = Self::path(dir);
        let mut store: Store = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Store::default(),
        };
        for note in store.notes.iter_mut() {
            note.sanitize();
        }
        // 空便签是「直接丢弃」的语义，deleted_at 打的是 u64::MAX，启动时顺手清掉
        store.notes.retain(|n| n.deleted_at != u64::MAX);
        store
    }

    /// 原子写入：先写临时文件并落盘，再 rename 覆盖。
    /// 中途断电最多丢掉最后一次改动，不会留下半个 JSON。
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let path = Self::path(dir);
        let tmp = dir.join("notes.json.tmp");
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)
    }

    pub fn live(&self) -> impl Iterator<Item = &Note> {
        self.notes.iter().filter(|n| n.deleted_at == 0)
    }

    pub fn deleted(&self) -> impl Iterator<Item = &Note> {
        self.notes
            .iter()
            .filter(|n| n.deleted_at != 0 && n.deleted_at != u64::MAX)
    }

    /// 每天首次启动留一份快照，保留最近 7 份。
    pub fn backup_today(&self, dir: &Path) -> std::io::Result<()> {
        let bdir = dir.join("backups");
        fs::create_dir_all(&bdir)?;
        let target = bdir.join(format!("notes-{}.json", today_stamp()));
        if target.exists() {
            return Ok(());
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(&target, text)?;
        prune_backups(&bdir, 7);
        Ok(())
    }
}

fn prune_backups(dir: &Path, keep: usize) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("notes-") && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    while files.len() > keep {
        let old = files.remove(0);
        let _ = fs::remove_file(old);
    }
}

/// 公历日期（Howard Hinnant 的 civil_from_days），不引额外依赖。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn today_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_detection() {
        assert!(is_blank(""));
        assert!(is_blank("<br>"));
        assert!(is_blank("<div><br></div>"));
        assert!(is_blank("<div><br><br></div>"));
        assert!(is_blank("<div>&nbsp;</div>"));
        assert!(!is_blank("<div>一</div>"));
        assert!(!is_blank("字"));
        assert!(!is_blank("<div><span>a</span></div>"));
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(31), (1970, 2, 1));
        assert_eq!(civil_from_days(365), (1971, 1, 1));
    }

    #[test]
    fn stamp_shape() {
        let s = today_stamp();
        assert_eq!(s.len(), 10, "{}", s);
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
    }
}

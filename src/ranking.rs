//! クリアタイムのランキング(ステージ × モード別、上位 10 件)。
//! exe と同じフォルダの `ranking.tsv` に「stage<TAB>mode<TAB>ms<TAB>name」で保存する。

use std::path::PathBuf;

pub const MAX_ENTRIES: usize = 10;
pub const NAME_MAX: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub stage: usize,
    pub hi: bool,
    pub ms: u32,
    pub name: String,
}

pub struct Ranking {
    pub entries: Vec<Entry>,
    pub last_name: String,
    path: Option<PathBuf>,
}

fn data_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("ranking.tsv"))
}

fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(NAME_MAX).collect::<String>().trim().to_string()
}

pub fn format_time(ms: u32) -> String {
    let total = ms / 10;
    let (m, s, cs) = (total / 6000, (total / 100) % 60, total % 100);
    format!("{m:02}:{s:02}.{cs:02}")
}

impl Ranking {
    pub fn load() -> Ranking {
        let path = data_path();
        let mut r = Ranking { entries: Vec::new(), last_name: String::new(), path: path.clone() };
        if let Some(p) = path {
            if let Ok(text) = std::fs::read_to_string(p) {
                r.parse(&text);
            }
        }
        r
    }

    fn parse(&mut self, text: &str) {
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f.as_slice() {
                ["last", name] => self.last_name = clean_name(name),
                [stage, mode, ms, name] => {
                    if let (Ok(stage), Ok(ms)) = (stage.parse(), ms.parse()) {
                        self.entries.push(Entry { stage, hi: *mode == "hi", ms, name: clean_name(name) });
                    }
                }
                _ => {}
            }
        }
    }

    fn serialize(&self) -> String {
        let mut s = format!("last\t{}\n", self.last_name);
        for e in &self.entries {
            s.push_str(&format!("{}\t{}\t{}\t{}\n", e.stage, if e.hi { "hi" } else { "normal" }, e.ms, e.name));
        }
        s
    }

    fn save(&self) {
        if let Some(p) = &self.path {
            let _ = std::fs::write(p, self.serialize());
        }
    }

    /// 指定ステージ/モードの上位(速い順)
    pub fn top(&self, stage: usize, hi: bool) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.entries.iter().filter(|e| e.stage == stage && e.hi == hi).collect();
        v.sort_by_key(|e| e.ms);
        v.truncate(MAX_ENTRIES);
        v
    }

    /// このタイムが何位に入るか(1 始まり)。圏外なら None
    pub fn rank_of(&self, stage: usize, hi: bool, ms: u32) -> Option<usize> {
        let better = self.entries.iter().filter(|e| e.stage == stage && e.hi == hi && e.ms <= ms).count();
        if better < MAX_ENTRIES { Some(better + 1) } else { None }
    }

    pub fn add(&mut self, stage: usize, hi: bool, ms: u32, name: &str) {
        let mut name = clean_name(name);
        if name.is_empty() {
            name = "NO NAME".to_string();
        }
        self.last_name = name.clone();
        self.entries.push(Entry { stage, hi, ms, name });
        // 各 (stage, mode) の上位だけ残す
        let mut keep: Vec<Entry> = Vec::new();
        for st in 0..64 {
            for hi in [false, true] {
                let mut v: Vec<Entry> = self.entries.iter().filter(|e| e.stage == st && e.hi == hi).cloned().collect();
                v.sort_by_key(|e| e.ms);
                v.truncate(MAX_ENTRIES);
                keep.extend(v);
            }
        }
        self.entries = keep;
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_rank() {
        let mut r = Ranking { entries: Vec::new(), last_name: String::new(), path: None };
        for i in 0..12u32 {
            r.add(0, false, 60_000 + i * 1000, &format!("P{i}"));
        }
        assert_eq!(r.top(0, false).len(), MAX_ENTRIES);
        assert_eq!(r.top(0, false)[0].name, "P0");
        assert_eq!(r.rank_of(0, false, 30_000), Some(1));
        assert_eq!(r.rank_of(0, false, 999_999), None);
        assert!(r.top(0, true).is_empty());

        let text = r.serialize();
        let mut r2 = Ranking { entries: Vec::new(), last_name: String::new(), path: None };
        r2.parse(&text);
        assert_eq!(r2.entries.len(), r.entries.len());
        assert_eq!(r2.last_name, "P11");
        assert_eq!(format_time(65_430), "01:05.43");
    }
}

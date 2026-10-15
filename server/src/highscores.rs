//! The leaderboards: every run against the waves that came to an end (lost, or left under way),
//! kept in a CSV file (no database yet), a line a run, appended as it ends and read back at start.
//! Its columns: `date` (UTC, ISO 8601), `version` (the game's), `wave` (the furthest reached),
//! `map`, `players`, `party` (`Name (hero)`, `; ` between them). A player who made a board names
//! their entry after the run: the file is written out again with it.

use std::io::Write as _;
use std::path::PathBuf;

use arena_shared::classes::ClassId;
use arena_shared::map::MapId;
use arena_shared::rooms::{HighScore, LEADERBOARD_ROWS, MAX_PARTY};
use bevy::prelude::*;

const HEADER: &str = "date,version,wave,map,players,party";

/// Every run on record, and the file they're kept in (none: only in memory, as in tests).
#[derive(Default)]
pub struct HighScores {
    path: Option<PathBuf>,
    all: Vec<HighScore>,
}

impl HighScores {
    /// The runs kept at `path` (none yet if there's no file).
    pub fn load(path: Option<PathBuf>) -> HighScores {
        let text = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        let all: Vec<HighScore> = text.lines().skip(1).filter(|line| !line.trim().is_empty()).filter_map(parse).collect();
        info!(path = ?path, runs = all.len(), "high scores loaded");
        HighScores { path, all }
    }

    /// A run came to an end: kept, and written to the file. Its index, to name it by.
    pub fn record(&mut self, score: HighScore) -> usize {
        info!(wave = score.wave, players = score.party.len(), "a run on the leaderboards");
        if let Some(path) = &self.path {
            let fresh = !path.exists();
            let written = std::fs::OpenOptions::new().create(true).append(true).open(path).and_then(|mut file| {
                if fresh {
                    writeln!(file, "{HEADER}")?;
                }
                writeln!(file, "{}", line(&score))
            });
            if let Err(error) = written {
                warn!(?path, %error, "couldn't write the high scores");
            }
        }
        self.all.push(score);
        self.all.len() - 1
    }

    /// Run `index`.
    pub fn get(&self, index: usize) -> Option<&HighScore> {
        self.all.get(index)
    }

    /// Where run `index` stands among every run by as many fighters (1: the best), and how many
    /// those are.
    pub fn standing(&self, index: usize) -> Option<(usize, usize)> {
        let score = self.all.get(index)?;
        let alike: Vec<&HighScore> = self.all.iter().filter(|s| s.party.len() == score.party.len()).collect();
        let better = self.all.iter().enumerate().filter(|(i, s)| *i != index && s.party.len() == score.party.len() && HighScore::rank(s, score).is_lt()).count();
        Some((better + 1, alike.len()))
    }

    /// Run `index`'s `member` goes by `name` on the boards: the file is written out again.
    pub fn rename(&mut self, index: usize, member: usize, name: String) {
        let Some(entry) = self.all.get_mut(index).and_then(|s| s.party.get_mut(member)) else { return };
        entry.0 = name;
        let Some(path) = &self.path else { return };
        let text: String = std::iter::once(HEADER.to_string()).chain(self.all.iter().map(line)).map(|l| l + "\n").collect();
        // Written beside it, then swapped in: never half a file.
        let fresh = path.with_extension("csv.new");
        if let Err(error) = std::fs::write(&fresh, text).and_then(|_| std::fs::rename(&fresh, path)) {
            warn!(?path, %error, "couldn't write the high scores");
        }
    }

    /// Each party size's best (`LEADERBOARD_ROWS` at most), best first.
    pub fn boards(&self) -> Vec<HighScore> {
        let mut boards = Vec::new();
        for size in 1..=MAX_PARTY {
            let mut board: Vec<&HighScore> = self.all.iter().filter(|s| s.party.len() == size).collect();
            board.sort_by(|a, b| HighScore::rank(a, b));
            boards.extend(board.into_iter().take(LEADERBOARD_ROWS).cloned());
        }
        boards
    }
}

/// Now, as a UTC date and time (ISO 8601, to the second: `2026-10-15T20:41:12Z`).
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    iso_date(secs)
}

/// Unix seconds as a UTC date and time (ISO 8601).
fn iso_date(secs: u64) -> String {
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (month <= 2) as i64;
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest / 60 % 60, rest % 60)
}

fn line(score: &HighScore) -> String {
    let party: Vec<String> = score.party.iter().map(|(name, class)| format!("{name} ({})", class.def().id)).collect();
    let fields = [score.date.clone(), score.version.clone(), score.wave.to_string(), format!("{:?}", score.map), score.party.len().to_string(), party.join("; ")];
    fields.iter().map(|f| quoted(f)).collect::<Vec<_>>().join(",")
}

/// A CSV field: as it is, or in quotes (its own doubled) if it has a comma, quote or line break.
fn quoted(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) { format!("\"{}\"", field.replace('"', "\"\"")) } else { field.to_string() }
}

/// A CSV line's fields (quoted ones unquoted).
fn fields(line: &str) -> Vec<String> {
    let (mut all, mut field, mut quoting) = (Vec::new(), String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoting && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoting = !quoting,
            ',' if !quoting => all.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    all.push(field);
    all
}

fn parse(line: &str) -> Option<HighScore> {
    let f = fields(line);
    let [date, version, wave, map, _, party] = f.as_slice() else { return None };
    let map = MapId::ALL.into_iter().find(|m| format!("{m:?}") == *map)?;
    let party = party
        .split("; ")
        .map(|member| {
            let (name, class) = member.rsplit_once(" (")?;
            Some((name.to_string(), ClassId::by_key(class.strip_suffix(')')?)?))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(HighScore { wave: wave.parse().ok()?, map, party, date: date.clone(), version: version.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(wave: u32, size: usize, date: &str) -> HighScore {
        let class = ClassId::playable().next().unwrap();
        HighScore { wave, map: MapId::Wilds, party: vec![("A".into(), class); size], date: date.into(), version: "0.0.1".into() }
    }

    #[test]
    fn a_line_reads_back_as_written() {
        let class = ClassId::playable().next().unwrap();
        let score = HighScore { party: vec![("Ketil, \"the\" Bold".into(), class), ("Astrid".into(), class)], ..run(12, 2, "2026-10-15T20:41:12Z") };
        assert_eq!(parse(&line(&score)), Some(score));
        assert_eq!(parse(HEADER), None);
    }

    #[test]
    fn dates_are_utc() {
        assert_eq!(iso_date(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_date(1_760_560_872), "2025-10-15T20:41:12Z");
        assert_eq!(iso_date(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn boards_are_by_party_size_best_first() {
        let mut scores = HighScores::default();
        for score in [run(3, 1, "2026-01-01"), run(9, 1, "2026-01-02"), run(9, 1, "2026-01-01"), run(4, 2, "2026-01-03")] {
            scores.record(score);
        }
        let boards = scores.boards();
        let solo: Vec<(u32, &str)> = boards.iter().filter(|s| s.party.len() == 1).map(|s| (s.wave, s.date.as_str())).collect();
        assert_eq!(solo, [(9, "2026-01-01"), (9, "2026-01-02"), (3, "2026-01-01")]);
        assert_eq!(boards.iter().filter(|s| s.party.len() == 2).count(), 1);
        assert_eq!([2, 1, 0, 3].map(|i| scores.standing(i)), [Some((1, 3)), Some((2, 3)), Some((3, 3)), Some((1, 1))]);
        scores.rename(0, 0, "Me".into());
        assert_eq!(scores.boards().iter().find(|s| s.wave == 3).unwrap().party[0].0, "Me");
    }

    #[test]
    fn off_the_board_still_has_a_place() {
        let mut scores = HighScores::default();
        for i in 0..LEADERBOARD_ROWS {
            scores.record(run(10, 1, &format!("2026-01-{i:02}")));
        }
        let low = scores.record(run(2, 1, "2026-02-01"));
        assert_eq!(scores.standing(low), Some((LEADERBOARD_ROWS + 1, LEADERBOARD_ROWS + 1)));
        assert!(!scores.boards().iter().any(|s| s.wave == 2), "past the top, on the board");
    }
}

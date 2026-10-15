//! The leaderboards: every run against the waves that came to an end (lost, or left under way),
//! kept in a CSV file (no database yet), a line a run, appended as it ends and read back at start.
//! Its columns: `time` (unix seconds), `wave` (the furthest reached), `map`, `players`, `party`
//! (`Name (class)`, `; ` between them).

use std::io::Write as _;
use std::path::PathBuf;

use arena_shared::classes::ClassId;
use arena_shared::map::MapId;
use arena_shared::rooms::{HighScore, LEADERBOARD_ROWS, MAX_PARTY};
use bevy::prelude::*;

const HEADER: &str = "time,wave,map,players,party";

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

    /// A run came to an end: kept, and written to the file.
    pub fn record(&mut self, score: HighScore) {
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

/// Unix seconds now.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn line(score: &HighScore) -> String {
    let party: Vec<String> = score.party.iter().map(|(name, class)| format!("{name} ({})", class.def().id)).collect();
    let fields = [score.at.to_string(), score.wave.to_string(), format!("{:?}", score.map), score.party.len().to_string(), party.join("; ")];
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
    let [at, wave, map, _, party] = f.as_slice() else { return None };
    let map = MapId::ALL.into_iter().find(|m| format!("{m:?}") == *map)?;
    let party = party
        .split("; ")
        .map(|member| {
            let (name, class) = member.rsplit_once(" (")?;
            Some((name.to_string(), ClassId::by_key(class.strip_suffix(')')?)?))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(HighScore { wave: wave.parse().ok()?, map, party, at: at.parse().ok()? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_reads_back_as_written() {
        let class = ClassId::playable().next().unwrap();
        let score = HighScore { wave: 12, map: MapId::Wilds, party: vec![("Ketil, \"the\" Bold".into(), class), ("Astrid".into(), class)], at: 1_700_000_000 };
        assert_eq!(parse(&line(&score)), Some(score));
        assert_eq!(parse(HEADER), None);
    }

    #[test]
    fn boards_are_by_party_size_best_first() {
        let class = ClassId::playable().next().unwrap();
        let run = |wave, size: usize, at| HighScore { wave, map: MapId::Wilds, party: vec![("A".into(), class); size], at };
        let mut scores = HighScores::default();
        for score in [run(3, 1, 1), run(9, 1, 2), run(9, 1, 1), run(4, 2, 3)] {
            scores.record(score);
        }
        let boards = scores.boards();
        let solo: Vec<(u32, u64)> = boards.iter().filter(|s| s.party.len() == 1).map(|s| (s.wave, s.at)).collect();
        assert_eq!(solo, [(9, 1), (9, 2), (3, 1)]);
        assert_eq!(boards.iter().filter(|s| s.party.len() == 2).count(), 1);
    }
}

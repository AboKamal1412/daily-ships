//! Persistent state (`%LOCALAPPDATA%\DailyShips\state.json`) and ship selection.
//!
//! Everything here is best effort: a missing, unreadable or corrupted state
//! file simply means "start fresh", and a failed write never stops the popup.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ships::Ship;

const APP_DIR: &str = "DailyShips";
const STATE_FILE: &str = "state.json";
const LOG_FILE: &str = "error.log";
/// A real state file is a few hundred bytes; anything huge is treated as corrupt.
const MAX_STATE_BYTES: u64 = 64 * 1024;
const MAX_LOG_BYTES: u64 = 256 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Id of the ship shown most recently.
    #[serde(default)]
    pub last_ship_id: Option<String>,
    /// Ids already shown in the current cycle. Every ship is shown once
    /// before any ship repeats; the list resets when the cycle completes.
    #[serde(default)]
    pub seen: Vec<String>,
    /// Local date (`YYYY-MM-DD`) on which "Don't show today" was clicked.
    #[serde(default)]
    pub snooze_date: Option<String>,
}

/// `%LOCALAPPDATA%\DailyShips`, falling back to `%APPDATA%` and then the temp dir.
pub fn data_dir() -> PathBuf {
    let base = ["LOCALAPPDATA", "APPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|path| path.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    base.join(APP_DIR)
}

pub struct Store {
    dir: PathBuf,
    pub state: State,
}

impl Store {
    /// Loads the saved state, or a default one if there is none or it is damaged.
    pub fn load() -> Self {
        Self::load_from(data_dir())
    }

    pub fn load_from(dir: PathBuf) -> Self {
        let path = dir.join(STATE_FILE);
        let state = fs::metadata(&path)
            .ok()
            .filter(|meta| meta.is_file() && meta.len() <= MAX_STATE_BYTES)
            .and_then(|_| fs::read(&path).ok())
            .and_then(|bytes| serde_json::from_slice::<State>(&bytes).ok())
            .unwrap_or_default();
        Self { dir, state }
    }

    /// Writes the state atomically (temp file, then rename).
    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&self.state).map_err(|err| err.to_string())?;
        fs::create_dir_all(&self.dir).map_err(|err| err.to_string())?;
        let tmp = self.dir.join(format!("{STATE_FILE}.tmp"));
        fs::write(&tmp, json).map_err(|err| err.to_string())?;
        fs::rename(&tmp, self.dir.join(STATE_FILE)).map_err(|err| {
            let _ = fs::remove_file(&tmp);
            err.to_string()
        })
    }

    /// Saves and records (rather than surfaces) any failure.
    pub fn save_best_effort(&self) {
        if let Err(err) = self.save() {
            log_error(&format!("could not save state: {err}"));
        }
    }

    pub fn is_snoozed(&self, today: &str) -> bool {
        self.state.snooze_date.as_deref() == Some(today)
    }
}

/// Appends a line to `%LOCALAPPDATA%\DailyShips\error.log`. The app has no
/// console, so this is the only place problems are reported. Never fails.
pub fn log_error(message: &str) {
    let dir = data_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(LOG_FILE);
    if fs::metadata(&path)
        .map(|meta| meta.len() > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let _ = fs::remove_file(&path);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[{}] {message}", crate::platform::local_date());
    }
}

/// Small non-cryptographic random number generator (SplitMix64), seeded from
/// the operating system's randomness via `RandomState` plus the clock.
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Self {
        let os_random = RandomState::new().build_hasher().finish();
        let clock = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0);
        Self(os_random ^ clock.rotate_left(29))
    }

    #[cfg(test)]
    pub fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-enough index in `0..n` (`n` must be non-zero).
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Picks the next ship and records it in `state`.
///
/// * Never returns the ship shown last time (unless it is the only ship).
/// * Shows every ship once before repeating any ("shuffle bag").
///
/// Returns `None` only for an empty database.
pub fn pick_next(ships: &[Ship], state: &mut State, rng: &mut Rng) -> Option<usize> {
    if ships.is_empty() {
        return None;
    }
    let last = state.last_ship_id.clone();
    let is_last = |ship: &Ship| last.as_deref() == Some(ship.id.as_str());

    // Forget ids that no longer exist in the database.
    state
        .seen
        .retain(|id| ships.iter().any(|ship| &ship.id == id));

    let mut candidates: Vec<usize> = (0..ships.len())
        .filter(|&i| !is_last(&ships[i]) && !state.seen.contains(&ships[i].id))
        .collect();
    if candidates.is_empty() {
        // Cycle complete: start a new one, still avoiding an immediate repeat.
        state.seen.clear();
        candidates = (0..ships.len()).filter(|&i| !is_last(&ships[i])).collect();
    }
    if candidates.is_empty() {
        // Single-ship database.
        candidates.push(0);
    }

    let index = candidates[rng.below(candidates.len())];
    record_shown(&ships[index], state);
    Some(index)
}

/// Marks `ship` as the one currently displayed.
pub fn record_shown(ship: &Ship, state: &mut State) {
    if !state.seen.contains(&ship.id) {
        state.seen.push(ship.id.clone());
    }
    state.last_ship_id = Some(ship.id.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fleet(n: usize) -> Vec<Ship> {
        let json: Vec<String> = (0..n)
            .map(|i| {
                format!(
                    r#"{{"id":"s{i}","name":"Ship {i}","nation":"n","class":"c","launched":1900,
                        "fate":"f","description":"d","fun_fact":"x","image":"s{i}.jpg"}}"#
                )
            })
            .collect();
        crate::ships::parse(&format!("[{}]", json.join(","))).unwrap()
    }

    #[test]
    fn never_repeats_consecutively_and_cycles_through_all() {
        let ships = fleet(7);
        let mut state = State::default();
        let mut rng = Rng::seeded(42);
        let mut previous = None;
        let mut history = Vec::new();
        for _ in 0..7 * 50 {
            let index = pick_next(&ships, &mut state, &mut rng).unwrap();
            assert_ne!(Some(index), previous, "same ship twice in a row");
            previous = Some(index);
            history.push(index);
        }
        // Each block of 7 picks is a full cycle: every ship exactly once.
        for cycle in history.chunks(7) {
            let mut sorted = cycle.to_vec();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..7).collect::<Vec<_>>());
        }
    }

    #[test]
    fn two_ships_alternate_and_one_ship_repeats() {
        let ships = fleet(2);
        let mut state = State::default();
        let mut rng = Rng::seeded(7);
        let first = pick_next(&ships, &mut state, &mut rng).unwrap();
        for step in 1..20 {
            let index = pick_next(&ships, &mut state, &mut rng).unwrap();
            assert_eq!(index, (first + step) % 2);
        }

        let single = fleet(1);
        let mut state = State::default();
        assert_eq!(pick_next(&single, &mut state, &mut rng), Some(0));
        assert_eq!(pick_next(&single, &mut state, &mut rng), Some(0));
        assert_eq!(pick_next(&[], &mut state, &mut rng), None);
    }

    #[test]
    fn stale_ids_in_state_are_ignored() {
        let ships = fleet(3);
        let mut state = State {
            last_ship_id: Some("gone".into()),
            seen: vec!["gone".into(), "also-gone".into(), "s1".into()],
            snooze_date: None,
        };
        let mut rng = Rng::seeded(1);
        let index = pick_next(&ships, &mut state, &mut rng).unwrap();
        assert_ne!(ships[index].id, "s1");
        assert!(!state.seen.iter().any(|id| id.contains("gone")));
    }

    #[test]
    fn state_round_trips_and_survives_corruption() {
        let dir = std::env::temp_dir().join(format!("daily-ships-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        // Missing directory and file.
        let mut store = Store::load_from(dir.clone());
        assert_eq!(store.state, State::default());

        store.state.last_ship_id = Some("vasa".into());
        store.state.seen = vec!["vasa".into()];
        store.state.snooze_date = Some("2026-01-02".into());
        store.save().unwrap();
        let reloaded = Store::load_from(dir.clone());
        assert_eq!(reloaded.state, store.state);
        assert!(reloaded.is_snoozed("2026-01-02"));
        assert!(!reloaded.is_snoozed("2026-01-03"));

        // Corrupted, wrongly typed and empty files all fall back to defaults.
        for garbage in ["{ not json", r#"{"seen": 5}"#, "", "null", "[1,2,3]"] {
            fs::write(dir.join(STATE_FILE), garbage).unwrap();
            assert_eq!(Store::load_from(dir.clone()).state, State::default());
        }
        // Unknown and missing fields are tolerated.
        fs::write(
            dir.join(STATE_FILE),
            r#"{"last_ship_id":"fram","future_field":true}"#,
        )
        .unwrap();
        assert_eq!(
            Store::load_from(dir.clone()).state.last_ship_id.as_deref(),
            Some("fram")
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

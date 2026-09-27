// This file owns two things: what a saved Board looks like, and how it gets
// read from / written to disk. Nothing in here touches the UI — main.rs will
// call into BoardManager, but BoardManager never reaches back into egui.
//
// A Board's `content` field is the project's real `LaidOutDiagram` — the
// exact same type the app already draws and mutates on screen. There is
// deliberately no second, parallel "SavedBoard" shape with its own copies
// of boxes/circles/arrows/strokes; saving a board just means writing that
// same struct to a JSON file, and loading means reading it back.

use crate::layout::LaidOutDiagram;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Serialize, Deserialize)]
pub struct CameraState {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: String,
    pub title: String,
    pub created_at: u64, // Unix seconds. Plain integers instead of a date
    pub updated_at: u64, // library, since that's all sorting/"x ago" needs.
    pub camera: CameraState,
    pub content: LaidOutDiagram,
}

/// A lightweight summary used by the sidebar (Stage 4.2), so listing boards
/// doesn't require the caller to hold every board's full whiteboard content
/// in memory at once.
#[derive(Clone)]
pub struct BoardSummary {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
}

pub struct BoardManager {
    boards_dir: PathBuf,
}

impl BoardManager {
    /// Sets up (and creates, if needed) the local storage directory, then
    /// returns a manager pointed at it.
    pub fn new() -> io::Result<Self> {
        let boards_dir = data_dir().join("boards");
        fs::create_dir_all(&boards_dir)?;
        Ok(Self { boards_dir })
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.boards_dir.join(format!("{id}.json"))
    }

    /// Creates a new board with the given title and whiteboard content,
    /// saves it immediately, and returns it.
    pub fn create_board(&self, title: &str, content: LaidOutDiagram, camera: CameraState) -> io::Result<Board> {
        let now = current_timestamp();
        let board = Board { id: generate_board_id(), title: clean_title(title), created_at: now, updated_at: now, camera, content };
        self.save_board(&board)?;
        Ok(board)
    }

    /// Writes a board to disk. Saves to a temporary file first and only
    /// then renames it over the real file, so a crash or power loss
    /// mid-write can't leave a half-written, corrupted board file behind —
    /// the rename either fully happens or doesn't happen at all.
    pub fn save_board(&self, board: &Board) -> io::Result<()> {
        let json = serde_json::to_string_pretty(board).map_err(io::Error::other)?;
        let final_path = self.path_for(&board.id);
        let temp_path = self.boards_dir.join(format!("{}.tmp", board.id));
        fs::write(&temp_path, json)?;
        fs::rename(&temp_path, &final_path)?;
        Ok(())
    }

    /// Loads a board by id. Returns a plain error message (not a panic) if
    /// the file is missing or its JSON is corrupted, so the caller can show
    /// this to the user and fall back to another board instead of crashing.
    pub fn load_board(&self, id: &str) -> Result<Board, String> {
        let path = self.path_for(id);
        let data = fs::read_to_string(&path).map_err(|error| format!("Could not read board file: {error}"))?;
        serde_json::from_str(&data).map_err(|error| format!("Board file is corrupted or in an unrecognized format: {error}"))
    }

    /// Updates a board's title, marks it as just-modified, and saves it.
    pub fn rename_board(&self, board: &mut Board, new_title: &str) -> io::Result<()> {
        board.title = clean_title(new_title);
        board.updated_at = current_timestamp();
        self.save_board(board)
    }

    pub fn delete_board(&self, id: &str) -> io::Result<()> {
        let path = self.path_for(id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Lists every saved board, most recently updated first. A board whose
    /// file can't be read or parsed is skipped rather than failing the
    /// whole listing — one corrupted file shouldn't hide every other board.
    pub fn list_boards(&self) -> Vec<BoardSummary> {
        let mut summaries = Vec::new();

        let Ok(entries) = fs::read_dir(&self.boards_dir) else {
            return summaries;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(board) = serde_json::from_str::<Board>(&data) {
                    summaries.push(BoardSummary { id: board.id, title: board.title, updated_at: board.updated_at });
                }
            }
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        summaries
    }
}

fn clean_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() { "Untitled".to_string() } else { trimmed.to_string() }
}

fn current_timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A new id combines the current time (nanosecond resolution) into a short
/// hex string — no external crate needed, and collisions are effectively
/// impossible for a single-user local app creating boards one at a time.
fn generate_board_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("board_{nanos:x}")
}

/// Fedora (and Linux generally) convention: user application data lives
/// under `$XDG_DATA_HOME`, or `~/.local/share` if that's not set. This
/// reads it from the environment rather than hardcoding a username, and
/// avoids adding a directories/dirs crate for what's a two-line lookup.
fn data_dir() -> PathBuf {
    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        if !xdg_data_home.is_empty() {
            return PathBuf::from(xdg_data_home).join("explainboard");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("explainboard");
    }
    // Neither variable is set — extremely unlikely on a real Linux desktop
    // session, but fall back to a local folder instead of failing outright.
    PathBuf::from(".explainboard-data")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram;
    use crate::layout;

    /// Each test gets its own throwaway directory under the OS temp folder,
    /// so tests never touch your real boards or collide with each other.
    fn test_manager() -> BoardManager {
        let dir = std::env::temp_dir().join(format!("explainboard-test-{}", generate_board_id()));
        fs::create_dir_all(&dir).expect("failed to create temp test directory");
        BoardManager { boards_dir: dir }
    }

    fn zero_camera() -> CameraState {
        CameraState { x: 0.0, y: 0.0, zoom: 1.0 }
    }

    #[test]
    fn create_save_and_load_round_trip() {
        let manager = test_manager();
        let content = layout::layout(&diagram::example_tcp());
        let board = manager.create_board("Physics", content, zero_camera()).expect("create_board failed");

        let loaded = manager.load_board(&board.id).expect("load_board failed");
        assert_eq!(loaded.id, board.id);
        assert_eq!(loaded.title, "Physics");
        assert_eq!(loaded.content.boxes.len(), board.content.boxes.len());
        assert_eq!(loaded.content.arrow_links.len(), board.content.arrow_links.len());
    }

    #[test]
    fn blank_title_becomes_untitled() {
        let manager = test_manager();
        let board = manager.create_board("   ", layout::layout(&diagram::example_tcp()), zero_camera()).unwrap();
        assert_eq!(board.title, "Untitled");
    }

    #[test]
    fn rename_keeps_the_same_id() {
        let manager = test_manager();
        let mut board = manager.create_board("Untitled", layout::layout(&diagram::example_tcp()), zero_camera()).unwrap();
        let original_id = board.id.clone();

        manager.rename_board(&mut board, "Physics — Magnetic Fields").unwrap();
        assert_eq!(board.id, original_id);
        assert_eq!(board.title, "Physics — Magnetic Fields");

        let reloaded = manager.load_board(&original_id).unwrap();
        assert_eq!(reloaded.title, "Physics — Magnetic Fields");
    }

    #[test]
    fn list_boards_sorts_most_recently_updated_first() {
        let manager = test_manager();
        let first = manager.create_board("First", layout::layout(&diagram::example_tcp()), zero_camera()).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let second = manager.create_board("Second", layout::layout(&diagram::example_dns()), zero_camera()).unwrap();

        let summaries = manager.list_boards();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, second.id);
        assert_eq!(summaries[1].id, first.id);
    }

    #[test]
    fn delete_board_removes_its_file() {
        let manager = test_manager();
        let board = manager.create_board("Temporary", layout::layout(&diagram::example_tcp()), zero_camera()).unwrap();
        manager.delete_board(&board.id).unwrap();
        assert!(manager.load_board(&board.id).is_err());
    }

    #[test]
    fn corrupted_file_fails_without_panicking() {
        let manager = test_manager();
        fs::write(manager.boards_dir.join("board_bad.json"), "{ this is not valid json").unwrap();
        let result = manager.load_board("board_bad");
        assert!(result.is_err());
    }
}
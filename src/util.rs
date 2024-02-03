use std::path::PathBuf;

// Some useful structs and functions


/// Indicates a position in a file
/// All fields start at 0. Even lines.
#[derive(Clone, Debug)]
pub struct FilePosition {
    pub file_path: PathBuf,
    pub absolute_position: usize,
    pub line: usize,
    pub line_character: usize
}


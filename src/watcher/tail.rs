use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Tracks byte offsets for append-only files to enable incremental reading.
/// Used for events.jsonl to avoid re-parsing entire file on each update.
///
/// # Functional Core Principle
/// This struct holds pure data (offsets). The I/O operations (`read_new_lines`)
/// are in methods but clearly separated as imperative shell.
#[derive(Debug, Clone)]
pub struct TailState {
    /// Map from file path to last read byte offset
    offsets: HashMap<PathBuf, u64>,
}

impl TailState {
    /// Create new empty tail state
    pub fn new() -> Self {
        Self {
            offsets: HashMap::new(),
        }
    }

    /// Get current offset for a file (0 if never read)
    pub fn get_offset(&self, path: &Path) -> u64 {
        self.offsets.get(path).copied().unwrap_or(0)
    }

    /// Update offset for a file
    pub fn set_offset(&mut self, path: PathBuf, offset: u64) {
        self.offsets.insert(path, offset);
    }

    /// Read only new content from file since last read.
    ///
    /// # Imperative Shell
    /// Performs file I/O. Updates internal offset state.
    ///
    /// # Truncation Detection
    /// If file size < stored offset, file was truncated/rotated.
    /// Resets offset to 0 and re-reads entire file.
    ///
    /// # Returns
    /// - `Ok(String)` - New content since last read (may be empty if file unchanged)
    /// - `Err(io::Error)` - File I/O error
    pub fn read_new_lines(&mut self, path: &Path) -> io::Result<String> {
        let mut file = File::open(path)?;
        let current_offset = self.get_offset(path);

        // Get file size to detect truncation
        let file_len = file.metadata()?.len();

        // Truncation detected: file shrank below our offset
        let read_offset = if file_len < current_offset {
            // Reset offset and re-read from start
            self.set_offset(path.to_path_buf(), 0);
            0
        } else {
            current_offset
        };

        // Seek to determined position
        file.seek(SeekFrom::Start(read_offset))?;

        // Read from position to end
        let mut new_content = String::new();
        let bytes_read = file.read_to_string(&mut new_content)?;

        // Update offset
        if bytes_read > 0 {
            self.set_offset(path.to_path_buf(), read_offset + bytes_read as u64);
        }

        Ok(new_content)
    }

    /// Reset offset for a file to 0 (force full re-read next time)
    pub fn reset(&mut self, path: &Path) {
        self.offsets.remove(path);
    }

    /// Clear all tracked offsets
    pub fn clear(&mut self) {
        self.offsets.clear();
    }
}

impl Default for TailState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_tail_state_new() {
        let state = TailState::new();
        let path = Path::new("/tmp/test.txt");
        assert_eq!(state.get_offset(path), 0);
    }

    #[test]
    fn test_tail_state_set_get_offset() {
        let mut state = TailState::new();
        let path = PathBuf::from("/tmp/test.txt");

        state.set_offset(path.clone(), 100);
        assert_eq!(state.get_offset(&path), 100);

        state.set_offset(path.clone(), 250);
        assert_eq!(state.get_offset(&path), 250);
    }

    #[test]
    fn test_tail_state_reset() {
        let mut state = TailState::new();
        let path = PathBuf::from("/tmp/test.txt");

        state.set_offset(path.clone(), 100);
        assert_eq!(state.get_offset(&path), 100);

        state.reset(&path);
        assert_eq!(state.get_offset(&path), 0);
    }

    #[test]
    fn test_tail_state_clear() {
        let mut state = TailState::new();
        let path1 = PathBuf::from("/tmp/test1.txt");
        let path2 = PathBuf::from("/tmp/test2.txt");

        state.set_offset(path1.clone(), 100);
        state.set_offset(path2.clone(), 200);

        state.clear();

        assert_eq!(state.get_offset(&path1), 0);
        assert_eq!(state.get_offset(&path2), 0);
    }

    #[test]
    fn test_read_new_lines_initial_read() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Line 1").unwrap();
        writeln!(file, "Line 2").unwrap();
        writeln!(file, "Line 3").unwrap();

        let mut state = TailState::new();
        let content = state.read_new_lines(file.path()).unwrap();

        assert_eq!(content, "Line 1\nLine 2\nLine 3\n");
        assert!(state.get_offset(file.path()) > 0);
    }

    #[test]
    fn test_read_new_lines_incremental() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Line 1").unwrap();

        let mut state = TailState::new();

        // First read
        let content1 = state.read_new_lines(file.path()).unwrap();
        assert_eq!(content1, "Line 1\n");

        // Append more lines
        writeln!(file, "Line 2").unwrap();
        writeln!(file, "Line 3").unwrap();

        // Second read - should only get new lines
        let content2 = state.read_new_lines(file.path()).unwrap();
        assert_eq!(content2, "Line 2\nLine 3\n");
    }

    #[test]
    fn test_read_new_lines_no_new_content() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Line 1").unwrap();

        let mut state = TailState::new();

        // First read
        state.read_new_lines(file.path()).unwrap();

        // Second read without new content
        let content2 = state.read_new_lines(file.path()).unwrap();
        assert_eq!(content2, "");
    }

    #[test]
    fn test_read_new_lines_multiple_files() {
        let mut file1 = NamedTempFile::new().unwrap();
        let mut file2 = NamedTempFile::new().unwrap();

        writeln!(file1, "File 1 Line 1").unwrap();
        writeln!(file2, "File 2 Line 1").unwrap();

        let mut state = TailState::new();

        let content1 = state.read_new_lines(file1.path()).unwrap();
        let content2 = state.read_new_lines(file2.path()).unwrap();

        assert_eq!(content1, "File 1 Line 1\n");
        assert_eq!(content2, "File 2 Line 1\n");

        // Append to file1 only
        writeln!(file1, "File 1 Line 2").unwrap();

        // Read both again
        let content1_new = state.read_new_lines(file1.path()).unwrap();
        let content2_new = state.read_new_lines(file2.path()).unwrap();

        assert_eq!(content1_new, "File 1 Line 2\n");
        assert_eq!(content2_new, ""); // No new content in file2
    }

    #[test]
    fn test_read_new_lines_nonexistent_file() {
        let mut state = TailState::new();
        let result = state.read_new_lines(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_new_lines_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let mut state = TailState::new();

        let content = state.read_new_lines(file.path()).unwrap();
        assert_eq!(content, "");
        assert_eq!(state.get_offset(file.path()), 0);
    }

    #[test]
    fn test_tail_state_reset_then_read() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Line 1").unwrap();

        let mut state = TailState::new();

        // First read
        state.read_new_lines(file.path()).unwrap();

        // Append more
        writeln!(file, "Line 2").unwrap();

        // Reset and re-read - should get all content
        state.reset(file.path());
        let content = state.read_new_lines(file.path()).unwrap();
        assert_eq!(content, "Line 1\nLine 2\n");
    }

    #[test]
    fn test_truncation_detection_resets_offset() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Line 1").unwrap();
        writeln!(file, "Line 2").unwrap();
        writeln!(file, "Line 3").unwrap();

        let mut state = TailState::new();

        // First read - consumes all content
        let content1 = state.read_new_lines(file.path()).unwrap();
        assert_eq!(content1, "Line 1\nLine 2\nLine 3\n");
        let offset_after_first = state.get_offset(file.path());
        assert!(offset_after_first > 0);

        // Truncate file to shorter content
        let path = file.path().to_path_buf();
        drop(file); // Close original file
        let mut file = File::create(&path).unwrap();
        writeln!(file, "New line").unwrap();
        drop(file);

        // Next read should detect truncation, reset offset, re-read from start
        let content2 = state.read_new_lines(&path).unwrap();
        assert_eq!(content2, "New line\n");

        // Offset should be set to length of new content, not old offset
        let new_offset = state.get_offset(&path);
        assert!(new_offset < offset_after_first);
        assert_eq!(new_offset, "New line\n".len() as u64);
    }

    #[test]
    fn test_truncation_with_replacement_content() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Original line 1").unwrap();
        writeln!(file, "Original line 2").unwrap();

        let mut state = TailState::new();

        // Read original content
        state.read_new_lines(file.path()).unwrap();
        let original_offset = state.get_offset(file.path());

        // Replace file with different, shorter content
        let path = file.path().to_path_buf();
        drop(file);
        let mut file = File::create(&path).unwrap();
        writeln!(file, "Short").unwrap();
        drop(file);

        // Should detect truncation via size comparison
        let content = state.read_new_lines(&path).unwrap();
        assert_eq!(content, "Short\n");

        let new_offset = state.get_offset(&path);
        assert!(new_offset < original_offset, "Offset should decrease after truncation");
    }

    #[test]
    fn test_normal_append_after_truncation_detection() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Line 1").unwrap();

        let mut state = TailState::new();

        // Initial read
        state.read_new_lines(file.path()).unwrap();

        // Truncate
        let path = file.path().to_path_buf();
        drop(file);
        let mut file = File::create(&path).unwrap();
        writeln!(file, "New").unwrap();

        // Read after truncation
        state.read_new_lines(&path).unwrap();

        // Append normally
        writeln!(file, "Appended").unwrap();
        drop(file);

        // Should read only appended content
        let content = state.read_new_lines(&path).unwrap();
        assert_eq!(content, "Appended\n");
    }
}

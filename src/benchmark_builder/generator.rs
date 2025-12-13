//! File generation logic for incremental build

use super::templates::{get_template, get_templates_through};
use std::fs;
use std::path::{Path, PathBuf};

/// Generates all files up to and including the specified step
pub fn generate_through_step(output_dir: &Path, step: usize) -> std::io::Result<Vec<PathBuf>> {
    let templates = get_templates_through(step);
    let mut created = Vec::new();

    for template in templates {
        let file_path = output_dir.join(template.path);

        // Create parent directories
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write file
        fs::write(&file_path, template.content)?;
        created.push(file_path);
    }

    Ok(created)
}

/// Generates only the file for a specific step (assumes previous steps already exist)
pub fn generate_step(output_dir: &Path, step: usize) -> std::io::Result<Option<PathBuf>> {
    let template = match get_template(step) {
        Some(t) => t,
        None => return Ok(None),
    };

    let file_path = output_dir.join(template.path);

    // Create parent directories
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write file
    fs::write(&file_path, template.content)?;

    Ok(Some(file_path))
}

/// Cleans up the output directory
pub fn cleanup(output_dir: &Path) -> std::io::Result<()> {
    if output_dir.exists() {
        fs::remove_dir_all(output_dir)?;
    }
    Ok(())
}

/// Returns the file path for a given step
pub fn get_step_file_path(output_dir: &Path, step: usize) -> Option<PathBuf> {
    get_template(step).map(|t| output_dir.join(t.path))
}

/// Returns the relative path for a given step
pub fn get_step_relative_path(step: usize) -> Option<&'static str> {
    get_template(step).map(|t| t.path)
}

/// Returns the purpose description for a given step
pub fn get_step_purpose(step: usize) -> Option<&'static str> {
    get_template(step).map(|t| t.purpose)
}

/// Counts the total number of files that would exist at a given step
pub fn count_files_at_step(step: usize) -> usize {
    get_templates_through(step).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_step() {
        let temp_dir = TempDir::new().unwrap();
        let output_dir = temp_dir.path();

        // Generate step 1
        let result = generate_step(output_dir, 1).unwrap();
        assert!(result.is_some());

        let file_path = result.unwrap();
        assert!(file_path.exists());
        assert!(file_path.to_string_lossy().contains("common.ts"));
    }

    #[test]
    fn test_generate_through_step() {
        let temp_dir = TempDir::new().unwrap();
        let output_dir = temp_dir.path();

        // Generate through step 5
        let files = generate_through_step(output_dir, 5).unwrap();
        assert_eq!(files.len(), 5);

        // All files should exist
        for file in &files {
            assert!(file.exists());
        }
    }

    #[test]
    fn test_count_files() {
        assert_eq!(count_files_at_step(1), 1);
        assert_eq!(count_files_at_step(10), 10);
        assert_eq!(count_files_at_step(65), 65);
    }
}

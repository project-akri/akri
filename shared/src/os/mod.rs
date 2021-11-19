pub mod env_var;

/// Provide file operations
pub mod file {
    use std::fs;
    use std::path::PathBuf;
    /// This will convert a relative path into the canonical path using std::fs
    pub fn get_canonical_path(relative_path: &str) -> String {
        fs::canonicalize(PathBuf::from(&relative_path))
            .unwrap_or_else(|_| panic!("unable to read file: {}", &relative_path))
            .to_str()
            .expect("unable to convert PathBuf to &str")
            .to_string()
    }
    /// This will read a file (as provided by a relative path) into a String
    pub fn read_file_to_string(relative_path: &str) -> String {
        let file_path = get_canonical_path(relative_path);
        fs::read_to_string(&file_path)
            .unwrap_or_else(|_| panic!("unable to read file: {}", &file_path))
    }
}

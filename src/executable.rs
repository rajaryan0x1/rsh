use std::env;
use std::fs::metadata;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub(crate) fn find_exe(cmd: &str) -> Option<PathBuf> {
    let path = env::var("PATH").unwrap_or_default();

    for dir in path.split(':') {
        let candidate = PathBuf::from(dir).join(cmd);

        if let Ok(metadata) = metadata(&candidate) {
            if metadata.is_file() && (metadata.permissions().mode() & 0o111 != 0) {
                return Some(candidate);
            }
        }
    }

    None
}

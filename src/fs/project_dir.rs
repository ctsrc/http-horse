//! Scan project dir

use crate::fs::exclude::EXCLUDE_FILES_BY_NAME;
use futures_util::future::join_all;
use smol::fs::{read_dir, File};
use smol::stream::StreamExt;
use std::fmt::Debug;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use thiserror::Error;
use tracing::{debug, info};
use trie_hard::TrieHard;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O: {0}")]
    IO(#[from] smol::io::Error),
    #[error("Exclusion rules not initialized")]
    ExcludeRulesNotInitialized,
    #[error("A full re-scan of the project directory was attempted")]
    FullRescanOfProjectDirWasAttempted,
}

static I_HAVE_ALREADY_BEEN_RUN: OnceLock<bool> = OnceLock::new();

/// Call this function once, at program startup.
///
/// Subsequent calls to this function should not be made. For staying up to date
/// with file system changes, file system event monitoring should be used.
pub async fn scan_project_dir(project_dir: PathBuf) -> Result<TrackedProjectDir, Error> {
    let exclude = EXCLUDE_FILES_BY_NAME
        .get()
        .ok_or(Error::ExcludeRulesNotInitialized)?;

    // HEED THE RULES, OR SUFFER THE CONSEQUENCES!
    I_HAVE_ALREADY_BEEN_RUN
        .set(true)
        .map_err(|_| Error::FullRescanOfProjectDirWasAttempted)?;

    scan_dir(project_dir, exclude).await
}

/// A regular file that we are tracking updates and changes for,
/// from the project directory tree.
#[derive(Debug)]
pub struct TrackedProjectFile {
    /// Absolute path to file.
    pub fpath: PathBuf,
    /// Open file handle.
    pub file: File,
}

/// A directory that we are tracking updates and changes for,
/// from the project directory tree.
#[derive(Debug)]
pub struct TrackedProjectDir {
    /// Regular files in this directory.
    pub tracked_files: Vec<TrackedProjectFile>,
    /// Subdirectories in this directory.
    pub tracked_dirs: Vec<TrackedProjectDir>,
}

async fn scan_dir(
    dpath: PathBuf,
    exclude: &TrieHard<'static, &str>,
) -> Result<TrackedProjectDir, Error> {
    info!(?dpath, "Scanning directory");

    let mut read_dir = read_dir(&dpath).await?;

    let mut tracked_files = vec![];
    //let mut dirs = vec![];

    let mut subdir_futs = vec![];

    while let Some(dir_entry) = read_dir.try_next().await? {
        let file_name = dir_entry.file_name();
        debug!(?file_name, ?dpath, "A dir entry was read from directory.");
        if let Some(matched) = exclude.get(dir_entry.file_name().as_bytes()) {
            info!(
                file_name = matched,
                ?dpath,
                "Skipping file based on exclusion rules."
            );
            continue;
        }

        // Symlinks are actually super useful, but because we want http-horse
        // to never serve files from outside the project directory, it is
        // convenient for http-horse to simply skip all symlinks for now.
        // Even when they point to something else within the project directory.
        // Consider raising an issue about this in our GitHub repo if your
        // use-case for http-horse makes use of symlinks.
        //
        // In the future we may loosen this up so that symlinks pointing to
        // something else within the project directory will be accepted,
        // and properly treated. In that case we will have to keep track
        // of which files are symlinks and when FS events affect files
        // that are linked *to*, we will emit an update event for
        // any symlinks pointing to that file.
        //
        // Further down the line after that, we may wish to loosen this up
        // even further, so that if you symlink to something that is outside
        // the project directory, but inside a git repo of which the project directory
        // exists (and regardless of whether the project directory is tracked or git ignored),
        // we would then allow, and watch, those too. Although that might be one step too far.
        //
        // Or, if not going as far as to allowing everything in the parent git repo to be linked to,
        // we could allow symlinks that point to files in the "source directory" of the project,
        // as indicated by the command line arguments provided to http-horse.
        //
        // TODO: ^
        let file_type = dir_entry.file_type().await?;
        if file_type.is_symlink() {
            info!(?file_name, ?dpath, "Skipping file because it is a symlink.");
            continue;
        } else if file_type.is_dir() {
            let mut child_dpath = dpath.clone();
            child_dpath.push(file_name);
            subdir_futs.push(scan_dir(child_dpath, exclude));
        } else if file_type.is_file() {
            let mut fpath = dpath.clone();
            fpath.push(file_name);
            let file = File::open(&fpath).await?;
            let tracked_file = TrackedProjectFile { fpath, file };
            tracked_files.push(tracked_file);
        } else {
            unreachable!("The only three kinds of file type we know of is directory, symlink and regular file.");
        }
    }

    let res: Result<Vec<_>, _> = join_all(subdir_futs).await.into_iter().collect();
    let tracked_dirs = res?;

    let tracked_dir = TrackedProjectDir {
        tracked_files,
        tracked_dirs,
    };

    Ok(tracked_dir)
}

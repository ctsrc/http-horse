use std::sync::OnceLock;
use trie_hard::TrieHard;

pub static EXCLUDE_FILES_BY_NAME: OnceLock<TrieHard<'static, &'static str>> = OnceLock::new();

/// Files and directories to be excluded based on file names.
///
/// These include metadata files of no interest, as well as files which may leak sensitive information.
pub fn exclude() -> TrieHard<'static, &'static str> {
    [
        // .DS_Store meta files created by macOS are of no interest do us. We don't want to serve those.
        ".DS_Store",
        // If a .git directory is encountered, it is most likely because someone is serving
        // directly from the root of a git repo, or from a directory that contains one or more
        // git repos.
        //
        // In order to avoid having users accidentally leak git history which could contain
        // sensitive information, we skip any file or directory named .git
        //
        // If the user really wants to serve .git directories, they should do so using
        // another tool, rather than using http-horse for that.
        //
        // Of course, this simple name check will not protect you in the case of bare git repos.
        // It is not meant as a bulletproof solution, but rather as a quick, simplistic protection
        // against one particular kind of situation involving git repo history inside the served
        // directory tree.
        ".git",
        // .htaccess files are intended for web servers, not to be served to clients.
        // We skip any .htaccess files encountered, as they may contain sensitive information.
        ".htaccess",
        // .gitignore files are for .git, no point in serving those.
        ".gitignore",
    ]
    .into_iter()
    .collect::<TrieHard<'_, _>>()
}

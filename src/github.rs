pub fn repo_archive_download_link(owner: &str, repo: &str, git_ref: &str) -> String {
    format!(
        "https://api.github.com/repos/{}/{}/tarball/{}",
        owner, repo, git_ref
    )
}

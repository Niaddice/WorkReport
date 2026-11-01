use chrono::{Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

const MAX_COMMITS: usize = 1000;
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;
pub const DEFAULT_COMMIT_PROMPT: &str = "根据暂存区的更改生成简洁准确的中文 Git 提交说明。首行使用 Conventional Commits 格式（如 feat:、fix:、refactor:），概括主要改动；必要时空一行后列出关键变化。只描述实际更改，不编造测试结果或工作成果，只输出提交说明正文。";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub executable: String,
    pub repositories: Vec<Repository>,
    pub include_merges: bool,
    pub commit_prompt: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            executable: "git".into(),
            repositories: Vec::new(),
            include_merges: false,
            commit_prompt: DEFAULT_COMMIT_PROMPT.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub path: String,
    pub author: String,
    pub branch: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub committed_at: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryLog {
    pub id: String,
    pub name: String,
    pub commits: Vec<Commit>,
    pub truncated: bool,
    pub error: Option<String>,
    pub duplicate_commits: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyLog {
    pub date: String,
    pub start_date: String,
    pub end_date: String,
    pub repositories: Vec<RepositoryLog>,
}

pub fn validate(settings: &Settings) -> Result<(), String> {
    if settings.commit_prompt.chars().count() > 2000 || settings.commit_prompt.contains('\0') {
        return Err("Git 提交说明提示词不能超过 2000 字，且不能包含空字符".into());
    }
    let executable = settings.executable.trim();
    if executable.is_empty()
        || executable.chars().any(char::is_control)
        || (executable != "git" && !Path::new(executable).is_absolute())
    {
        return Err("Git 程序请填写 git 或可执行文件的绝对路径".into());
    }
    if settings.repositories.len() > 20 {
        return Err("Git 仓库最多配置 20 个".into());
    }
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for repository in &settings.repositories {
        if repository.id.trim().is_empty() || !ids.insert(&repository.id) {
            return Err("Git 仓库标识无效或重复，请删除后重新添加".into());
        }
        if repository.name.trim().is_empty() || repository.name.chars().count() > 100 {
            return Err("请填写 Git 仓库名称，最多 100 字".into());
        }
        if !Path::new(repository.path.trim()).is_absolute()
            || repository.path.chars().any(char::is_control)
        {
            return Err(format!("{}：请填写仓库目录的绝对路径", repository.name));
        }
        if !paths.insert(repository.path.trim().trim_end_matches(['/', '\\'])) {
            return Err("同一 Git 仓库目录不能重复添加".into());
        }
        if repository.author.len() > 500
            || repository.branch.len() > 500
            || repository.author.chars().any(char::is_control)
            || repository.branch.chars().any(char::is_control)
            || repository.branch.trim().starts_with('-')
        {
            return Err(format!("{}：作者或分支配置无效", repository.name));
        }
    }
    Ok(())
}

fn day_bounds(date: &str) -> Result<(i64, i64), String> {
    if date.len() != 8 || !date.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("请选择有效日期（YYYYMMDD）".into());
    }
    let day = NaiveDate::parse_from_str(date, "%Y%m%d").map_err(|_| "请选择有效日期")?;
    let next = day.succ_opt().ok_or("日期超出支持范围")?;
    let start = Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .ok_or("无法确定所选日期的本地时间")?;
    let end = Local
        .from_local_datetime(&next.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .ok_or("无法确定所选日期的本地时间")?;
    Ok((start.timestamp(), end.timestamp()))
}

/// The end date is inclusive. Resolve each midnight in local time so ranges
/// spanning daylight-saving changes do not assume every day has 24 hours.
fn range_bounds(start_date: &str, end_date: &str) -> Result<(i64, i64), String> {
    let (start, _) = day_bounds(start_date)?;
    let (_, end) = day_bounds(end_date)?;
    if start_date > end_date {
        return Err("开始日期不能晚于结束日期".into());
    }
    Ok((start, end))
}

async fn read_limited(stream: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    stream
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("读取 Git 输出失败：{error}"))?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err("Git 输出超过 4 MB，请缩小日期、作者或分支范围后重试".into());
    }
    Ok(bytes)
}

pub(super) async fn run(
    settings: &Settings,
    path: &Path,
    args: &[String],
) -> Result<std::process::Output, String> {
    let mut command = Command::new(settings.executable.trim());
    command
        .current_dir(path)
        .args(["--no-pager", "--no-optional-locks"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .kill_on_drop(true);
    // GUI apps may inherit a minimal PATH. Preserve the user's search order.
    let mut paths: Vec<_> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    paths.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(Into::into));
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(key);
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 Git，请检查 Git 程序和仓库目录：{error}"))?;
    let stdout = child.stdout.take().ok_or("无法读取 Git 输出")?;
    let stderr = child.stderr.take().ok_or("无法读取 Git 错误信息")?;
    let (stdout, stderr, status) = timeout(Duration::from_secs(30), async {
        tokio::try_join!(read_limited(stdout), read_limited(stderr), async {
            child
                .wait()
                .await
                .map_err(|error| format!("Git 运行失败：{error}"))
        })
    })
    .await
    .map_err(|_| "读取 Git 超时，请检查仓库后重试".to_string())??;
    Ok(std::process::Output {
        stdout,
        stderr,
        status,
    })
}

fn checked(output: std::process::Output) -> Result<Vec<u8>, String> {
    if !output.status.success() {
        let detail: String = String::from_utf8_lossy(&output.stderr)
            .trim()
            .chars()
            .take(1000)
            .collect();
        return Err(format!("读取 Git 提交失败：{detail}"));
    }
    Ok(output.stdout)
}

fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[derive(Debug)]
struct RepositoryIdentity {
    common_directory: PathBuf,
    remote: Option<String>,
}

impl RepositoryIdentity {
    fn same_repository(&self, other: &Self) -> bool {
        self.common_directory == other.common_directory
            || self
                .remote
                .as_ref()
                .zip(other.remote.as_ref())
                .is_some_and(|(left, right)| left == right)
    }
}

/// Compare common SSH/HTTPS URL forms without URL userinfo.
/// Keep repository path case, custom ports and query parameters: they may identify different repositories.
fn remote_identity(value: &str, directory: &Path) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.contains("://") {
        let url = reqwest::Url::parse(value).ok()?;
        if url.scheme() == "file" {
            let path = std::fs::canonicalize(url.to_file_path().ok()?).ok()?;
            return Some(format!("file:{}", path.to_string_lossy()));
        }
        if !["https", "http", "ssh", "git", "git+ssh"].contains(&url.scheme()) {
            return None;
        }
        let host = url.host_str()?.to_ascii_lowercase();
        let port = match (url.scheme(), url.port()) {
            ("ssh" | "git+ssh", Some(22)) | ("git", Some(9418)) => String::new(),
            (_, Some(port)) => format!(":{port}"),
            _ => String::new(),
        };
        let path = url.path().trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        if path.is_empty() {
            return None;
        }
        let query = url
            .query()
            .map(|query| format!("?{query}"))
            .unwrap_or_default();
        return Some(format!("remote:{host}{port}/{path}{query}"));
    }
    // SCP-style SSH addresses, e.g. git@example.com:team/project.git.
    if !Path::new(value).is_absolute() {
        if let Some((host, path)) = value.split_once(':') {
            if !host.contains(['/', '\\']) && !host.is_empty() {
                let host = host.rsplit('@').next()?.to_ascii_lowercase();
                let path = path.trim_matches('/');
                let path = path.strip_suffix(".git").unwrap_or(path);
                if !host.is_empty() && !path.is_empty() {
                    return Some(format!("remote:{host}/{path}"));
                }
            }
        }
    }
    // Local file remotes may be relative to the working tree.
    let path = std::fs::canonicalize(directory.join(value)).ok()?;
    Some(format!("file:{}", path.to_string_lossy()))
}

async fn repository_identity(
    settings: &Settings,
    repository: &Repository,
) -> Result<RepositoryIdentity, String> {
    let path = Path::new(repository.path.trim());
    let common = checked(
        run(
            settings,
            path,
            &arguments(&["rev-parse", "--path-format=absolute", "--git-common-dir"]),
        )
        .await?,
    )?;
    let common = String::from_utf8(common).map_err(|_| "Git 仓库目录信息无效")?;
    let common_directory =
        std::fs::canonicalize(common.trim()).map_err(|_| "无法确定 Git 仓库的实际目录")?;
    let remotes = checked(run(settings, path, &arguments(&["remote"])).await?)?;
    let remotes = String::from_utf8(remotes).map_err(|_| "Git 远程仓库信息无效")?;
    let names: Vec<_> = remotes.lines().filter(|name| !name.is_empty()).collect();
    // Shared upstream remotes do not make two independent forks the same project.
    let primary = names
        .iter()
        .find(|name| **name == "origin")
        .copied()
        .or_else(|| (names.len() == 1).then(|| names[0]));
    let remote = if let Some(name) = primary {
        let url = checked(run(settings, path, &arguments(&["remote", "get-url", name])).await?)?;
        remote_identity(&String::from_utf8_lossy(&url), path)
    } else {
        None
    };
    Ok(RepositoryIdentity {
        common_directory,
        remote,
    })
}

/// Called before committing changed Git settings; inspection only reads local metadata.
pub async fn validate_repositories(settings: &Settings) -> Result<(), String> {
    validate(settings)?;
    let mut identities: Vec<(RepositoryIdentity, &str)> = Vec::new();
    for repository in &settings.repositories {
        let identity = repository_identity(settings, repository)
            .await
            .map_err(|_| {
                format!(
                    "无法检查仓库“{}”，请确认目录是有效的 Git 仓库且 Git 程序可用",
                    repository.name
                )
            })?;
        if let Some((_, existing)) = identities
            .iter()
            .find(|(existing, _)| identity.same_repository(existing))
        {
            return Err(format!(
                "仓库“{}”与“{}”指向同一 Git 仓库，不能重复添加，请只保留一个",
                repository.name, existing
            ));
        }
        identities.push((identity, &repository.name));
    }
    Ok(())
}

fn parse_log(bytes: &[u8], start: i64, end: i64) -> Result<(Vec<Commit>, bool), String> {
    let text = String::from_utf8_lossy(bytes);
    let mut fields: Vec<_> = text.split('\0').collect();
    if fields.last().is_some_and(|field| field.trim().is_empty()) {
        fields.pop();
    }
    if fields.len() % 6 != 0 {
        return Err("Git 提交格式无效，请检查仓库".into());
    }
    let mut commits = Vec::new();
    for record in fields.chunks_exact(6) {
        let timestamp: i64 = record[4].parse().map_err(|_| "Git 提交时间无效")?;
        if timestamp < start || timestamp >= end {
            continue;
        }
        commits.push(Commit {
            hash: record[0].trim().into(),
            author: record[1].into(),
            email: record[2].into(),
            committed_at: record[3].into(),
            message: record[5].trim().into(),
        });
    }
    let truncated = commits.len() > MAX_COMMITS;
    commits.truncate(MAX_COMMITS);
    Ok((commits, truncated))
}

async fn repository_log(
    settings: &Settings,
    repository: &Repository,
    start: i64,
    end: i64,
) -> Result<(Vec<Commit>, bool), String> {
    let path = Path::new(repository.path.trim());
    let branch = repository.branch.trim();
    let revision = format!(
        "{}^{{commit}}",
        if branch.is_empty() { "HEAD" } else { branch }
    );
    let resolved = run(
        settings,
        path,
        &arguments(&["rev-parse", "--verify", "--end-of-options", &revision]),
    )
    .await?;
    if !resolved.status.success() && branch.is_empty() {
        // An initialized repository without its first commit is an empty day.
        let head = run(
            settings,
            path,
            &arguments(&["symbolic-ref", "--quiet", "HEAD"]),
        )
        .await?;
        if head.status.success() {
            let reference = String::from_utf8_lossy(&head.stdout).trim().to_owned();
            let exists = run(
                settings,
                path,
                &arguments(&["show-ref", "--verify", "--quiet", &reference]),
            )
            .await?;
            if exists.status.code() == Some(1) {
                return Ok((Vec::new(), false));
            }
        }
    }
    let hash = String::from_utf8(checked(resolved)?).map_err(|_| "Git 分支信息无效")?;
    let mut args = arguments(&[
        "log",
        "--no-color",
        "--no-decorate",
        "--no-notes",
        "--no-patch",
        "--no-show-signature",
        "--encoding=UTF-8",
        "--format=%H%x00%an%x00%ae%x00%cI%x00%ct%x00%B%x00",
    ]);
    // since-as-filter traverses past out-of-order commit dates instead of pruning history.
    args.push(format!("--since-as-filter=@{}", start - 1));
    args.push(format!("--until=@{}", end - 1));
    args.push(format!("--max-count={}", MAX_COMMITS + 1));
    if !settings.include_merges {
        args.push("--no-merges".into());
    }
    if !repository.author.trim().is_empty() {
        args.extend(arguments(&["--fixed-strings", "--regexp-ignore-case"]));
        args.push(format!("--author={}", repository.author.trim()));
    }
    args.push(hash.trim().into());
    args.push("--".into());
    parse_log(&checked(run(settings, path, &args).await?)?, start, end)
}

pub async fn daily_log(settings: Settings, date: String) -> Result<DailyLog, String> {
    range_log(settings, date.clone(), date).await
}

pub async fn range_log(
    settings: Settings,
    start_date: String,
    end_date: String,
) -> Result<DailyLog, String> {
    validate(&settings)?;
    let (start, end) = range_bounds(&start_date, &end_date)?;
    if settings.repositories.is_empty() {
        return Err("请先在设置 → Git 配置中添加本地仓库".into());
    }
    let mut repositories = Vec::new();
    let mut seen: Vec<(RepositoryIdentity, HashSet<String>)> = Vec::new();
    for repository in &settings.repositories {
        let mut log = RepositoryLog {
            id: repository.id.clone(),
            name: repository.name.clone(),
            commits: Vec::new(),
            truncated: false,
            error: None,
            duplicate_commits: 0,
        };
        let data = async {
            let identity = repository_identity(&settings, repository).await?;
            let (commits, truncated) = repository_log(&settings, repository, start, end).await?;
            Ok::<_, String>((identity, commits, truncated))
        }
        .await;
        match data {
            Ok((identity, mut commits, truncated)) => {
                // Also defend against old settings or remote URLs changed outside the app.
                let index = seen
                    .iter()
                    .position(|(existing, _)| identity.same_repository(existing))
                    .unwrap_or_else(|| {
                        seen.push((identity, HashSet::new()));
                        seen.len() - 1
                    });
                let count = commits.len();
                commits.retain(|commit| seen[index].1.insert(commit.hash.clone()));
                log.duplicate_commits = count - commits.len();
                log.commits = commits;
                log.truncated = truncated;
            }
            Err(error) => log.error = Some(error),
        }
        repositories.push(log);
    }
    Ok(DailyLog {
        date: start_date.clone(),
        start_date,
        end_date,
        repositories,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(path: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(path)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn setup(path: &Path) -> Settings {
        git(path, &["init", "-b", "main"]);
        git(path, &["config", "user.name", "测试作者"]);
        git(path, &["config", "user.email", "author@example.test"]);
        Settings {
            repositories: vec![Repository {
                id: "repo".into(),
                name: "日报项目".into(),
                path: path.to_string_lossy().into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn commit(path: &Path, timestamp: i64, message: &str, email: &str) {
        let output = std::process::Command::new("git")
            .current_dir(path)
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "--allow-empty",
                "-m",
                message,
            ])
            .env("GIT_AUTHOR_DATE", format!("{timestamp} +0000"))
            .env("GIT_COMMITTER_DATE", format!("{timestamp} +0000"))
            .env("GIT_AUTHOR_EMAIL", email)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn ranges_include_both_boundary_days_and_preserve_author_filter_across_year_and_leap_day() {
        for (first, middle, last) in [
            ("20261231", "20270101", "20270102"),
            ("20240228", "20240229", "20240301"),
        ] {
            let root = tempfile::tempdir().unwrap();
            let mut settings = setup(root.path());
            let (start, end) = range_bounds(first, last).unwrap();
            commit(root.path(), start - 1, "范围之前", "author@example.test");
            commit(root.path(), start, "开始日零点", "author@example.test");
            commit(
                root.path(),
                day_bounds(middle).unwrap().0,
                "中间日期",
                "author@example.test",
            );
            commit(root.path(), end - 1, "结束日最后一秒", "author@example.test");
            commit(root.path(), end, "范围之后", "author@example.test");
            // Out-of-order commit times and other authors must retain the same
            // filtering semantics as the existing single-day report.
            commit(root.path(), start + 100, "其他作者", "other@example.test");
            settings.repositories[0].author = "AUTHOR@EXAMPLE.TEST".into();
            let log = range_log(settings, first.into(), last.into()).await.unwrap();
            assert_eq!(log.date, first);
            assert_eq!(log.start_date, first);
            assert_eq!(log.end_date, last);
            let repository = &log.repositories[0];
            assert!(repository.error.is_none(), "{:?}", repository.error);
            let messages: HashSet<_> = repository.commits.iter()
                .map(|commit| commit.message.as_str())
                .collect();
            assert_eq!(messages, HashSet::from(["开始日零点", "中间日期", "结束日最后一秒"]));
            let serialized = serde_json::to_value(&log).unwrap();
            assert_eq!(serialized["startDate"], first);
            assert_eq!(serialized["endDate"], last);
        }
    }

    #[tokio::test]
    async fn range_with_identical_dates_matches_the_existing_single_day_query() {
        let root = tempfile::tempdir().unwrap();
        let settings = setup(root.path());
        let (start, end) = day_bounds("20261028").unwrap();
        commit(root.path(), start - 1, "之前", "author@example.test");
        commit(root.path(), start, "当天工作", "author@example.test");
        commit(root.path(), end, "之后", "author@example.test");
        let single = daily_log(settings.clone(), "20261028".into()).await.unwrap();
        let range = range_log(settings, "20261028".into(), "20261028".into()).await.unwrap();
        assert_eq!(serde_json::to_value(single).unwrap(), serde_json::to_value(range).unwrap());
    }

    #[tokio::test]
    async fn rejects_reversed_and_invalid_ranges_before_reading_repositories() {
        for (start, end) in [
            ("20261029", "20261028"),
            ("20260230", "20260301"),
            ("20261028", "20261301"),
            ("2026-10-28", "20261029"),
            ("20261028", ""),
            ("20261028", "100000101"),
        ] {
            assert!(range_log(Settings::default(), start.into(), end.into()).await.is_err());
        }
        assert!(range_bounds("20261029", "20261028").unwrap_err().contains("开始日期"));
        assert_eq!(range_bounds("20261028", "20261028").unwrap(), day_bounds("20261028").unwrap());
    }

    #[tokio::test]
    async fn filters_local_day_boundaries_and_literal_author_without_losing_chinese_bodies() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = setup(root.path());
        let (start, end) = day_bounds("20261028").unwrap();
        commit(root.path(), start - 1, "前一天", "author@example.test");
        commit(
            root.path(),
            start,
            "完成 Git 配置\n\n支持多仓库\n保留提交说明",
            "author@example.test",
        );
        commit(root.path(), end - 1, "完成模块", "author@example.test");
        commit(
            root.path(),
            start - 2,
            "日期乱序的旧提交",
            "author@example.test",
        );
        commit(root.path(), end, "次日提交", "author@example.test");
        // A newer ancestor must not hide an older commit further in history.
        commit(root.path(), start + 100, "其他作者", "other@example.test");
        settings.repositories[0].author = "AUTHOR@EXAMPLE.TEST".into();
        let log = daily_log(settings.clone(), "20261028".into())
            .await
            .unwrap();
        let commits = &log.repositories[0].commits;
        assert!(
            log.repositories[0].error.is_none(),
            "{:?}",
            log.repositories[0].error
        );
        assert_eq!(commits.len(), 2);
        assert!(commits
            .iter()
            .any(|commit| commit.message.contains("支持多仓库\n保留提交说明")));
        settings.repositories[0].author = ".*".into();
        assert!(daily_log(settings, "20261028".into())
            .await
            .unwrap()
            .repositories[0]
            .commits
            .is_empty());
    }

    #[tokio::test]
    async fn handles_empty_repositories_invalid_branches_and_partial_failures() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = setup(root.path());
        let log = daily_log(settings.clone(), "20261028".into())
            .await
            .unwrap();
        assert!(log.repositories[0].error.is_none());
        assert!(log.repositories[0].commits.is_empty());
        commit(
            root.path(),
            day_bounds("20261028").unwrap().0,
            "完成日报",
            "author@example.test",
        );
        let mut invalid = settings.repositories[0].clone();
        invalid.id = "missing".into();
        invalid.name = "无效仓库".into();
        invalid.path.push_str("/missing");
        settings.repositories.push(invalid);
        let log = daily_log(settings.clone(), "20261028".into())
            .await
            .unwrap();
        assert_eq!(log.repositories[0].commits.len(), 1);
        assert!(log.repositories[1].error.is_some());
        settings.repositories[0].branch = "does-not-exist".into();
        assert!(daily_log(settings, "20261028".into())
            .await
            .unwrap()
            .repositories[0]
            .error
            .is_some());
    }

    #[tokio::test]
    async fn respects_selected_branch_and_merge_setting() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = setup(root.path());
        let start = day_bounds("20261028").unwrap().0;
        commit(root.path(), start + 10, "基础功能", "author@example.test");
        git(root.path(), &["checkout", "-b", "feature"]);
        commit(root.path(), start + 20, "分支功能", "author@example.test");
        git(root.path(), &["checkout", "main"]);
        commit(root.path(), start + 30, "主线功能", "author@example.test");
        settings.repositories[0].branch = "feature".into();
        let log = daily_log(settings.clone(), "20261028".into())
            .await
            .unwrap();
        assert_eq!(log.repositories[0].commits.len(), 2);
        assert!(log.repositories[0]
            .commits
            .iter()
            .all(|commit| commit.message != "主线功能"));
        let output = std::process::Command::new("git")
            .current_dir(root.path())
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "merge",
                "--no-ff",
                "feature",
                "-m",
                "合并分支",
            ])
            .env("GIT_AUTHOR_DATE", format!("{} +0000", start + 40))
            .env("GIT_COMMITTER_DATE", format!("{} +0000", start + 40))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        settings.repositories[0].branch.clear();
        let log = daily_log(settings.clone(), "20261028".into())
            .await
            .unwrap();
        assert_eq!(log.repositories[0].commits.len(), 3);
        settings.include_merges = true;
        let log = daily_log(settings, "20261028".into()).await.unwrap();
        assert_eq!(log.repositories[0].commits.len(), 4);
        assert!(log.repositories[0]
            .commits
            .iter()
            .any(|commit| commit.message == "合并分支"));
    }

    #[test]
    fn reports_truncation_and_rejects_malformed_log_output() {
        let record = "abcdef\0作者\0me@example.test\0时间\010\0提交\0\n";
        let (commits, truncated) =
            parse_log(record.repeat(MAX_COMMITS + 1).as_bytes(), 10, 20).unwrap();
        assert_eq!(commits.len(), MAX_COMMITS);
        assert!(truncated);
        assert!(parse_log(b"broken\0record", 10, 20).is_err());
    }

    #[test]
    fn normalizes_common_remote_urls_without_merging_distinct_repository_paths_or_ports() {
        let path = Path::new("/tmp");
        let expected = remote_identity("https://example.com/team/project.git", path).unwrap();
        for url in [
            "https://user:private-token@EXAMPLE.COM/team/project/",
            "http://example.com:80/team/project.git",
            "ssh://git@example.com:22/team/project.git/",
            "git@example.com:team/project.git",
            "git://example.com:9418/team/project",
        ] {
            assert_eq!(
                remote_identity(url, path).as_ref(),
                Some(&expected),
                "{url}"
            );
        }
        for url in [
            "https://example.com/other/project.git",
            "https://example.com/team/Project.git",
            "https://example.com:8443/team/project.git",
            "https://another.example.com/team/project.git",
            "https://example.com/team/project.git?repository=other",
        ] {
            assert_ne!(
                remote_identity(url, path).as_ref(),
                Some(&expected),
                "{url}"
            );
        }
        assert!(!expected.contains("private-token"));
        assert!(remote_identity("", path).is_none());
        assert!(remote_identity("unsupported://example.com/project", path).is_none());
    }

    #[tokio::test]
    async fn rejects_subdirectories_worktrees_and_symlinks_of_the_same_local_repository() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = setup(root.path());
        let mut duplicate = settings.repositories[0].clone();
        duplicate.id = "duplicate".into();
        duplicate.name = "重复项目".into();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        duplicate.path = child.to_string_lossy().into();
        settings.repositories.push(duplicate);
        let error = validate_repositories(&settings).await.unwrap_err();
        assert!(
            error.contains("日报项目")
                && error.contains("重复项目")
                && error.contains("不能重复添加")
        );

        commit(
            root.path(),
            day_bounds("20261028").unwrap().0,
            "首次提交",
            "author@example.test",
        );
        let other = tempfile::tempdir().unwrap();
        let worktree = other.path().join("worktree");
        git(
            root.path(),
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "worktree",
                "add",
                "-b",
                "other",
                worktree.to_str().unwrap(),
            ],
        );
        settings.repositories[1].path = worktree.to_string_lossy().into();
        assert!(validate_repositories(&settings)
            .await
            .unwrap_err()
            .contains("不能重复添加"));
        #[cfg(unix)]
        {
            let alias = other.path().join("alias");
            std::os::unix::fs::symlink(root.path(), &alias).unwrap();
            settings.repositories[1].path = alias.to_string_lossy().into();
            assert!(validate_repositories(&settings)
                .await
                .unwrap_err()
                .contains("不能重复添加"));
        }
    }

    #[tokio::test]
    async fn rejects_two_clones_with_equivalent_origins_but_allows_forks_sharing_an_upstream() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let mut settings = setup(first.path());
        let mut another = setup(second.path()).repositories.remove(0);
        another.id = "second".into();
        another.name = "另一份克隆".into();
        settings.repositories.push(another);
        git(
            first.path(),
            &[
                "remote",
                "add",
                "origin",
                "git@example.test:team/project.git",
            ],
        );
        git(
            second.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://user:private-token@example.test/team/project",
            ],
        );
        let error = validate_repositories(&settings).await.unwrap_err();
        assert!(error.contains("同一 Git 仓库"));
        assert!(!error.contains("private-token"));
        git(
            second.path(),
            &[
                "remote",
                "set-url",
                "origin",
                "https://example.test/another/project",
            ],
        );
        for path in [first.path(), second.path()] {
            git(
                path,
                &[
                    "remote",
                    "add",
                    "upstream",
                    "https://example.test/upstream/project",
                ],
            );
        }
        assert!(validate_repositories(&settings).await.is_ok());

        git(first.path(), &["remote", "remove", "upstream"]);
        git(first.path(), &["remote", "rename", "origin", "custom"]);
        git(second.path(), &["remote", "remove", "upstream"]);
        git(
            second.path(),
            &[
                "remote",
                "set-url",
                "origin",
                "https://example.test/team/project",
            ],
        );
        assert!(validate_repositories(&settings)
            .await
            .unwrap_err()
            .contains("不能重复添加"));
    }

    #[tokio::test]
    async fn deduplicates_matching_sources_from_old_settings_without_dropping_unique_local_commits()
    {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let start = day_bounds("20261028").unwrap().0;
        let mut settings = setup(first.path());
        commit(first.path(), start + 10, "共同提交", "author@example.test");
        let clone = second.path().join("clone");
        git(
            second.path(),
            &[
                "clone",
                "--quiet",
                first.path().to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        git(&clone, &["config", "user.name", "测试作者"]);
        git(&clone, &["config", "user.email", "author@example.test"]);
        commit(
            first.path(),
            start + 20,
            "本地独有提交",
            "author@example.test",
        );
        commit(
            &clone,
            start + 30,
            "另一个来源独有提交",
            "author@example.test",
        );
        git(
            first.path(),
            &[
                "remote",
                "add",
                "origin",
                "git@example.test:team/project.git",
            ],
        );
        git(
            &clone,
            &[
                "remote",
                "set-url",
                "origin",
                "https://example.test/team/project",
            ],
        );
        settings.repositories.push(Repository {
            id: "clone".into(),
            name: "克隆仓库".into(),
            path: clone.to_string_lossy().into(),
            ..Default::default()
        });
        let log = daily_log(settings.clone(), "20261028".into())
            .await
            .unwrap();
        assert!(log
            .repositories
            .iter()
            .all(|repository| repository.error.is_none()));
        assert_eq!(log.repositories[0].commits.len(), 2);
        assert_eq!(log.repositories[1].commits.len(), 1);
        assert_eq!(log.repositories[1].commits[0].message, "另一个来源独有提交");
        assert_eq!(log.repositories[1].duplicate_commits, 1);
        // Identical history alone does not make an independent fork a duplicate project.
        git(
            &clone,
            &[
                "remote",
                "set-url",
                "origin",
                "https://example.test/fork/project",
            ],
        );
        let log = daily_log(settings, "20261028".into()).await.unwrap();
        assert_eq!(log.repositories[1].commits.len(), 2);
        assert_eq!(log.repositories[1].duplicate_commits, 0);
    }

    #[tokio::test]
    async fn rejects_uninspectable_configuration_without_exposing_remote_credentials() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = setup(root.path());
        settings.repositories[0].path.push_str("/missing");
        assert!(validate_repositories(&settings)
            .await
            .unwrap_err()
            .contains("无法检查仓库“日报项目”"));
    }

    #[test]
    fn rejects_invalid_dates_and_option_injection_and_round_trips_configuration() {
        assert!(day_bounds("20260230").is_err());
        assert!(day_bounds("2026-10-28").is_err());
        let root = tempfile::tempdir().unwrap();
        let mut settings = setup(root.path());
        settings.repositories[0].branch = "--all".into();
        assert!(validate(&settings).is_err());
        settings.repositories[0].branch = "main".into();
        assert_eq!(
            serde_json::from_str::<Settings>(&serde_json::to_string(&settings).unwrap()).unwrap(),
            settings
        );
        assert_eq!(
            serde_json::from_str::<Settings>("{}").unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn old_settings_gain_a_commit_prompt_and_invalid_prompts_are_rejected() {
        let mut settings: Settings = serde_json::from_str(r#"{"executable":"git","repositories":[],"includeMerges":false}"#).unwrap();
        assert_eq!(settings.commit_prompt, DEFAULT_COMMIT_PROMPT);
        settings.commit_prompt = "字".repeat(2001);
        assert!(validate(&settings).is_err());
        settings.commit_prompt.clear();
        assert!(validate(&settings).is_ok());
    }
}

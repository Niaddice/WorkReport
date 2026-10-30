//! A small, shell-free Git client for configured working trees.
use crate::git::{self, Settings};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokio::io::AsyncReadExt;

const PAGE_SIZE: usize = 100;
const DIFF_LIMIT: usize = 512 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub conflicted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub remote: bool,
    pub current: bool,
    pub upstream: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    pub reference: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub root: String,
    pub branch: String,
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub files: Vec<FileStatus>,
    pub branches: Vec<Branch>,
    pub stashes: Vec<Stash>,
    pub remotes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub committed_at: String,
    pub message: String,
    pub refs: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub commits: Vec<Commit>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Action {
    pub operation: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub reference: String,
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

async fn command(
    settings: &Settings,
    root: &Path,
    values: &[String],
) -> Result<std::process::Output, String> {
    let mut arguments = args(&[
        "--no-literal-pathspecs",
        "-c", "core.quotepath=false",
        "-c", "color.ui=false",
    ]);
    // stash internally cleans with the magic pathspec :/. Enabling literal
    // pathspecs globally prevents untracked files from being removed by stash.
    if values.first().is_some_and(|value| {
        matches!(value.as_str(), "diff" | "show" | "add" | "restore" | "rm")
    }) {
        arguments.push("--literal-pathspecs".into());
    }
    arguments.extend_from_slice(values);
    git::run(settings, root, &arguments).await
}

fn checked(output: std::process::Output) -> Result<Vec<u8>, String> {
    if !output.status.success() {
        let detail: String = String::from_utf8_lossy(&output.stderr)
            .trim()
            .chars()
            .take(2000)
            .collect();
        return Err(format!("Git 操作失败：{detail}"));
    }
    Ok(output.stdout)
}

async fn read(settings: &Settings, root: &Path, values: &[&str]) -> Result<Vec<u8>, String> {
    checked(command(settings, root, &args(values)).await?)
}

fn utf8(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| "Git 返回了无法显示的非 UTF-8 文件名或内容".into())
}

async fn root(settings: &Settings, id: &str) -> Result<PathBuf, String> {
    git::validate(settings)?;
    let repository = settings.repositories.iter()
        .find(|repository| repository.id == id)
        .ok_or("仓库配置已变化，请重新选择仓库")?;
    let top = utf8(read(
        settings,
        Path::new(repository.path.trim()),
        &["rev-parse", "--show-toplevel"],
    ).await?)?;
    std::fs::canonicalize(top.trim_end_matches(['\r', '\n']))
        .map_err(|error| format!("无法打开 Git 工作区：{error}"))
}

/// Lock all app mutations to the same canonical root, including subdirectory aliases.
pub async fn repository_lock(
    settings: &Settings,
    id: &str,
) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
    type Locks = Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>;
    static LOCKS: OnceLock<Locks> = OnceLock::new();
    let path = root(settings, id).await?;
    let lock = {
        let mut locks = LOCKS.get_or_init(Mutex::default)
            .lock()
            .map_err(|_| "Git 仓库忙，请重试")?;
        locks.entry(path).or_default().clone()
    };
    Ok(lock.lock_owned().await)
}

fn parse_status(bytes: &[u8]) -> Result<Vec<FileStatus>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "仓库包含非 UTF-8 文件名，无法安全操作")?;
    let mut entries = text.split('\0').filter(|value| !value.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 || !entry.is_char_boundary(3) {
            return Err("Git 文件状态格式无效".into());
        }
        let code = &entry[..2];
        let original_path = if code.contains(['R', 'C']) {
            Some(entries.next().ok_or("Git 重命名状态格式无效")?.into())
        } else {
            None
        };
        files.push(FileStatus {
            path: entry[3..].into(),
            original_path,
            index_status: code[..1].into(),
            worktree_status: code[1..].into(),
            conflicted: matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU"),
        });
    }
    Ok(files)
}

async fn files(settings: &Settings, root: &Path) -> Result<Vec<FileStatus>, String> {
    let output = read(settings, root, &[
        "status", "--porcelain=v1", "-z", "--untracked-files=all",
    ]).await?;
    parse_status(&output)
}

async fn has_head(settings: &Settings, root: &Path) -> Result<bool, String> {
    let output = command(settings, root, &args(&[
        "rev-parse", "--verify", "--quiet", "HEAD^{commit}",
    ])).await?;
    Ok(output.status.success())
}

async fn upstream(settings: &Settings, root: &Path) -> Result<Option<String>, String> {
    let output = command(settings, root, &args(&[
        "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}",
    ])).await?;
    Ok(output.status.success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty()))
}

pub async fn repository(settings: &Settings, id: &str) -> Result<Repository, String> {
    let root = root(settings, id).await?;
    let head = command(settings, &root, &args(&[
        "symbolic-ref", "--short", "--quiet", "HEAD",
    ])).await?;
    let detached = !head.status.success();
    let branch = if detached {
        utf8(read(settings, &root, &["rev-parse", "--short", "HEAD"]).await?)?
            .trim().into()
    } else {
        utf8(head.stdout)?.trim().into()
    };
    let upstream = upstream(settings, &root).await?;
    let (ahead, behind) = if upstream.is_some() {
        let counts = utf8(read(settings, &root, &[
            "rev-list", "--left-right", "--count", "HEAD...@{upstream}",
        ]).await?)?;
        let mut counts = counts.split_whitespace()
            .map(|value| value.parse::<usize>().unwrap_or(0));
        (counts.next().unwrap_or(0), counts.next().unwrap_or(0))
    } else {
        (0, 0)
    };
    let refs = utf8(read(settings, &root, &[
        "for-each-ref",
        "--format=%(refname)%00%(HEAD)%00%(upstream:short)%00%(symref)",
        "refs/heads", "refs/remotes",
    ]).await?)?;
    let mut branches = Vec::new();
    for line in refs.lines() {
        let fields = line.split('\0').collect::<Vec<_>>();
        if fields.len() != 4 || !fields[3].is_empty() {
            continue;
        }
        let remote = fields[0].starts_with("refs/remotes/");
        let prefix = if remote { "refs/remotes/" } else { "refs/heads/" };
        let name = fields[0].strip_prefix(prefix).unwrap_or(fields[0]);
        branches.push(Branch {
            name: name.into(),
            remote,
            current: fields[1] == "*",
            upstream: (!fields[2].is_empty()).then(|| fields[2].into()),
        });
    }
    let stash_text = utf8(read(settings, &root, &[
        "stash", "list", "--format=%gd%x00%gs%x00", "-z",
    ]).await?)?;
    let stash_fields = stash_text.split('\0')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let stashes = stash_fields.chunks_exact(2)
        .map(|fields| Stash {
            reference: fields[0].trim_start_matches('\n').into(),
            message: fields[1].into(),
        })
        .collect();
    let remotes = utf8(read(settings, &root, &["remote"]).await?)?
        .lines().map(String::from).collect();
    Ok(Repository {
        root: root.to_string_lossy().into(),
        branch,
        detached,
        upstream,
        ahead,
        behind,
        files: files(settings, &root).await?,
        branches,
        stashes,
        remotes,
    })
}

fn validate_reference(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 500
        || value.starts_with('-')
        || value.chars().any(char::is_control)
    {
        return Err("Git 引用无效".into());
    }
    Ok(())
}

async fn resolve(settings: &Settings, root: &Path, value: &str) -> Result<String, String> {
    validate_reference(value)?;
    let revision = format!("{value}^{{commit}}");
    let hash = utf8(read(settings, root, &[
        "rev-parse", "--verify", "--end-of-options", &revision,
    ]).await?)?;
    Ok(hash.trim().into())
}

pub async fn history(
    settings: &Settings,
    id: &str,
    query: &str,
    reference: &str,
    skip: usize,
) -> Result<History, String> {
    if query.len() > 1000 || query.chars().any(char::is_control) || skip > 1_000_000 {
        return Err("提交搜索条件过长或页数无效".into());
    }
    let root = root(settings, id).await?;
    if reference.is_empty() && !has_head(settings, &root).await? {
        return Ok(History { commits: Vec::new(), has_more: false });
    }
    let mut values = args(&[
        "log", "--no-color", "--no-notes", "--no-show-signature",
        "--encoding=UTF-8", "--date-order",
        "--format=%H%x00%P%x00%an%x00%ae%x00%cI%x00%B%x00%D%x00",
    ]);
    values.push(format!("--max-count={}", PAGE_SIZE + 1));
    values.push(format!("--skip={skip}"));
    if !query.trim().is_empty() {
        values.extend(args(&["--fixed-strings", "--regexp-ignore-case"]));
        values.push(format!("--grep={}", query.trim()));
    }
    if reference == "__all__" {
        values.push("--all".into());
    } else {
        let reference = if reference.is_empty() { "HEAD" } else { reference };
        values.push(resolve(settings, &root, reference).await?);
    }
    values.push("--".into());
    let text = utf8(checked(command(settings, &root, &values).await?)?)?;
    let mut fields = text.split('\0').collect::<Vec<_>>();
    if fields.last().is_some_and(|value| value.trim().is_empty()) {
        fields.pop();
    }
    if fields.len() % 7 != 0 {
        return Err("Git 提交格式无效".into());
    }
    let mut commits = fields.chunks_exact(7)
        .map(|fields| Commit {
            hash: fields[0].trim().into(),
            parents: fields[1].split_whitespace().map(String::from).collect(),
            author: fields[2].into(),
            email: fields[3].into(),
            committed_at: fields[4].into(),
            message: fields[5].trim_end().into(),
            refs: fields[6].into(),
        })
        .collect::<Vec<_>>();
    let has_more = commits.len() > PAGE_SIZE;
    commits.truncate(PAGE_SIZE);
    Ok(History { commits, has_more })
}

fn validate_path(path: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    if path.is_empty()
        || path.contains('\0')
        || candidate.is_absolute()
        || candidate.components().any(|value| !matches!(value, Component::Normal(_)))
        || path.split(['/', '\\']).any(|value| value == ".." || value.eq_ignore_ascii_case(".git"))
    {
        return Err("文件路径必须位于当前仓库内，且不能指向 Git 元数据".into());
    }
    Ok(())
}

fn diff_result(bytes: &[u8]) -> Diff {
    let truncated = bytes.len() > DIFF_LIMIT;
    let bytes = &bytes[..bytes.len().min(DIFF_LIMIT)];
    let text = if bytes.contains(&0) {
        "二进制文件，无法显示文本差异。".into()
    } else {
        String::from_utf8_lossy(bytes).into()
    };
    Diff { text, truncated }
}

pub async fn diff(
    settings: &Settings,
    id: &str,
    path: &str,
    staged: bool,
    commit: &str,
) -> Result<Diff, String> {
    let root = root(settings, id).await?;
    if !path.is_empty() {
        validate_path(path)?;
    }
    let mut values;
    if !commit.is_empty() {
        let hash = resolve(settings, &root, commit).await?;
        values = args(&[
            "show", "--format=fuller", "--root", "--first-parent", "--patch",
            "--no-ext-diff", "--no-textconv", "--no-color", &hash,
        ]);
    } else {
        let untracked = !staged && !path.is_empty()
            && files(settings, &root).await?.iter()
                .any(|file| file.path == path && file.index_status == "?");
        if untracked {
            let resolved = std::fs::canonicalize(root.join(path))
                .map_err(|error| format!("读取文件失败：{error}"))?;
            if !resolved.starts_with(&root) || !resolved.is_file() {
                return Err("无法预览指向仓库外部的链接或非普通文件".into());
            }
            let file = tokio::fs::File::open(resolved).await
                .map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            file.take((DIFF_LIMIT + 1) as u64).read_to_end(&mut bytes).await
                .map_err(|error| error.to_string())?;
            let mut result = diff_result(&bytes);
            if !bytes.contains(&0) {
                let additions = result.text.lines()
                    .map(|line| format!("+{line}\n"))
                    .collect::<String>();
                result.text = format!("新文件：{path}\n{additions}");
            }
            return Ok(result);
        }
        values = args(&["diff", "--no-ext-diff", "--no-textconv", "--no-color"]);
        if staged {
            values.push("--cached".into());
        }
    }
    values.push("--".into());
    if !path.is_empty() {
        values.push(path.into());
        if staged && commit.is_empty() {
            let statuses = files(settings, &root).await?;
            let original = statuses.iter()
                .find(|file| file.path == path)
                .and_then(|file| file.original_path.as_ref());
            if let Some(original) = original {
                validate_path(original)?;
                values.push(original.clone());
            }
        }
    }
    Ok(diff_result(&checked(command(settings, &root, &values).await?)?))
}

fn valid_stash(reference: &str) -> bool {
    reference.strip_prefix("stash@{")
        .and_then(|value| value.strip_suffix('}'))
        .is_some_and(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
}

pub struct CommitSource {
    pub content: String,
    pub revision: String,
}

async fn staged_revision(settings: &Settings, root: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let head = command(settings, root, &args(&["rev-parse", "--verify", "HEAD"])).await?;
    let branch = command(settings, root, &args(&["symbolic-ref", "--quiet", "HEAD"])).await?;
    let index = read(settings, root, &["diff", "--cached", "--raw", "--full-index", "--no-abbrev", "--no-renames", "--no-ext-diff", "--no-textconv", "-z", "--"]).await?;
    let mut digest = Sha256::new();
    for bytes in [&head.stdout, &branch.stdout, &index] { digest.update(bytes); digest.update([0]); }
    Ok(format!("{:x}", digest.finalize()))
}

/// Capture only staged content. Neither worktree changes nor untracked file contents reach AI.
pub async fn commit_source(settings: &Settings, id: &str) -> Result<CommitSource, String> {
    let root = root(settings, id).await?;
    let before = staged_revision(settings, &root).await?;
    let state = repository(settings, id).await?;
    if state.files.iter().any(|file| file.conflicted) { return Err("请先解决冲突，再生成提交说明".into()); }
    let staged: Vec<_> = state.files.iter().filter(|file| ![" ", "?", "!"].contains(&file.index_status.as_str())).map(|file| {
        serde_json::json!({ "path": file.path, "originalPath": file.original_path, "status": file.index_status })
    }).collect();
    if staged.is_empty() { return Err("请先暂存需要提交的文件，再生成提交说明".into()); }
    let patch = diff(settings, id, "", true, "").await?;
    if patch.truncated { return Err("暂存区差异过大，请分批暂存后再生成提交说明".into()); }
    let content = serde_json::json!({ "branch": state.branch, "files": staged, "diff": patch.text }).to_string();
    if content.len() > 200_000 { return Err("暂存区内容超过 200 KB，请分批暂存后再生成提交说明".into()); }
    let after = staged_revision(settings, &root).await?;
    if before != after { return Err("暂存区或分支已变化，请重新生成提交说明".into()); }
    Ok(CommitSource { content, revision: before })
}

async fn local_branch(settings: &Settings, root: &Path, reference: &str) -> Result<(), String> {
    validate_reference(reference)?;
    read(settings, root, &[
        "show-ref", "--verify", &format!("refs/heads/{reference}"),
    ]).await?;
    Ok(())
}

/// Caller holds repository_lock and has revalidated the originating workspace.
pub async fn action(settings: &Settings, id: &str, request: Action) -> Result<String, String> {
    let root = root(settings, id).await?;
    if request.message.len() > 20000 || request.message.contains('\0') || request.paths.len() > 10000 {
        return Err("Git 操作参数过长或无效".into());
    }
    let mut values;
    match request.operation.as_str() {
        "stage" | "unstage" | "discard" => {
            if request.paths.is_empty() {
                return Err("请先选择文件".into());
            }
            for path in &request.paths {
                validate_path(path)?;
            }
            let statuses = files(settings, &root).await?;
            let mut paths = Vec::new();
            for path in &request.paths {
                let file = statuses.iter().find(|file| file.path == *path)
                    .ok_or("所选文件状态已变化，请刷新后重试")?;
                if request.operation == "discard" && (file.index_status == "?" || file.conflicted) {
                    return Err("只能还原已跟踪文件的工作区修改；冲突请先手动解决".into());
                }
                paths.push(path.clone());
                if request.operation == "unstage" {
                    if let Some(original) = &file.original_path {
                        validate_path(original)?;
                        paths.push(original.clone());
                    }
                }
            }
            values = match request.operation.as_str() {
                "stage" => args(&["add", "--all"]),
                "unstage" if !has_head(settings, &root).await? => {
                    args(&["rm", "--cached", "--force", "--quiet", "--ignore-unmatch"])
                }
                "unstage" => args(&["restore", "--staged"]),
                _ => args(&["restore", "--worktree"]),
            };
            values.push("--".into());
            values.extend(paths);
        }
        "commit" => {
            let head = command(settings, &root, &args(&[
                "symbolic-ref", "--quiet", "HEAD",
            ])).await?;
            if !head.status.success() {
                return Err("当前处于分离 HEAD 状态，请先创建或切换到本地分支，再提交".into());
            }
            if request.message.trim().is_empty() {
                return Err("请填写提交说明".into());
            }
            let statuses = files(settings, &root).await?;
            if statuses.iter().any(|file| file.conflicted) {
                return Err("请先解决冲突并暂存文件，再提交".into());
            }
            if !statuses.iter().any(|file| file.index_status != " " && file.index_status != "?") {
                return Err("没有已暂存的修改，请先暂存文件".into());
            }
            values = args(&["-c", "core.editor=true", "commit", "-m", &request.message]);
        }
        "createBranch" => {
            validate_reference(&request.reference)?;
            read(settings, &root, &[
                "check-ref-format", &format!("refs/heads/{}", request.reference),
            ]).await?;
            values = args(&["switch", "-c", &request.reference]);
        }
        "switchBranch" => {
            validate_reference(&request.reference)?;
            let local = command(settings, &root, &args(&[
                "show-ref", "--verify", "--quiet", &format!("refs/heads/{}", request.reference),
            ])).await?;
            if local.status.success() {
                values = args(&["switch", &request.reference]);
            } else {
                read(settings, &root, &[
                    "show-ref", "--verify", &format!("refs/remotes/{}", request.reference),
                ]).await?;
                values = args(&["switch", "--track", &request.reference]);
            }
        }
        "deleteBranch" | "mergeBranch" => {
            if request.operation == "deleteBranch" {
                local_branch(settings, &root, &request.reference).await?;
                values = args(&["branch", "-d", "--", &request.reference]);
            } else {
                validate_reference(&request.reference)?;
                let local_ref = format!("refs/heads/{}", request.reference);
                let remote_ref = format!("refs/remotes/{}", request.reference);
                let local = command(settings, &root, &args(&[
                    "show-ref", "--verify", "--quiet", &local_ref,
                ])).await?;
                let reference = if local.status.success() {
                    local_ref
                } else {
                    read(settings, &root, &["show-ref", "--verify", &remote_ref]).await?;
                    remote_ref
                };
                values = args(&["merge", "--ff-only", "--", &reference]);
            }
        }
        "stash" => {
            if files(settings, &root).await?.is_empty() {
                return Err("工作区没有可储藏的修改".into());
            }
            values = args(&["stash", "push", "--include-untracked"]);
            if !request.message.trim().is_empty() {
                values.extend(args(&["-m", &request.message]));
            }
        }
        "applyStash" | "popStash" | "dropStash" => {
            if !valid_stash(&request.reference) {
                return Err("请选择有效的 Stash 记录".into());
            }
            let operation = match request.operation.as_str() {
                "applyStash" => "apply",
                "popStash" => "pop",
                _ => "drop",
            };
            values = args(&["stash", operation, &request.reference]);
        }
        "fetch" => {
            if read(settings, &root, &["remote"]).await?.is_empty() {
                return Err("尚未配置远程仓库，请先在终端添加 remote".into());
            }
            values = args(&["fetch", "--all", "--prune"]);
        }
        "pull" | "push" => {
            let symbolic = read(settings, &root, &["symbolic-ref", "--quiet", "HEAD"]).await?;
            let branch_ref = utf8(symbolic)?.trim().to_string();
            let tracking = utf8(read(settings, &root, &[
                "for-each-ref", "--format=%(upstream:remotename)%00%(upstream:remoteref)",
                &branch_ref,
            ]).await?)?;
            let (remote, remote_ref) = tracking.trim_end().split_once('\0')
                .ok_or("当前分支没有上游分支，请先在终端配置 upstream 后再同步")?;
            if remote.is_empty()
                || remote.starts_with('-')
                || remote.chars().any(char::is_control)
                || !remote_ref.starts_with("refs/heads/")
            {
                return Err("当前分支没有有效的上游分支，请先在终端配置 upstream 后再同步".into());
            }
            read(settings, &root, &["check-ref-format", remote_ref]).await?;
            if request.operation == "pull" {
                if !files(settings, &root).await?.is_empty() {
                    return Err("请先提交或储藏工作区修改，再拉取更新".into());
                }
                values = args(&[
                    "pull", "--ff-only", "--no-rebase", "--no-autostash", "--", remote, remote_ref,
                ]);
            } else {
                // Explicitly push one upstream ref. User configuration such as
                // push.default=matching, remote.*.push or mirror must not push others.
                values = args(&[
                    "-c", &format!("remote.{remote}.mirror=false"),
                    "-c", "push.followTags=false",
                    "push", "--no-force", "--no-follow-tags", "--", remote,
                    &format!("HEAD:{remote_ref}"),
                ]);
            }
        }
        _ => return Err("不支持的 Git 操作".into()),
    }
    let output = command(settings, &root, &values).await?;
    if !output.status.success() {
        return checked(output).map(|_| String::new());
    }
    let text = [
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ].iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(if text.is_empty() {
        "Git 操作已完成".into()
    } else {
        text.chars().take(4000).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn git(path: &Path, values: &[&str]) -> String {
        let output =
            std::process::Command::new("git").current_dir(path).args(values).env("GIT_CONFIG_NOSYSTEM",
                            "1").env("GIT_CONFIG_GLOBAL",
                        "/dev/null").output().unwrap();
        assert!(output.status.success(), "git {:?}: {}", values,
        String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().into()
    }
    fn setup() -> (tempfile::TempDir, Settings) {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        git(dir.path(), &["config", "user.name", "测试作者"]);
        git(dir.path(), &["config", "user.email", "author@example.test"]);
        git(dir.path(), &["config", "commit.gpgsign", "false"]);
        let settings =
            Settings {
                repositories: vec![git::Repository
                    {
                        id: "repo".into(), name: "测试".into(), path:
                        dir.path().to_string_lossy().into(), ..Default::default()
                    }],
                ..Default::default()
            };
        (dir, settings)
    }
    fn request(operation: &str, paths: &[&str], message: &str,
        reference: &str) -> Action {
        Action {
            operation: operation.into(),
            paths: paths.iter().map(|s| (*s).into()).collect(),
            message: message.into(),
            reference: reference.into(),
        }
    }
    async fn act(settings: &Settings, operation: &str, paths: &[&str],
        message: &str, reference: &str) -> String {
        action(settings, "repo",
                    request(operation, paths, message,
                        reference)).await.unwrap()
    }
    async fn first_commit(dir: &Path, settings: &Settings) {
        std::fs::write(dir.join("file.txt"), "initial\n").unwrap();
        act(settings, "stage", &["file.txt"], "", "").await;
        act(settings, "commit", &[], "initial", "").await;
    }
    #[tokio::test]
    async fn unborn_repository_status_stage_diff_unstage_and_commit() {
        let (dir, settings) = setup();
        let status = repository(&settings, "repo").await.unwrap();
        assert_eq!(status.branch, "main");
        assert!(!status.detached);
        assert!(status.files.is_empty());
        assert!(history(&settings, "repo", "", "",
        0).await.unwrap().commits.is_empty());
        let path = "new [literal]*\nfile.txt";
        std::fs::write(dir.path().join(path), "hello\n").unwrap();
        let status = repository(&settings, "repo").await.unwrap();
        assert_eq!(status.files[0].path, path);
        assert!(diff(&settings, "repo", path, false,
        "").await.unwrap().text.contains("+hello"));
        act(&settings, "stage", &[path], "", "").await;
        assert!(diff(&settings, "repo", path, true,
        "").await.unwrap().text.contains("+hello"));
        std::fs::write(dir.path().join(path), "hello\nmore\n").unwrap();
        act(&settings, "unstage", &[path], "", "").await;
        assert!(dir.path().join(path).exists());
        assert_eq!(repository(&settings,
        "repo").await.unwrap().files[0].index_status, "?");
        act(&settings, "stage", &[path], "", "").await;
        act(&settings, "commit", &[], "完成文件\n\n详细说明",
                "").await;
        let log = history(&settings, "repo", "详细", "", 0).await.unwrap();
        assert_eq!(log.commits.len(), 1);
        assert!(log.commits[0].parents.is_empty());
        assert!(diff(&settings, "repo", "", false,
        &log.commits[0].hash).await.unwrap().text.contains("+more"));
    }

    #[tokio::test]
    async fn ai_commit_source_uses_only_the_index_and_detects_staged_or_branch_changes() {
        let (dir, settings) = setup();
        assert!(commit_source(&settings, "repo").await.err().unwrap().contains("先暂存"));
        std::fs::write(dir.path().join("file.txt"), "STAGED_CONTENT\n").unwrap();
        git(dir.path(), &["add", "file.txt"]);
        std::fs::write(dir.path().join("file.txt"), "UNSTAGED_SECRET\n").unwrap();
        std::fs::write(dir.path().join("private.txt"), "UNTRACKED_SECRET\n").unwrap();
        let before = commit_source(&settings, "repo").await.unwrap();
        assert!(before.content.contains("STAGED_CONTENT"));
        assert!(!before.content.contains("UNSTAGED_SECRET"));
        assert!(!before.content.contains("UNTRACKED_SECRET"));
        assert!(!before.content.contains("private.txt"));
        assert!(!before.content.contains(dir.path().to_str().unwrap()));
        let stable = commit_source(&settings, "repo").await.unwrap();
        assert_eq!(stable.revision, before.revision);
        git(dir.path(), &["add", "file.txt"]);
        let changed = commit_source(&settings, "repo").await.unwrap();
        assert_ne!(changed.revision, before.revision);
        assert_eq!(git(dir.path(), &["show", ":file.txt"]), "UNSTAGED_SECRET");
        git(dir.path(), &["symbolic-ref", "HEAD", "refs/heads/other"]);
        assert_ne!(commit_source(&settings, "repo").await.unwrap().revision, changed.revision);
    }

    #[tokio::test]
    async fn ai_commit_source_rejects_oversized_changes_without_modifying_the_index() {
        let (dir, settings) = setup();
        std::fs::write(dir.path().join("large.txt"), "x".repeat(210_000)).unwrap();
        git(dir.path(), &["add", "large.txt"]);
        let status = git(dir.path(), &["status", "--porcelain"]);
        assert!(commit_source(&settings, "repo").await.err().unwrap().contains("分批暂存"));
        assert_eq!(git(dir.path(), &["status", "--porcelain"]), status);
    }
    #[tokio::test]
    async fn partial_staging_discard_and_rename_preserve_index_and_working_files() {
        let (dir, settings) = setup();
        first_commit(dir.path(), &settings).await;
        std::fs::write(dir.path().join("file.txt"), "staged\n").unwrap();
        act(&settings, "stage", &["file.txt"], "", "").await;
        std::fs::write(dir.path().join("file.txt"), "unstaged\n").unwrap();
        let status = repository(&settings, "repo").await.unwrap();
        assert_eq!(status.files[0].index_status, "M");
        assert_eq!(status.files[0].worktree_status, "M");
        act(&settings, "discard", &["file.txt"], "", "").await;
        assert_eq!(std::fs::read_to_string(dir.path().join("file.txt")).unwrap(),
        "staged\n");
        assert_eq!(git(dir.path(), &["show", ":file.txt"]), "staged");
        act(&settings, "unstage", &["file.txt"], "", "").await;
        git(dir.path(), &["restore", "--worktree", "file.txt"]);
        git(dir.path(), &["mv", "file.txt", "renamed file.txt"]);
        let status = repository(&settings, "repo").await.unwrap();
        assert_eq!(status.files[0].original_path.as_deref(),
        Some("file.txt"));
        std::fs::write(dir.path().join("renamed file.txt"),
                "initial\nx\n").unwrap();
        act(&settings, "stage", &["renamed file.txt"], "", "").await;
        act(&settings, "unstage", &["renamed file.txt"], "", "").await;
        assert!(dir.path().join("renamed file.txt").exists());
        assert!(git(dir.path(),
        &["diff", "--cached", "--name-only"]).is_empty());
    }
    #[tokio::test]
    async fn branch_and_stash_lifecycle_keeps_untracked_files() {
        let (dir, settings) = setup();
        first_commit(dir.path(), &settings).await;
        act(&settings, "createBranch", &[], "", "topic").await;
        assert_eq!(repository(&settings, "repo").await.unwrap().branch,
        "topic");
        std::fs::write(dir.path().join("untracked.txt"), "saved").unwrap();
        act(&settings, "stash", &[], "保存工作", "").await;
        assert!(!dir.path().join("untracked.txt").exists());
        let status = repository(&settings, "repo").await.unwrap();
        assert_eq!(status.stashes.len(), 1);
        assert_eq!(status.stashes[0].reference, "stash@{0}");
        act(&settings, "applyStash", &[], "", "stash@{0}").await;
        assert_eq!(std::fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
        "saved");
        act(&settings, "dropStash", &[], "", "stash@{0}").await;
        act(&settings, "switchBranch", &[], "", "main").await;
        act(&settings, "mergeBranch", &[], "", "topic").await;
        act(&settings, "deleteBranch", &[], "", "topic").await;
        assert!(repository(&settings,
        "repo").await.unwrap().branches.iter().all(|b| b.name != "topic"));
    }
    #[tokio::test]
    async fn local_remote_fetch_pull_push_tracking_and_remote_checkout() {
        let (dir, settings) = setup();
        first_commit(dir.path(), &settings).await;
        let bare = tempfile::tempdir().unwrap();
        git(bare.path(), &["init", "--bare", "-b", "main"]);
        git(dir.path(),
            &["remote", "add", "origin", bare.path().to_str().unwrap()]);
        git(dir.path(), &["push", "-u", "origin", "main"]);
        act(&settings, "createBranch", &[], "", "topic").await;
        git(dir.path(), &["push", "-u", "origin", "topic"]);
        act(&settings, "switchBranch", &[], "", "main").await;
        act(&settings, "deleteBranch", &[], "", "topic").await;
        act(&settings, "fetch", &[], "", "").await;
        act(&settings, "switchBranch", &[], "", "origin/topic").await;
        assert_eq!(repository(&settings,
        "repo").await.unwrap().upstream.as_deref(), Some("origin/topic"));
        std::fs::write(dir.path().join("file.txt"), "remote\n").unwrap();
        act(&settings, "stage", &["file.txt"], "", "").await;
        act(&settings, "commit", &[], "remote change", "").await;
        assert_eq!(repository(&settings, "repo").await.unwrap().ahead, 1);
        act(&settings, "push", &[], "", "").await;
        act(&settings, "pull", &[], "", "").await;
        assert_eq!(repository(&settings, "repo").await.unwrap().ahead, 0);
    }
    #[tokio::test]
    async fn rejects_unconfigured_repositories_paths_options_and_destructive_shortcuts() {
        let (dir, settings) = setup();
        first_commit(dir.path(), &settings).await;
        assert!(repository(&settings, "missing").await.is_err());
        for path in
            ["../outside", "/tmp/outside", ".git/config",
                    "nested/../../outside", "a\\..\\outside"] {
            assert!(action(&settings, "repo",
            request("stage", &[path], "", "")).await.is_err(), "{path}");
        }
        for op in
            ["switchBranch", "createBranch", "deleteBranch", "mergeBranch",
                    "applyStash"] {
            assert!(action(&settings, "repo",
            request(op, &[], "", "--force")).await.is_err());
        }
        assert!(action(&settings, "repo",
        request("reset", &[], "", "")).await.is_err());
        assert!(action(&settings, "repo",
        request("push", &[], "",
        "")).await.unwrap_err().contains("upstream"));
        assert!(action(&settings, "repo",
        request("commit", &[], "  ", "")).await.is_err());
        std::fs::write(dir.path().join("new.txt"), "keep").unwrap();
        assert!(action(&settings, "repo",
        request("discard", &["new.txt"], "", "")).await.is_err());
        assert!(dir.path().join("new.txt").exists());
    }
    #[tokio::test]
    async fn push_only_updates_current_upstream_despite_matching_mirror_and_force_config() {
        let (dir, settings) = setup();
        first_commit(dir.path(), &settings).await;
        let bare = tempfile::tempdir().unwrap();
        git(bare.path(), &["init", "--bare", "-b", "main"]);
        git(dir.path(),
            &["remote", "add", "origin", bare.path().to_str().unwrap()]);
        git(dir.path(), &["push", "-u", "origin", "main"]);
        act(&settings, "createBranch", &[], "", "unrelated").await;
        git(dir.path(), &["push", "-u", "origin", "unrelated"]);
        let remote_before =
            git(bare.path(), &["rev-parse", "refs/heads/unrelated"]);
        std::fs::write(dir.path().join("file.txt"),
                "other branch\n").unwrap();
        act(&settings, "stage", &["file.txt"], "", "").await;
        act(&settings, "commit", &[], "unrelated local commit", "").await;
        act(&settings, "switchBranch", &[], "", "main").await;
        std::fs::write(dir.path().join("file.txt"),
                "current branch\n").unwrap();
        act(&settings, "stage", &["file.txt"], "", "").await;
        act(&settings, "commit", &[], "current local commit", "").await;
        git(dir.path(), &["config", "push.default", "matching"]);
        git(dir.path(), &["config", "remote.origin.mirror", "true"]);
        git(dir.path(),
            &["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"]);
        act(&settings, "push", &[], "", "").await;
        assert_eq!(git(bare.path(), &["rev-parse", "refs/heads/main"]),
        git(dir.path(), &["rev-parse", "HEAD"]));
        assert_eq!(git(bare.path(), &["rev-parse", "refs/heads/unrelated"]),
        remote_before);
        std::fs::write(dir.path().join("file.txt"),
                "keep unsaved\n").unwrap();
        let failure =
            action(&settings, "repo",
                        request("pull", &[], "", "")).await.unwrap_err();
        assert!(failure.contains("提交或储藏"));
        assert_eq!(std::fs::read_to_string(dir.path().join("file.txt")).unwrap(),
        "keep unsaved\n");
    }

    #[tokio::test]
    async fn conflicts_block_commit_and_discard_and_fast_forward_merge_rejects_divergence() {
        let (dir, settings) = setup();
        first_commit(dir.path(), &settings).await;
        act(&settings, "createBranch", &[], "", "topic").await;
        std::fs::write(dir.path().join("file.txt"), "topic\n").unwrap();
        act(&settings, "stage", &["file.txt"], "", "").await;
        act(&settings, "commit", &[], "topic commit", "").await;
        act(&settings, "switchBranch", &[], "", "main").await;
        std::fs::write(dir.path().join("file.txt"), "main\n").unwrap();
        act(&settings, "stage", &["file.txt"], "", "").await;
        act(&settings, "commit", &[], "main commit", "").await;
        let before = git(dir.path(), &["rev-parse", "HEAD"]);
        assert!(action(&settings, "repo",
        request("mergeBranch", &[], "", "topic")).await.is_err());
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
        let merge =
            std::process::Command::new("git").current_dir(dir.path()).args(["-c",
                                "commit.gpgsign=false", "merge", "--no-edit",
                                "topic"]).output().unwrap();
        assert!(!merge.status.success());
        assert!(repository(&settings,
        "repo").await.unwrap().files[0].conflicted);
        assert!(action(&settings, "repo",
        request("commit", &[], "blocked",
        "")).await.unwrap_err().contains("冲突"));
        assert!(action(&settings, "repo",
        request("discard", &["file.txt"], "", "")).await.is_err());
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    }

    #[tokio::test]
    async fn detached_head_refuses_commit_even_with_staged_changes() {
        let (dir, settings) = setup();
        first_commit(dir.path(), &settings).await;
        let before = git(dir.path(), &["rev-parse", "HEAD"]);
        git(dir.path(), &["switch", "--detach", "HEAD"]);
        std::fs::write(dir.path().join("file.txt"),
                "preserve staged\n").unwrap();
        act(&settings, "stage", &["file.txt"], "", "").await;
        assert!(repository(&settings, "repo").await.unwrap().detached);
        let failure =
            action(&settings, "repo",
                        request("commit", &[], "would detach",
                            "")).await.unwrap_err();
        assert!(failure.contains("分离 HEAD"));
        assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
        assert_eq!(git(dir.path(), &["show", ":file.txt"]),
        "preserve staged");
    }

    #[test]
    fn parses_conflicts_and_unusual_rename_names_without_quoting() {
        let status =
            parse_status(b"UU conflict\0R  new\nname\0old name\0?? [file]*\0").unwrap();
        assert!(status[0].conflicted);
        assert_eq!(status[1].path, "new\nname");
        assert_eq!(status[1].original_path.as_deref(), Some("old name"));
        assert_eq!(status[2].path, "[file]*");
    }
}

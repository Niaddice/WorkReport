//! Database storage for journals. Every query is scoped by a persistent workspace UUID.
//! SQL parameters and connection options are bound separately from statement text.

use crate::workspace::Format;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode},
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    ConnectOptions, MySqlPool, PgPool, SqlitePool,
};
use std::{
    collections::HashSet,
    fmt,
    fs,
    future::Future,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
/// 历史版本上限的可配置范围：1 GB – 10 GB，默认 1 GB。
pub const MIN_HISTORY_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_HISTORY_LIMIT_BYTES: u64 = 10 * 1024 * 1024 * 1024;
pub const DEFAULT_HISTORY_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Sqlite,
    Mysql,
    Postgres,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Dual,
    Database,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TlsMode {
    Disabled,
    #[default]
    Preferred,
    Required,
    VerifyFull,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {    pub enabled: bool,
    pub engine: Engine,
    pub mode: Mode,
    pub workspace_id: String,
    pub sqlite_path: String,
    pub host: String,
    /// Zero means the engine's usual port (3306 or 5432).
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub tls_mode: TlsMode,
    /// 新建日报时使用的默认格式；已有日报的格式不受影响。
    pub default_format: crate::workspace::Format,
    /// 历史版本占用的最大字节数，超出后从最旧的一天开始清理。
    pub history_limit_bytes: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: Engine::Sqlite,
            mode: Mode::Dual,
            workspace_id: String::new(),
            sqlite_path: "workreport.sqlite".into(),
            host: "127.0.0.1".into(),
            port: 0,
            database: "workreport".into(),
            user: String::new(),
            password: String::new(),
            tls_mode: TlsMode::Preferred,
            default_format: crate::workspace::Format::Txt,
            history_limit_bytes: DEFAULT_HISTORY_LIMIT_BYTES,
        }
    }
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("enabled", &self.enabled)
            .field("engine", &self.engine)
            .field("mode", &self.mode)
            .field("workspace_id", &self.workspace_id)
            .field("sqlite_path", &self.sqlite_path)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password", &"[REDACTED]")
            .field("tls_mode", &self.tls_mode)
            .finish()
    }
}

pub fn validate(settings: &Settings) -> Result<Settings, String> {
    let mut settings = settings.clone();
    settings.sqlite_path = settings.sqlite_path.trim().into();
    settings.host = settings.host.trim().into();
    settings.database = settings.database.trim().into();
    settings.user = settings.user.trim().into();
    settings.workspace_id = settings.workspace_id.trim().to_ascii_lowercase();
    settings.history_limit_bytes = settings
        .history_limit_bytes
        .clamp(MIN_HISTORY_LIMIT_BYTES, MAX_HISTORY_LIMIT_BYTES);
    if settings.sqlite_path.is_empty() {
        settings.sqlite_path = "workreport.sqlite".into();
    }
    if !settings.workspace_id.is_empty() {
        validate_workspace_id(&settings.workspace_id)?;
    }
    // An inactive section may retain incomplete connection fields for later editing.
    if !settings.enabled {
        return Ok(settings);
    }
    match settings.engine {
        Engine::Sqlite => validate_relative_sqlite_path(&settings.sqlite_path)?,
        Engine::Mysql | Engine::Postgres => {
            if settings.host.is_empty()
                || settings.host.len() > 253
                || settings.host.chars().any(char::is_whitespace)
                || settings.host.contains(['/', '\\', '@', '?', '#', '\0'])
            {
                return Err("数据库地址须为主机名或 IP 地址，不要填写连接 URL".into());
            }
            if settings.database.is_empty() || settings.database.len() > 128 {
                return Err("请填写数据库名称（最多 128 个字节）".into());
            }
            if settings.user.is_empty() || settings.user.len() > 128 {
                return Err("请填写数据库用户名（最多 128 个字节）".into());
            }
            if settings.database.contains('\0')
                || settings.user.contains('\0')
                || settings.password.contains('\0')
            {
                return Err("数据库连接设置包含无效字符".into());
            }
        }
    }
    Ok(settings)
}

pub fn validate_workspace_id(id: &str) -> Result<(), String> {
    if id.len() != 36
        || !id.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
    {
        return Err("工作空间数据库标识无效，请重新保存数据库设置".into());
    }
    Ok(())
}

fn validate_relative_sqlite_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.contains(['\\', '\0'])
        || path.components().next().is_none()
        || !path.components().all(|part| matches!(part, Component::Normal(_)))
        || !matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("sqlite" | "sqlite3" | "db")
        )
    {
        return Err("SQLite 文件须为工作空间内的相对路径，扩展名为 .sqlite、.sqlite3 或 .db".into());
    }
    Ok(())
}

fn sqlite_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative_sqlite_path(relative)?;
    let root = fs::canonicalize(root).map_err(|_| "无法访问 SQLite 所在工作空间")?;
    if !root.is_dir() {
        return Err("SQLite 工作空间不是目录".into());
    }
    let mut current = root.clone();
    let parts: Vec<_> = Path::new(relative).components().collect();
    for (index, part) in parts.iter().enumerate() {
        current.push(part.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                // Reject even an in-workspace symlink so SQLite sidecar files are predictable.
                if metadata.file_type().is_symlink() {
                    return Err("SQLite 路径不能包含符号链接，请选择工作空间内的实际文件".into());
                }
                if index + 1 < parts.len() && !metadata.is_dir() {
                    return Err("SQLite 文件的上级路径不是目录".into());
                }
                if index + 1 == parts.len() && !metadata.is_file() {
                    return Err("SQLite 路径须指向文件".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if index + 1 < parts.len() {
                    fs::create_dir(&current).map_err(|_| "无法创建 SQLite 文件目录")?;
                }
            }
            Err(_) => return Err("无法访问 SQLite 路径".into()),
        }
    }
    // Existing directories above were checked individually, rather than following links.
    if !current.starts_with(&root) {
        return Err("SQLite 文件必须位于当前工作空间内".into());
    }
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = current.as_os_str().to_os_string();
        sidecar.push(suffix);
        if fs::symlink_metadata(Path::new(&sidecar))
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err("SQLite 辅助文件不能为符号链接，请检查数据库目录".into());
        }
    }
    Ok(current)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JournalRecord {
    pub date: String,
    pub format: Format,
    pub content: String,
    pub revision: String,
}

impl JournalRecord {
    pub fn new(date: impl Into<String>, format: Format, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            date: date.into(),
            format,
            revision: content_revision(&content),
            content,
        }
    }
}

pub fn content_revision(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

fn validate_date(date: &str) -> Result<(), String> {
    if date.len() != 8
        || !date.bytes().all(|byte| byte.is_ascii_digit())
        || NaiveDate::parse_from_str(date, "%Y%m%d").is_err()
    {
        return Err("数据库日报日期必须为有效的 yyyymmdd 日期".into());
    }
    Ok(())
}

fn validate_record(record: &JournalRecord) -> Result<(), String> {
    validate_date(&record.date)?;
    if record.revision != content_revision(&record.content) {
        return Err(format!("{} 数据库日报校验失败，内容与版本不一致，请检查数据库记录", record.date));
    }
    Ok(())
}

fn decode_row(row: (String, String, String, String)) -> Result<JournalRecord, String> {
    let (date, format, content, revision) = row;
    let format = match format.as_str() {
        "md" => Format::Md,
        "txt" => Format::Txt,
        _ => return Err("数据库包含不支持的日报格式，请检查数据库记录".into()),
    };
    let record = JournalRecord { date, format, content, revision };
    validate_record(&record)?;
    Ok(record)
}

#[derive(Clone)]
enum Pool {
    Sqlite(SqlitePool),
    Mysql(MySqlPool),
    Postgres(PgPool),
}

#[derive(Clone)]
pub struct Database {
    pool: Pool,
    workspace_id: String,
}

impl fmt::Debug for Database {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Database")
            .field("workspace_id", &self.workspace_id)
            .finish_non_exhaustive()
    }
}

// The expanded body is checked once against each concrete driver; all values use binds.
macro_rules! with_pool {
    ($pool:expr, $binding:ident, $body:block) => {
        match $pool {
            Pool::Sqlite($binding) => $body,
            Pool::Mysql($binding) => $body,
            Pool::Postgres($binding) => $body,
        }
    };
}

fn database_error(error: sqlx::Error) -> String {
    // Driver messages may contain connection options or journal values. Expose only
    // a bounded SQLSTATE/code and an actionable category, never the original message.
    if let Some(code) = error.as_database_error().and_then(|error| error.code()) {
        let code: String = code.chars().filter(char::is_ascii_alphanumeric).take(10).collect();
        return format!("数据库操作失败（{code}），请检查账号权限、数据库结构与连接状态");
    }
    match error {
        sqlx::Error::PoolTimedOut => "数据库连接超时，请检查地址、端口及网络".into(),
        sqlx::Error::Tls(_) => "数据库 TLS 连接失败，请检查加密模式与服务端证书".into(),
        sqlx::Error::Io(_) => "数据库连接失败，请检查地址、端口、网络及账号设置".into(),
        sqlx::Error::Configuration(_) => "数据库连接配置无效，请检查连接设置".into(),
        _ => "数据库操作失败，请检查连接、账号权限与数据库结构".into(),
    }
}

async fn bounded<T>(future: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    tokio::time::timeout(QUERY_TIMEOUT, future)
        .await
        .map_err(|_| "数据库操作超时，请检查数据库状态后重试".to_string())?
}

impl Database {
    pub async fn connect(root: &Path, settings: &Settings, workspace_id: &str) -> Result<Self, String> {
        validate_workspace_id(workspace_id)?;
        let mut connection_settings = settings.clone();
        connection_settings.enabled = true;
        let settings = validate(&connection_settings)?;
        let future = async {
            let pool = match settings.engine {
                Engine::Sqlite => {
                    let path = sqlite_file(root, &settings.sqlite_path)?;
                    let options = SqliteConnectOptions::new()
                        .filename(path)
                        .create_if_missing(true)
                        .journal_mode(SqliteJournalMode::Wal)
                        .busy_timeout(Duration::from_secs(5))
                        .disable_statement_logging();
                    Pool::Sqlite(SqlitePoolOptions::new()
                        .max_connections(1)
                        .acquire_timeout(CONNECT_TIMEOUT)
                        .connect_with(options).await.map_err(database_error)?)
                }
                Engine::Mysql => {
                    let options = MySqlConnectOptions::new()
                        .host(&settings.host)
                        .port(if settings.port == 0 { 3306 } else { settings.port })
                        .database(&settings.database)
                        .username(&settings.user)
                        .password(&settings.password)
                        .ssl_mode(match settings.tls_mode {
                            TlsMode::Disabled => MySqlSslMode::Disabled,
                            TlsMode::Preferred => MySqlSslMode::Preferred,
                            TlsMode::Required => MySqlSslMode::Required,
                            TlsMode::VerifyFull => MySqlSslMode::VerifyIdentity,
                        })
                        .charset("utf8mb4")
                        .disable_statement_logging();
                    Pool::Mysql(MySqlPoolOptions::new()
                        .max_connections(1)
                        .acquire_timeout(CONNECT_TIMEOUT)
                        .connect_with(options).await.map_err(database_error)?)
                }
                Engine::Postgres => {
                    let options = PgConnectOptions::new_without_pgpass()
                        .host(&settings.host)
                        .port(if settings.port == 0 { 5432 } else { settings.port })
                        .database(&settings.database)
                        .username(&settings.user)
                        .password(&settings.password)
                        .ssl_mode(match settings.tls_mode {
                            TlsMode::Disabled => PgSslMode::Disable,
                            TlsMode::Preferred => PgSslMode::Prefer,
                            TlsMode::Required => PgSslMode::Require,
                            TlsMode::VerifyFull => PgSslMode::VerifyFull,
                        })
                        .application_name("WorkReport")
                        .disable_statement_logging();
                    Pool::Postgres(PgPoolOptions::new()
                        .max_connections(1)
                        .acquire_timeout(CONNECT_TIMEOUT)
                        .connect_with(options).await.map_err(database_error)?)
                }
            };
            let database = Self { pool, workspace_id: workspace_id.to_ascii_lowercase() };
            database.ensure_schema().await?;
            Ok(database)
        };
        tokio::time::timeout(CONNECT_TIMEOUT, future)
            .await
            .map_err(|_| "数据库连接超时，请检查地址、端口及网络".to_string())?
    }

    async fn ensure_schema(&self) -> Result<(), String> {
        let sql = match &self.pool {
            Pool::Mysql(_) => "CREATE TABLE IF NOT EXISTS workreport_journal (workspace_id VARCHAR(36) NOT NULL, journal_date CHAR(8) NOT NULL, format VARCHAR(3) NOT NULL, content LONGTEXT NOT NULL, revision CHAR(64) NOT NULL, PRIMARY KEY (workspace_id, journal_date)) ENGINE=InnoDB CHARACTER SET utf8mb4 COLLATE utf8mb4_bin",
            _ => "CREATE TABLE IF NOT EXISTS workreport_journal (workspace_id VARCHAR(36) NOT NULL, journal_date CHAR(8) NOT NULL, format VARCHAR(3) NOT NULL, content TEXT NOT NULL, revision CHAR(64) NOT NULL, PRIMARY KEY (workspace_id, journal_date))",
        };
        with_pool!(&self.pool, pool, {
            sqlx::query(sql).execute(pool).await.map_err(database_error)?;
        });
        // A pre-existing MyISAM table would silently defeat transaction rollback.
        if let Pool::Mysql(pool) = &self.pool {
            let engine: String = sqlx::query_scalar(
                "SELECT ENGINE FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'workreport_journal'",
            ).fetch_one(pool).await.map_err(database_error)?;
            if !engine.eq_ignore_ascii_case("innodb") {
                return Err("数据库日报表必须使用 InnoDB，当前表不支持可靠的事务备份".into());
            }
        }
        with_pool!(&self.pool, pool, {
            sqlx::query("SELECT workspace_id, journal_date, format, content, revision FROM workreport_journal WHERE 1 = 0")
                .execute(pool).await.map_err(database_error)?;
        });
        Ok(())
    }

    fn sql<'a>(&self, question: &'a str, numbered: &'a str) -> &'a str {
        if matches!(self.pool, Pool::Postgres(_)) { numbered } else { question }
    }

    pub async fn list(&self) -> Result<Vec<JournalRecord>, String> {
        bounded(async {
            let sql = self.sql(
                "SELECT journal_date, format, content, revision FROM workreport_journal WHERE workspace_id = ? ORDER BY journal_date",
                "SELECT journal_date, format, content, revision FROM workreport_journal WHERE workspace_id = $1 ORDER BY journal_date",
            );
            let rows = with_pool!(&self.pool, pool, {
                sqlx::query_as::<_, (String, String, String, String)>(sql)
                    .bind(&self.workspace_id).fetch_all(pool).await.map_err(database_error)?
            });
            rows.into_iter().map(decode_row).collect()
        }).await
    }

    pub async fn read(&self, date: &str) -> Result<Option<JournalRecord>, String> {
        validate_date(date)?;
        bounded(async {
            let sql = self.sql(
                "SELECT journal_date, format, content, revision FROM workreport_journal WHERE workspace_id = ? AND journal_date = ?",
                "SELECT journal_date, format, content, revision FROM workreport_journal WHERE workspace_id = $1 AND journal_date = $2",
            );
            let row = with_pool!(&self.pool, pool, {
                sqlx::query_as::<_, (String, String, String, String)>(sql)
                    .bind(&self.workspace_id).bind(date).fetch_optional(pool).await.map_err(database_error)?
            });
            row.map(decode_row).transpose()
        }).await
    }

    /// Copy a complete snapshot atomically. Conflicts abort the entire transaction.
    /// `overwrite` is reserved for an explicit user choice at the migration layer.
    pub async fn upsert_many(&self, records: &[JournalRecord], overwrite: bool) -> Result<(usize, usize), String> {
        self.write_records(records, WritePolicy::Copy { overwrite }).await
    }

    /// Insert when `None`, or replace only the revision that the editor originally read.
    pub async fn save(&self, record: &JournalRecord, expected_revision: Option<&str>) -> Result<(), String> {
        self.write_records(std::slice::from_ref(record), WritePolicy::Save { expected_revision }).await?;
        Ok(())
    }

    async fn write_records(&self, records: &[JournalRecord], policy: WritePolicy<'_>) -> Result<(usize, usize), String> {
        let mut dates = HashSet::new();
        for record in records {
            validate_record(record)?;
            if !dates.insert(&record.date) {
                return Err(format!("复制内容包含重复日期 {}，未写入数据库", record.date));
            }
        }
        bounded(async {
            let select = match &self.pool {
                Pool::Sqlite(_) => "SELECT journal_date, format, content, revision FROM workreport_journal WHERE workspace_id = ? AND journal_date = ?",
                Pool::Mysql(_) => "SELECT journal_date, format, content, revision FROM workreport_journal WHERE workspace_id = ? AND journal_date = ? FOR UPDATE",
                Pool::Postgres(_) => "SELECT journal_date, format, content, revision FROM workreport_journal WHERE workspace_id = $1 AND journal_date = $2 FOR UPDATE",
            };
            let insert = self.sql(
                "INSERT INTO workreport_journal (workspace_id, journal_date, format, content, revision) VALUES (?, ?, ?, ?, ?)",
                "INSERT INTO workreport_journal (workspace_id, journal_date, format, content, revision) VALUES ($1, $2, $3, $4, $5)",
            );
            let update = self.sql(
                "UPDATE workreport_journal SET format = ?, content = ?, revision = ? WHERE workspace_id = ? AND journal_date = ? AND revision = ?",
                "UPDATE workreport_journal SET format = $1, content = $2, revision = $3 WHERE workspace_id = $4 AND journal_date = $5 AND revision = $6",
            );
            with_pool!(&self.pool, pool, {
                let mut transaction = pool.begin().await.map_err(database_error)?;
                let mut copied = 0;
                let mut skipped = 0;
                for record in records {
                    let existing = sqlx::query_as::<_, (String, String, String, String)>(select)
                        .bind(&self.workspace_id).bind(&record.date)
                        .fetch_optional(&mut *transaction).await.map_err(database_error)?
                        .map(decode_row).transpose()?;
                    let changed = match (&policy, &existing) {
                        (WritePolicy::Copy { .. }, Some(current)) if current == record => false,
                        (WritePolicy::Copy { overwrite: false }, Some(_)) => return Err(conflict(&record.date)),
                        (WritePolicy::Save { expected_revision }, current) => {
                            if current.as_ref().map(|item| item.revision.as_str()) != *expected_revision {
                                return Err(conflict(&record.date));
                            }
                            current.as_ref() != Some(record)
                        }
                        _ => true,
                    };
                    if !changed {
                        skipped += 1;
                        continue;
                    }
                    if let Some(current) = existing {
                        let result = sqlx::query(update)
                            .bind(record.format.extension()).bind(&record.content).bind(&record.revision)
                            .bind(&self.workspace_id).bind(&record.date).bind(&current.revision)
                            .execute(&mut *transaction).await.map_err(database_error)?;
                        if result.rows_affected() != 1 {
                            return Err(conflict(&record.date));
                        }
                    } else {
                        sqlx::query(insert)
                            .bind(&self.workspace_id).bind(&record.date).bind(record.format.extension())
                            .bind(&record.content).bind(&record.revision)
                            .execute(&mut *transaction).await.map_err(database_error)?;
                    }
                    copied += 1;
                }
                transaction.commit().await.map_err(database_error)?;
                Ok((copied, skipped))
            })
        }).await
    }
}

enum WritePolicy<'a> {
    Copy { overwrite: bool },
    Save { expected_revision: Option<&'a str> },
}

fn conflict(date: &str) -> String {
    format!("{date} 的数据库日报与当前内容冲突，未覆盖数据。请先检查两份内容后再复制或保存。")
}

/// One persistent reactor keeps pooled sockets alive between synchronous workspace calls.
/// The scoped caller thread also makes this safe when invoked from another Tokio runtime.
pub fn run_blocking<T, F, Fut>(operation: F) -> Result<T, String>
where
    T: Send,
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = Result<T, String>>,
{
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| "无法启动数据库运行环境".to_string())
    }).as_ref().map_err(Clone::clone)?;
    std::thread::scope(|scope| {
        scope.spawn(move || runtime.block_on(operation()))
            .join().map_err(|_| "数据库任务未能完成，请重试".to_string())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACE_ONE: &str = "d806e145-799a-4f75-a869-cf99ad0b9d13";
    const WORKSPACE_TWO: &str = "b65c38c8-ab80-4ad8-9d8b-dfbbcf8ad035";

    fn settings() -> Settings {
        Settings { enabled: true, workspace_id: WORKSPACE_ONE.into(), ..Settings::default() }
    }

    #[test]
    fn defaults_to_local_sqlite_and_dual_storage_and_redacts_password() {
        let restored: Settings = serde_json::from_str("{}").unwrap();
        assert!(!restored.enabled);
        assert_eq!(restored.engine, Engine::Sqlite);
        assert_eq!(restored.mode, Mode::Dual);
        assert_eq!(restored.sqlite_path, "workreport.sqlite");
        assert_eq!(restored.default_format, Format::Txt);
        let secret = Settings { password: "do-not-print-this".into(), ..restored };
        assert!(!format!("{secret:?}").contains("do-not-print-this"));
    }

    #[test]
    fn keeps_an_explicit_markdown_default() {
        let restored: Settings = serde_json::from_str(r#"{"defaultFormat":"md"}"#).unwrap();
        assert_eq!(restored.default_format, Format::Md);
    }

    #[test]
    fn defaults_and_clamps_the_history_limit() {
        let restored: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(restored.history_limit_bytes, DEFAULT_HISTORY_LIMIT_BYTES);
        let mut small = settings();
        small.history_limit_bytes = 1;
        assert_eq!(validate(&small).unwrap().history_limit_bytes, MIN_HISTORY_LIMIT_BYTES);
        let mut large = settings();
        large.history_limit_bytes = u64::MAX;
        assert_eq!(validate(&large).unwrap().history_limit_bytes, MAX_HISTORY_LIMIT_BYTES);
    }

    #[test]
    fn validates_active_connection_and_workspace_relative_sqlite_path() {
        for path in ["/tmp/escape.sqlite", "../escape.db", "a/../../escape.sqlite", "workreport.json", "a\\b.sqlite", ".", ""] {
            let mut value = settings();
            value.sqlite_path = path.into();
            if path.is_empty() {
                assert_eq!(validate(&value).unwrap().sqlite_path, "workreport.sqlite");
            } else {
                assert!(validate(&value).is_err(), "{path}");
            }
        }
        let mut value = settings();
        value.engine = Engine::Postgres;
        assert!(validate(&value).unwrap_err().contains("用户名"));
        value.user = "user".into();
        value.host = "postgres://user:secret@localhost/db".into();
        assert!(validate(&value).unwrap_err().contains("连接 URL"));
        value.enabled = false;
        assert!(validate(&value).is_ok());
        assert!(validate_workspace_id("/Users/a/workspace").is_err());
    }

    #[test]
    fn sqlite_persists_reopens_and_isolates_workspaces() {
        let root = tempfile::tempdir().unwrap();
        let first = run_blocking(|| async { Database::connect(root.path(), &settings(), WORKSPACE_ONE).await }).unwrap();
        let record = JournalRecord::new("20261022", Format::Md, "# 工作\n\n完成数据库支持 😀");
        assert_eq!(run_blocking(|| first.upsert_many(std::slice::from_ref(&record), false)).unwrap(), (1, 0));
        drop(first);
        let second = run_blocking(|| async { Database::connect(root.path(), &settings(), WORKSPACE_ONE).await }).unwrap();
        assert_eq!(run_blocking(|| second.read("20261022")).unwrap(), Some(record.clone()));
        let other = run_blocking(|| async { Database::connect(root.path(), &settings(), WORKSPACE_TWO).await }).unwrap();
        assert!(run_blocking(|| other.list()).unwrap().is_empty());
        assert!(root.path().join("workreport.sqlite").is_file());
    }

    #[test]
    fn batch_conflict_rolls_back_earlier_writes_and_identical_rows_are_skipped() {
        let root = tempfile::tempdir().unwrap();
        let database = run_blocking(|| async { Database::connect(root.path(), &settings(), WORKSPACE_ONE).await }).unwrap();
        let original = JournalRecord::new("20261022", Format::Txt, "原文");
        run_blocking(|| database.upsert_many(std::slice::from_ref(&original), false)).unwrap();
        assert_eq!(run_blocking(|| database.upsert_many(std::slice::from_ref(&original), false)).unwrap(), (0, 1));
        let records = vec![JournalRecord::new("20261021", Format::Md, "先写这条"), JournalRecord::new("20261022", Format::Txt, "冲突内容")];
        assert!(run_blocking(|| database.upsert_many(&records, false)).unwrap_err().contains("冲突"));
        assert_eq!(run_blocking(|| database.list()).unwrap(), vec![original]);
        assert_eq!(run_blocking(|| database.upsert_many(&records, true)).unwrap(), (2, 0));
        assert_eq!(run_blocking(|| database.list()).unwrap(), records);
    }

    #[test]
    fn editor_save_requires_current_revision_and_can_change_format() {
        let root = tempfile::tempdir().unwrap();
        let database = run_blocking(|| async { Database::connect(root.path(), &settings(), WORKSPACE_ONE).await }).unwrap();
        let original = JournalRecord::new("20261022", Format::Md, "原文");
        run_blocking(|| database.save(&original, None)).unwrap();
        let changed = JournalRecord::new("20261022", Format::Txt, "新内容");
        assert!(run_blocking(|| database.save(&changed, None)).is_err());
        assert!(run_blocking(|| database.save(&changed, Some("wrong-revision"))).is_err());
        run_blocking(|| database.save(&changed, Some(&original.revision))).unwrap();
        assert_eq!(run_blocking(|| database.read("20261022")).unwrap(), Some(changed));
    }

    #[test]
    fn rejects_invalid_snapshot_before_any_write_and_detects_external_corruption() {
        let root = tempfile::tempdir().unwrap();
        let database = run_blocking(|| async { Database::connect(root.path(), &settings(), WORKSPACE_ONE).await }).unwrap();
        let valid = JournalRecord::new("20261022", Format::Md, "原文");
        let invalid = JournalRecord::new("20260230", Format::Md, "无效日期");
        assert!(run_blocking(|| async { database.upsert_many(&[valid.clone(), invalid], false).await }).is_err());
        assert!(run_blocking(|| database.list()).unwrap().is_empty());
        assert!(run_blocking(|| async { database.upsert_many(&[valid.clone(), valid.clone()], false).await }).is_err());
        run_blocking(|| database.save(&valid, None)).unwrap();
        run_blocking(|| async {
            if let Pool::Sqlite(pool) = &database.pool {
                sqlx::query("UPDATE workreport_journal SET content = 'tampered'").execute(pool).await.map_err(database_error)?;
            }
            Ok(())
        }).unwrap();
        assert!(run_blocking(|| database.list()).unwrap_err().contains("校验失败"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_sqlite_symlink_escaping_workspace() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        let settings = Settings { sqlite_path: "linked/data.sqlite".into(), ..settings() };
        assert!(run_blocking(|| Database::connect(root.path(), &settings, WORKSPACE_ONE)).unwrap_err().contains("符号链接"));
        assert!(!outside.path().join("data.sqlite").exists());
    }

    #[test]
    fn blocking_bridge_is_safe_inside_an_existing_tokio_runtime() {
        let root = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let database = run_blocking(|| async { Database::connect(root.path(), &settings(), WORKSPACE_ONE).await }).unwrap();
            assert!(run_blocking(|| database.list()).unwrap().is_empty());
        });
    }
}

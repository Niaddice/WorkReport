//! Machine-local encrypted Git credentials. The encryption key never enters the config file.
use crate::git_auth::Credentials;
use ring::{aead, rand::{SecureRandom, SystemRandom}};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::{ErrorKind, Write}, path::{Path, PathBuf}, sync::{Arc, Mutex}};
use tempfile::NamedTempFile;
use zeroize::Zeroizing;

pub const FILE_NAME: &str = "git-credentials.json";
const AAD: &[u8] = b"com.workreport.desktop/git-credentials/v1";
const MAX_FILE_BYTES: usize = 1024 * 1024;
static STORE_LOCK: Mutex<()> = Mutex::new(());

trait KeyProvider {
    fn read(&self) -> Result<Option<[u8; 32]>, String>;
    fn write(&self, key: &[u8; 32]) -> Result<(), String>;
}

struct SystemKey;

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn key_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.workreport.desktop.git-credentials", "encryption-key-v1")
        .map_err(|_| "无法访问系统安全存储，凭据未保存".into())
}

impl KeyProvider for SystemKey {
    fn read(&self) -> Result<Option<[u8; 32]>, String> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            match key_entry()?.get_secret() {
                Ok(bytes) => {
                    let bytes = Zeroizing::new(bytes);
                    let key: [u8; 32] = bytes.as_slice().try_into().map_err(|_| "系统中的 Git 加密密钥无效")?;
                    Ok(Some(key))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err("无法读取系统安全存储，请解锁后重试".into()),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        Err("当前系统不支持安全保存 Git 凭据".into())
    }

    fn write(&self, key: &[u8; 32]) -> Result<(), String> {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        { key_entry()?.set_secret(key).map_err(|_| "无法保存系统加密密钥，凭据未保存".into()) }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        { let _ = key; Err("当前系统不支持安全保存 Git 凭据".into()) }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Serialize)]
pub struct Metadata {
    pub url: String,
    pub username: String,
}

struct Vault<'a> {
    directory: &'a Path,
    keys: &'a dyn KeyProvider,
}

impl Vault<'_> {
    fn records(&self) -> Result<BTreeMap<String, Credentials>, String> {
        let path = self.directory.join(FILE_NAME);
        let size = match fs::metadata(&path) {
            Ok(value) => value.len(),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(_) => return Err("无法读取已保存的 Git 凭据".into()),
        };
        if size > MAX_FILE_BYTES as u64 { return Err("Git 凭据文件过大，请清除后重新保存".into()); }
        let envelope: Envelope = serde_json::from_slice(&fs::read(path).map_err(|_| "无法读取 Git 凭据文件")?)
            .map_err(|_| "Git 凭据文件损坏，请清除后重新保存")?;
        if envelope.version != 1 { return Err("不支持此 Git 凭据文件版本".into()); }
        let key = Zeroizing::new(self.keys.read()?.ok_or("找不到系统加密密钥，无法解密 Git 凭据")?);
        let nonce: [u8; 12] = hex::decode(envelope.nonce).map_err(|_| "Git 凭据文件损坏")?
            .try_into().map_err(|_| "Git 凭据文件损坏")?;
        let mut ciphertext = Zeroizing::new(hex::decode(envelope.ciphertext).map_err(|_| "Git 凭据文件损坏")?);
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, key.as_ref()).map_err(|_| "Git 加密密钥无效")?);
        let plaintext = key.open_in_place(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(AAD), &mut ciphertext)
            .map_err(|_| "Git 凭据解密失败，文件或系统密钥已变化")?;
        let records: BTreeMap<String, Credentials> = serde_json::from_slice(plaintext).map_err(|_| "Git 凭据内容无效")?;
        if records.len() > 100 { return Err("保存的 Git 服务器数量超过限制".into()); }
        for (origin, value) in &records {
            value.validate()?;
            if origin != &value.url { return Err("Git 凭据服务器地址不匹配".into()); }
        }
        Ok(records)
    }

    fn persist(&self, records: &BTreeMap<String, Credentials>) -> Result<(), String> {
        if records.len() > 100 { return Err("最多保存 100 个 Git 服务器的凭据".into()); }
        let key = match self.keys.read()? {
            Some(value) => Zeroizing::new(value),
            None => {
                let mut key = Zeroizing::new([0; 32]);
                SystemRandom::new().fill(key.as_mut()).map_err(|_| "无法生成 Git 加密密钥")?;
                self.keys.write(&key)?;
                key
            }
        };
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, key.as_ref()).map_err(|_| "Git 加密密钥无效")?);
        let mut nonce = [0; 12];
        SystemRandom::new().fill(&mut nonce).map_err(|_| "无法生成 Git 加密随机数")?;
        let mut payload = Zeroizing::new(serde_json::to_vec(records).map_err(|_| "无法序列化 Git 凭据")?);
        key.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(AAD), &mut *payload)
            .map_err(|_| "无法加密 Git 凭据")?;
        let bytes = serde_json::to_vec_pretty(&Envelope { version: 1, nonce: hex::encode(nonce), ciphertext: hex::encode(&*payload) })
            .map_err(|_| "无法序列化加密凭据")?;
        if bytes.len() > MAX_FILE_BYTES { return Err("Git 凭据过多或过长，原文件保持不变".into()); }
        fs::create_dir_all(self.directory).map_err(|_| "无法创建本机凭据目录")?;
        let mut file = NamedTempFile::new_in(self.directory).map_err(|_| "无法创建凭据临时文件")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file().set_permissions(fs::Permissions::from_mode(0o600)).map_err(|_| "无法设置凭据文件权限")?;
        }
        file.write_all(&bytes).map_err(|_| "无法写入加密凭据")?;
        file.as_file().sync_all().map_err(|_| "无法同步加密凭据")?;
        file.persist(self.directory.join(FILE_NAME)).map_err(|_| "无法保存凭据，原文件保持不变")?;
        Ok(())
    }

    fn save(&self, credentials: &Credentials) -> Result<(), String> {
        credentials.validate()?;
        let mut records = self.records()?;
        let mut value = credentials.clone();
        value.url = reqwest::Url::parse(&value.url).map_err(|_| "Git 认证地址无效")?.origin().ascii_serialization();
        value.save = false;
        records.insert(value.url.clone(), value);
        self.persist(&records)
    }

    fn forget(&self, origin: Option<&str>) -> Result<(), String> {
        if let Some(origin) = origin {
            let mut records = self.records()?;
            if records.remove(origin).is_none() { return Ok(()); }
            if !records.is_empty() { return self.persist(&records); }
        }
        match fs::remove_file(self.directory.join(FILE_NAME)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(_) => Err("无法清除本机 Git 凭据".into()),
        }
    }
}

fn with_vault<T>(directory: &Path, action: impl FnOnce(Vault<'_>) -> Result<T, String>) -> Result<T, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "Git 凭据存储正忙，请重试")?;
    fs::create_dir_all(directory).map_err(|_| "无法创建本机凭据目录")?;
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Serialize independent app processes before reading the key or replacing the file.
    let lock = options.open(directory.join("git-credentials.lock")).map_err(|_| "无法锁定 Git 凭据文件")?;
    lock.try_lock().map_err(|_| "其他应用实例正在处理 Git 凭据，请稍后重试")?;
    action(Vault { directory, keys: &SystemKey })
}

pub fn load(directory: &Path, origin: &str) -> Result<Option<Credentials>, String> {
    with_vault(directory, |vault| Ok(vault.records()?.remove(origin)))
}

pub fn save(directory: &Path, credentials: &Credentials) -> Result<(), String> {
    with_vault(directory, |vault| vault.save(credentials))
}

pub fn list(directory: &Path) -> Result<Vec<Metadata>, String> {
    with_vault(directory, |vault| Ok(vault.records()?.values().map(|value| Metadata { url: value.url.clone(), username: value.username.clone() }).collect()))
}

pub fn forget(directory: &Path, origin: Option<&str>) -> Result<(), String> {
    with_vault(directory, |vault| vault.forget(origin))
}

pub trait CredentialStorage: Send + Sync {
    fn load(&self, origin: &str) -> Result<Option<Credentials>, String>;
    fn save(&self, credentials: &Credentials) -> Result<(), String>;
}

pub struct LocalStore(pub Result<PathBuf, String>);
impl CredentialStorage for LocalStore {
    fn load(&self, origin: &str) -> Result<Option<Credentials>, String> {
        load(self.0.as_ref().map_err(|_| "无法访问本机凭据目录")?, origin)
    }
    fn save(&self, credentials: &Credentials) -> Result<(), String> {
        save(self.0.as_ref().map_err(|_| "无法访问本机凭据目录")?, credentials)
    }
}

/// Retry only a failed network action, and persist only an explicitly selected successful login.
pub async fn execute<F, Fut>(
    mut request: crate::git_client::Action,
    storage: Arc<dyn CredentialStorage>,
    mut run: F,
) -> Result<String, String>
where
    F: FnMut(crate::git_client::Action) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let remember = request.credentials.as_ref().filter(|value| value.save).cloned();
    let mut result = run(request.clone()).await;
    if request.credentials.is_none() && matches!(request.operation.as_str(), "pull" | "push") {
        if let Some(origin) = result.as_ref().err().and_then(|error| crate::git_auth::authentication_origin(error)) {
            let store = storage.clone();
            let saved = tokio::task::spawn_blocking(move || store.load(&origin)).await
                .unwrap_or_else(|_| Err("读取本机 Git 凭据失败".into()));
            match saved {
                Ok(Some(mut value)) => {
                    value.save = false;
                    request.credentials = Some(value);
                    result = run(request).await;
                }
                Ok(None) => {}
                Err(error) => if let Err(message) = &mut result {
                    message.push_str(&format!("\n已保存凭据不可用：{error}"));
                },
            }
        }
    }
    if let (Ok(output), Some(credentials)) = (&mut result, remember) {
        let saved = tokio::task::spawn_blocking(move || storage.save(&credentials)).await;
        match saved {
            Ok(Ok(())) => output.push_str("\nGit 凭据已加密保存到本机。"),
            Ok(Err(error)) => output.push_str(&format!("\nGit 操作已完成，但凭据未能保存：{error}")),
            Err(_) => output.push_str("\nGit 操作已完成，但凭据未能保存，请重试。"),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct Keys(RefCell<Option<[u8; 32]>>);
    impl KeyProvider for Keys {
        fn read(&self) -> Result<Option<[u8; 32]>, String> { Ok(*self.0.borrow()) }
        fn write(&self, key: &[u8; 32]) -> Result<(), String> { *self.0.borrow_mut() = Some(*key); Ok(()) }
    }
    fn credentials(url: &str) -> Credentials {
        Credentials { url: url.into(), username: "private-user".into(), password: "private-token".into(), save: true }
    }
    #[test]
    fn encrypted_file_round_trips_across_restart_without_exposing_credentials_or_key() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::default();
        let vault = Vault { directory: dir.path(), keys: &keys };
        vault.save(&credentials("https://git.example.test")).unwrap();
        let bytes = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        for secret in ["private-user", "private-token", "git.example.test", &hex::encode(keys.0.borrow().unwrap())] {
            assert!(!bytes.contains(secret));
        }
        let reopened = Vault { directory: dir.path(), keys: &keys };
        let values = reopened.records().unwrap();
        assert_eq!(values["https://git.example.test"].password, "private-token");
        assert!(!values["https://git.example.test"].save);
        vault.save(&credentials("https://git.example.test")).unwrap();
        assert_ne!(bytes, fs::read_to_string(dir.path().join(FILE_NAME)).unwrap());
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(dir.path().join(FILE_NAME)).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
    #[test]
    fn rejects_tampering_wrong_keys_and_never_overwrites_unreadable_saved_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::default();
        let vault = Vault { directory: dir.path(), keys: &keys };
        vault.save(&credentials("https://git.example.test")).unwrap();
        let path = dir.path().join(FILE_NAME);
        let original = fs::read(&path).unwrap();
        let wrong_keys = Keys(RefCell::new(Some([42; 32])));
        let wrong = Vault { directory: dir.path(), keys: &wrong_keys };
        assert!(wrong.records().is_err());
        assert!(wrong.save(&credentials("https://other.example.test")).is_err());
        assert_eq!(original, fs::read(&path).unwrap());
        let missing_keys = Keys::default();
        let missing = Vault { directory: dir.path(), keys: &missing_keys };
        assert!(missing.save(&credentials("https://other.example.test")).is_err());
        assert_eq!(original, fs::read(&path).unwrap());
        let mut oversized = vault.records().unwrap();
        for index in 0..70 {
            let mut value = credentials(&format!("https://server{index}.example.test"));
            value.password = "x".repeat(8192);
            oversized.insert(value.url.clone(), value);
        }
        assert!(vault.persist(&oversized).is_err());
        assert_eq!(original, fs::read(&path).unwrap());
        let mut envelope: Envelope = serde_json::from_slice(&original).unwrap();
        envelope.ciphertext.replace_range(0..2, if envelope.ciphertext.starts_with("00") { "01" } else { "00" });
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(vault.records().is_err());
        vault.forget(None).unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn stores_separate_origins_and_removes_only_the_selected_server() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::default();
        let vault = Vault { directory: dir.path(), keys: &keys };
        vault.save(&credentials("https://git.example.test")).unwrap();
        vault.save(&credentials("http://git.example.test:13000")).unwrap();
        assert_eq!(vault.records().unwrap().len(), 2);
        vault.forget(Some("https://git.example.test")).unwrap();
        let records = vault.records().unwrap();
        assert_eq!(records.len(), 1);
        assert!(records.contains_key("http://git.example.test:13000"));
        vault.forget(Some("http://git.example.test:13000")).unwrap();
        assert!(!dir.path().join(FILE_NAME).exists());
    }
    #[test]
    fn unavailable_system_key_never_falls_back_to_plaintext_storage() {
        struct Locked;
        impl KeyProvider for Locked {
            fn read(&self) -> Result<Option<[u8; 32]>, String> { Ok(None) }
            fn write(&self, _: &[u8; 32]) -> Result<(), String> { Err("系统安全存储未解锁".into()) }
        }
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault { directory: dir.path(), keys: &Locked };
        assert!(vault.save(&credentials("https://git.example.test")).is_err());
        assert!(!dir.path().join(FILE_NAME).exists());
    }

    #[derive(Default)]
    struct MemoryStore {
        entries: Mutex<BTreeMap<String, Credentials>>,
        reads: Mutex<Vec<String>>,
        writes: Mutex<usize>,
        fail_save: bool,
    }
    impl CredentialStorage for MemoryStore {
        fn load(&self, origin: &str) -> Result<Option<Credentials>, String> {
            self.reads.lock().unwrap().push(origin.into());
            Ok(self.entries.lock().unwrap().get(origin).cloned())
        }
        fn save(&self, credentials: &Credentials) -> Result<(), String> {
            *self.writes.lock().unwrap() += 1;
            if self.fail_save { return Err("系统安全存储未解锁".into()); }
            self.entries.lock().unwrap().insert(credentials.url.clone(), credentials.clone());
            Ok(())
        }
    }
    fn action(credentials: Option<Credentials>) -> crate::git_client::Action {
        crate::git_client::Action { operation: "push".into(), paths: vec![], message: String::new(), reference: String::new(), credentials }
    }
    #[tokio::test]
    async fn remembers_only_checked_successful_logins_and_keeps_push_success_when_saving_fails() {
        let store = Arc::new(MemoryStore::default());
        let mut once = credentials("https://git.example.test");
        once.save = false;
        execute(action(Some(once)), store.clone(), |_| async { Ok("pushed".into()) }).await.unwrap();
        assert_eq!(*store.writes.lock().unwrap(), 0);
        assert!(execute(action(Some(credentials("https://git.example.test"))), store.clone(), |_| async { Err("authentication failed".into()) }).await.is_err());
        assert_eq!(*store.writes.lock().unwrap(), 0);
        execute(action(Some(credentials("https://git.example.test"))), store.clone(), |_| async { Ok("pushed".into()) }).await.unwrap();
        assert_eq!(*store.writes.lock().unwrap(), 1);
        let locked = Arc::new(MemoryStore { fail_save: true, ..Default::default() });
        let output = execute(action(Some(credentials("https://git.example.test"))), locked, |_| async { Ok("pushed".into()) }).await.unwrap();
        assert!(output.contains("pushed"));
        assert!(output.contains("凭据未能保存"));
    }
    #[tokio::test]
    async fn reuses_only_the_failed_origin_and_retries_an_invalid_cached_password_once() {
        let store = Arc::new(MemoryStore::default());
        store.entries.lock().unwrap().insert("https://git.example.test".into(), credentials("https://git.example.test"));
        let calls = Mutex::new(Vec::new());
        let output = execute(action(None), store.clone(), |request| {
            calls.lock().unwrap().push(request.operation.clone());
            async move {
                if let Some(value) = request.credentials {
                    assert_eq!(value.password, "private-token");
                    Ok("pushed with saved credentials".into())
                } else { Err("fatal: could not read Username for 'https://git.example.test': terminal prompts disabled".into()) }
            }
        }).await.unwrap();
        assert!(output.contains("pushed"));
        assert_eq!(*calls.lock().unwrap(), vec!["push", "push"]);
        assert_eq!(*store.writes.lock().unwrap(), 0);
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let failed = execute(action(None), store.clone(), |_| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Err("fatal: Authentication failed for 'https://git.example.test/repo.git/'".into()) }
        }).await;
        assert!(failed.is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(store.reads.lock().unwrap().iter().all(|origin| origin == "https://git.example.test"));
    }
}

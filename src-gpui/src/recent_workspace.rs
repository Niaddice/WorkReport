use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use tempfile::NamedTempFile;

const APPLICATION_DIRECTORY: &str = "com.workreport.gpui-prototype";
const FILE_NAME: &str = "workspace.json";

pub(crate) struct RecentWorkspace {
    directory: PathBuf,
}

impl RecentWorkspace {
    pub(crate) fn in_config_dir() -> Result<Self, String> {
        let directory = dirs::config_dir().ok_or("无法确定应用配置目录")?;
        Ok(Self::in_directory(directory.join(APPLICATION_DIRECTORY)))
    }

    pub(crate) fn in_directory(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub(crate) fn load(&self) -> Result<Option<PathBuf>, String> {
        let bytes = match fs::read(self.directory.join(FILE_NAME)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("无法读取上次工作空间：{error}")),
        };
        let path: PathBuf = serde_json::from_slice(&bytes)
            .map_err(|_| "上次工作空间记录无效，请重新选择工作空间".to_string())?;
        if !path.is_absolute() {
            return Err("上次工作空间路径无效，请重新选择工作空间".into());
        }
        Ok(Some(path))
    }

    pub(crate) fn save(&self, path: &Path) -> Result<(), String> {
        if !path.is_absolute() {
            return Err("无法记录相对路径，请先打开工作空间".into());
        }
        let bytes =
            serde_json::to_vec(path).map_err(|_| "工作空间路径含有无法记录的字符".to_string())?;
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("无法创建应用配置目录：{error}"))?;
        let mut file = NamedTempFile::new_in(&self.directory)
            .map_err(|error| format!("无法创建工作空间记录：{error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("无法写入工作空间记录：{error}"))?;
        file.as_file()
            .sync_all()
            .map_err(|error| format!("无法同步工作空间记录：{error}"))?;
        file.persist(self.directory.join(FILE_NAME))
            .map_err(|error| format!("无法记录上次工作空间：{}", error.error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_only_the_prototype_record_and_round_trips_unicode_paths() {
        let config = tempfile::tempdir().unwrap();
        let tauri_directory = config.path().join("com.workreport.desktop");
        fs::create_dir(&tauri_directory).unwrap();
        fs::write(
            tauri_directory.join(FILE_NAME),
            br#""/existing-tauri-workspace""#,
        )
        .unwrap();
        let directory = config.path().join(APPLICATION_DIRECTORY);
        let recent = RecentWorkspace::in_directory(directory.clone());

        assert_eq!(recent.load().unwrap(), None);
        assert!(!directory.exists());
        let workspace = config.path().join("中文工作空间 📝");
        recent.save(&workspace).unwrap();
        assert_eq!(recent.load().unwrap(), Some(workspace));
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert_eq!(
            fs::read(tauri_directory.join(FILE_NAME)).unwrap(),
            br#""/existing-tauri-workspace""#
        );
    }

    #[test]
    fn rejects_invalid_records_and_preserves_previous_record_on_invalid_save() {
        let directory = tempfile::tempdir().unwrap();
        let recent = RecentWorkspace::in_directory(directory.path().to_owned());
        let file = directory.path().join(FILE_NAME);
        for invalid in ["not json", r#""relative/path""#, "null", "{}"] {
            fs::write(&file, invalid).unwrap();
            assert!(recent.load().is_err());
        }

        let workspace = directory.path().join("valid-workspace");
        recent.save(&workspace).unwrap();
        let before = fs::read(&file).unwrap();
        assert!(recent.save(Path::new("relative/path")).is_err());
        assert_eq!(fs::read(&file).unwrap(), before);

        let second = directory.path().join("second-workspace");
        recent.save(&second).unwrap();
        assert_eq!(recent.load().unwrap(), Some(second));
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_paths_never_replace_the_previous_record_lossily() {
        use std::os::unix::ffi::OsStringExt;

        let directory = tempfile::tempdir().unwrap();
        let recent = RecentWorkspace::in_directory(directory.path().to_owned());
        let original = directory.path().join("existing-workspace");
        recent.save(&original).unwrap();
        let non_unicode = directory
            .path()
            .join(std::ffi::OsString::from_vec(vec![0xff]));
        assert!(recent.save(&non_unicode).is_err());
        assert_eq!(recent.load().unwrap(), Some(original));
    }

    #[test]
    fn failed_atomic_publication_leaves_the_existing_destination_intact() {
        let directory = tempfile::tempdir().unwrap();
        let recent = RecentWorkspace::in_directory(directory.path().to_owned());
        let destination = directory.path().join(FILE_NAME);
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep"), "original data").unwrap();

        assert!(recent.save(&directory.path().join("workspace")).is_err());
        assert_eq!(
            fs::read_to_string(destination.join("keep")).unwrap(),
            "original data"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

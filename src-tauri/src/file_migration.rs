use crate::file_rules::{self, Feature, Settings};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

const FEATURES: [Feature; 3] = [Feature::Journal, Feature::Excel, Feature::Summary];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryChange {
    feature: Feature,
    label: String,
    from: String,
    to: String,
    file_count: usize,
    target_file_count: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilenameChange {
    feature: Feature,
    label: String,
    from: String,
    to: String,
    file_count: usize,
    unrecognized_count: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub token: String,
    pub directories: Vec<DirectoryChange>,
    pub filenames: Vec<FilenameChange>,
    pub warnings: Vec<String>,
}
impl Plan {
    fn needs_confirmation(&self) -> bool {
        self.directories
            .iter()
            .any(|change| change.file_count > 0 || change.target_file_count > 0)
            || !self.filenames.is_empty()
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Confirmation {
    pub token: String,
    #[serde(default)]
    pub rename_existing: BTreeMap<Feature, bool>,
}

// Inventory includes unknown files: directory renaming moves the whole directory.
// Links are rejected so neither inspection nor migration can leave the workspace.
fn inventory(path: &Path, hash: &mut Sha256) -> Result<usize, String> {
    hash.update(path.to_string_lossy().as_bytes());
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            hash.update(b"missing");
            return Ok(0);
        }
        Err(error) => return Err(format!("无法检查目录内容：{error}")),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("目录和文件不能使用符号链接：{}", path.display()));
    }
    if metadata.is_dir() {
        hash.update(b"directory");
        let mut entries = fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort();
        let mut count = 0;
        for entry in entries {
            count += inventory(&entry, hash)?;
        }
        Ok(count)
    } else if metadata.is_file() {
        hash.update(b"file");
        hash.update(metadata.len().to_le_bytes());
        let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut buffer = [0u8; 32768];
        loop {
            let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if size == 0 {
                break;
            }
            hash.update(&buffer[..size]);
        }
        Ok(1)
    } else {
        Err(format!("保存目录包含不支持的文件：{}", path.display()))
    }
}

fn changed_directory(old: &Settings, new: &Settings, feature: Feature) -> bool {
    old.rule(feature).directory != new.rule(feature).directory
}
fn source_directory(root: &Path, old: &Settings, feature: Feature) -> Result<PathBuf, String> {
    file_rules::safe_directory(root, &old.rule(feature).directory, false)
}

fn records_for_change(
    root: &Path,
    old: &Settings,
    new: &Settings,
) -> Result<Vec<file_rules::ManagedFile>, String> {
    let mut records = BTreeMap::new();
    for record in file_rules::managed_records(root, old)? {
        let key = fs::canonicalize(root.join(&record.relative_path))
            .map_err(|error| error.to_string())?;
        records.insert(key, record);
    }
    // Also recognize files in an adopted existing directory using the previous
    // naming rule, so choosing 'new files only' never orphans a custom journal.
    for feature in FEATURES {
        if !changed_directory(old, new, feature) {
            continue;
        }
        let mut candidate = old.clone();
        let rule = match feature {
            Feature::Journal => &mut candidate.journal,
            Feature::Excel => &mut candidate.excel,
            Feature::Summary => &mut candidate.summary,
        };
        rule.directory = new.rule(feature).directory.clone();
        for record in file_rules::managed_records(root, &candidate)?
            .into_iter()
            .filter(|record| record.feature == feature)
        {
            let key = fs::canonicalize(root.join(&record.relative_path))
                .map_err(|error| error.to_string())?;
            records.entry(key).or_insert(record);
        }
    }
    Ok(records.into_values().collect())
}

pub fn preview(root: &Path, old: &Settings, proposed: &Settings) -> Result<Plan, String> {
    let new = file_rules::normalize(root, proposed)?;
    let records = records_for_change(root, old, &new)?;
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&(old, &new)).map_err(|e| e.to_string())?);
    inventory(&root.join(crate::settings::FILE_NAME), &mut hash)?;
    inventory(&root.join(file_rules::CATALOG_FILE), &mut hash)?;
    let mut plan = Plan {
        token: String::new(),
        directories: Vec::new(),
        filenames: Vec::new(),
        warnings: Vec::new(),
    };
    for feature in FEATURES {
        let before = old.rule(feature);
        let after = new.rule(feature);
        let files: Vec<_> = records
            .iter()
            .filter(|record| record.feature == feature)
            .collect();
        let directory = source_directory(root, old, feature)?;
        let count = if before.directory.is_empty() {
            for record in &files {
                inventory(
                    &file_rules::safe_file(root, &record.relative_path)?,
                    &mut hash,
                )?;
            }
            files
                .iter()
                .filter(|record| {
                    !changed_directory(old, &new, feature)
                        || new.rule(feature).directory.is_empty()
                        || !Path::new(&record.relative_path)
                            .starts_with(&new.rule(feature).directory)
                })
                .count()
        } else {
            inventory(&directory, &mut hash)?
        };
        if changed_directory(old, &new, feature) {
            let target = file_rules::safe_directory(root, &after.directory, false)?;
            let target_file_count =
                if !after.directory.is_empty() && !same_directory(&directory, &target) {
                    inventory(&target, &mut hash)?
                } else {
                    0
                };
            // Adopting an existing directory is fine when there is no source.
            // Never merge two populated directories or replace user contents.
            if !before.directory.is_empty()
                && directory.exists()
                && target.exists()
                && target != root
                && !same_directory(&directory, &target)
                && fs::read_dir(&directory)
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_some()
                && fs::read_dir(&target)
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_some()
            {
                return Err(format!(
                    "{}目标目录已有内容，请使用其他名称，避免覆盖或合并文件",
                    feature.label()
                ));
            }
            plan.directories.push(DirectoryChange {
                feature,
                label: feature.label().into(),
                from: before.directory.clone(),
                to: after.directory.clone(),
                file_count: if after.directory.is_empty() {
                    files.len()
                } else {
                    count
                },
                target_file_count,
            });
            if before.directory.is_empty() {
                plan.warnings.push(
                    "日报原目录为工作空间根目录：仅迁移已识别的日报，其他文件和旧年份空目录保留。"
                        .into(),
                );
            }
            if after.directory.is_empty() {
                plan.warnings.push(
                    "日报恢复到工作空间根目录：仅迁移已识别的日报，原目录中的其他文件保留。".into(),
                );
            }
        }
        if before.filename_template != after.filename_template && (count > 0 || !files.is_empty()) {
            let known_in_directory = files
                .iter()
                .filter(|record| {
                    before.directory.is_empty()
                        || Path::new(&record.relative_path).starts_with(&before.directory)
                })
                .count();
            let unrecognized_count = count.saturating_sub(known_in_directory);
            plan.filenames.push(FilenameChange {
                feature,
                label: feature.label().into(),
                from: before.filename_template.clone(),
                to: after.filename_template.clone(),
                file_count: files.len(),
                unrecognized_count,
            });
        }
    }
    // Catalogued files may be outside the current configured directory after a
    // prior 'new files only' choice. Include them in stale-plan protection too.
    for record in records {
        inventory(
            &file_rules::safe_file(root, &record.relative_path)?,
            &mut hash,
        )?;
    }
    plan.token = format!("{:x}", hash.finalize());
    Ok(plan)
}
fn same_directory(a: &Path, b: &Path) -> bool {
    a == b || matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

enum Undo {
    Move(PathBuf, PathBuf),
    CreatedDirectory(PathBuf),
    RemovedDirectory(PathBuf),
}
fn mkdir(path: &Path, undo: &mut Vec<Undo>) -> Result<(), String> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        mkdir(parent, undo)?;
    }
    fs::create_dir(path).map_err(|e| format!("无法创建目录 {}：{e}", path.display()))?;
    undo.push(Undo::CreatedDirectory(path.into()));
    Ok(())
}
fn move_without_overwrite(from: &Path, to: &Path) -> Result<(), String> {
    match fs::symlink_metadata(to) {
        Ok(_) => return Err(format!("目标已存在，未覆盖：{}", to.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    if from.is_dir() {
        fs::rename(from, to).map_err(|e| format!("无法移动目录：{e}"))
    } else {
        fs::hard_link(from, to).map_err(|e| format!("无法移动文件：{e}"))?;
        if let Err(error) = fs::remove_file(from) {
            let _ = fs::remove_file(to);
            return Err(error.to_string());
        }
        Ok(())
    }
}
fn move_recorded(from: &Path, to: &Path, undo: &mut Vec<Undo>) -> Result<(), String> {
    mkdir(to.parent().ok_or("目标目录无效")?, undo)?;
    move_without_overwrite(from, to)?;
    undo.push(Undo::Move(to.into(), from.into()));
    Ok(())
}
fn rollback(undo: &mut Vec<Undo>) -> Result<(), String> {
    let mut errors = Vec::new();
    while let Some(operation) = undo.pop() {
        let result = match operation {
            Undo::Move(from, to) => move_without_overwrite(&from, &to),
            Undo::CreatedDirectory(path) => fs::remove_dir(path).map_err(|e| e.to_string()),
            Undo::RemovedDirectory(path) => fs::create_dir(path).map_err(|e| e.to_string()),
        };
        if let Err(error) = result {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}
fn bytes_if_exists(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            Err("设置或索引不是普通文件".into())
        }
        Ok(_) => fs::read(path).map(Some).map_err(|e| e.to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
fn restore(path: &Path, bytes: &Option<Vec<u8>>) -> Result<(), String> {
    if let Some(bytes) = bytes {
        file_rules::atomic_bytes(path, bytes)
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

/// The catalog and settings are committed inside the same rollback scope as
/// filesystem moves. The caller validates database/AI configuration beforehand.
pub fn apply(
    root: &Path,
    old: &Settings,
    new: &Settings,
    confirmation: Option<&Confirmation>,
    persist: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if old == new {
        return persist();
    }
    let plan = preview(root, old, new)?;
    if let Some(confirmation) = confirmation {
        if confirmation.token != plan.token {
            return Err("目录内容或设置已变化，请重新保存并确认".into());
        }
    } else if plan.needs_confirmation() {
        return Err("目录或文件名修改涉及已有文件，请先预览并确认处理方式".into());
    }
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let settings_path = root.join(crate::settings::FILE_NAME);
    let catalog_path = root.join(file_rules::CATALOG_FILE);
    let settings_bytes = bytes_if_exists(&settings_path)?;
    let catalog_bytes = bytes_if_exists(&catalog_path)?;
    let mut records = records_for_change(&root, old, new)?;
    // Keep missing catalog records as well, e.g. database backup state. Existing
    // records take precedence and are updated to their final paths below.
    let mut missing = file_rules::catalog(&root)?;
    missing.retain(|record| !root.join(&record.relative_path).is_file());
    let stage = tempfile::Builder::new()
        .prefix(".workreport-migration-")
        .tempdir_in(&root)
        .map_err(|e| e.to_string())?
        .keep();
    let mut undo = Vec::new();
    let mut metadata_written = false;
    let result = (|| {
        let mut moved_directories = Vec::<(PathBuf, PathBuf, PathBuf)>::new();
        for feature in FEATURES {
            let from = &old.rule(feature).directory;
            let to = &new.rule(feature).directory;
            if from == to || from.is_empty() {
                continue;
            }
            let source = file_rules::safe_directory(&root, from, false)?;
            let target = file_rules::safe_directory(&root, to, false)?;
            if !source.exists() {
                continue;
            }
            if target.is_dir()
                && !same_directory(&source, &target)
                && fs::read_dir(&source)
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_none()
                && fs::read_dir(&target)
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_some()
            {
                fs::remove_dir(&source).map_err(|e| e.to_string())?;
                undo.push(Undo::RemovedDirectory(source));
                continue;
            }
            // Returning journals to workspace root moves managed files only.
            if to.is_empty() {
                continue;
            }
            let staged = stage.join(format!("directory-{}", moved_directories.len()));
            move_recorded(&source, &staged, &mut undo)?;
            moved_directories.push((source, target, staged));
        }
        for (_, target, staged) in &moved_directories {
            if target.exists() {
                fs::remove_dir(target).map_err(|_| "目标目录已有内容，已取消修改".to_string())?;
                undo.push(Undo::RemovedDirectory(target.clone()));
            }
            move_recorded(staged, target, &mut undo)?;
        }
        let mut pending = Vec::<(PathBuf, PathBuf)>::new();
        let mut reserved = BTreeSet::new();
        for (index, record) in records.iter_mut().enumerate() {
            let original = file_rules::safe_file(&root, &record.relative_path)?;
            let source = moved_directories
                .iter()
                .find_map(|(from, to, _)| {
                    original
                        .strip_prefix(from)
                        .ok()
                        .map(|relative| to.join(relative))
                })
                .unwrap_or(original);
            let extension = source
                .extension()
                .and_then(|value| value.to_str())
                .ok_or("文件格式无效")?;
            let rename = old.rule(record.feature).filename_template
                != new.rule(record.feature).filename_template
                && confirmation
                    .and_then(|decision| decision.rename_existing.get(&record.feature))
                    .copied()
                    .unwrap_or(false);
            let directory_changed = changed_directory(old, new, record.feature);
            let parent = if directory_changed {
                let base =
                    file_rules::safe_directory(&root, &new.rule(record.feature).directory, false)?;
                if record.feature == Feature::Journal {
                    base.join(&record.date[..4])
                } else {
                    base
                }
            } else {
                source.parent().ok_or("文件目录无效")?.into()
            };
            let name = if rename {
                file_rules::render_filename(
                    new.rule(record.feature),
                    &record.context()?,
                    extension,
                )?
            } else {
                source
                    .file_name()
                    .ok_or("文件名无效")?
                    .to_string_lossy()
                    .into_owned()
            };
            let mut target = parent.join(name);
            if source != target {
                // Reports may share a literal template; retain every export.
                if record.feature != Feature::Journal {
                    let stem = target.file_stem().unwrap().to_string_lossy().into_owned();
                    for sequence in 0..10_000 {
                        if (!target.exists() || same_directory(&source, &target))
                            && !reserved.contains(&target)
                        {
                            break;
                        }
                        target = parent.join(format!("{stem}_{}.{}", sequence + 1, extension));
                    }
                }
                if (target.exists() && !same_directory(&source, &target))
                    || reserved.contains(&target)
                {
                    return Err(format!(
                        "修改后文件名冲突：{}，未覆盖任何文件",
                        target.display()
                    ));
                }
                file_rules::safe_file(&root, &file_rules::relative_path(&root, &target)?)?;
                let staged = stage.join(format!("file-{index}"));
                move_recorded(&source, &staged, &mut undo)?;
                pending.push((staged, target.clone()));
            }
            reserved.insert(target.clone());
            record.relative_path = file_rules::relative_path(&root, &target)?;
        }
        for (staged, target) in pending {
            move_recorded(&staged, &target, &mut undo)?;
        }
        // A formerly deleted filename can be reused by a new naming rule.
        // The real file's current metadata supersedes its dangling old entry.
        missing.retain(|missing| {
            !records
                .iter()
                .any(|record| record.relative_path == missing.relative_path)
        });
        records.extend(missing);
        metadata_written = true;
        file_rules::write_catalog(&root, &records)?;
        persist()
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        if let Err(error) = rollback(&mut undo) {
            failures.push(error);
        }
        if metadata_written {
            if let Err(error) = restore(&catalog_path, &catalog_bytes) {
                failures.push(error);
            }
            if let Err(error) = restore(&settings_path, &settings_bytes) {
                failures.push(error);
            }
        }
        let _ = fs::remove_dir(&stage);
        return if failures.is_empty() {
            Err(format!("修改未完成，已恢复原文件和设置：{error}"))
        } else {
            Err(format!(
                "修改失败：{error}；部分恢复失败：{}。请保留恢复目录 {}",
                failures.join("；"),
                stage.display()
            ))
        };
    }
    let _ = fs::remove_dir(&stage);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn confirm(root: &Path, old: &Settings, new: &Settings, rename: bool) -> Confirmation {
        Confirmation {
            token: preview(root, old, new).unwrap().token,
            rename_existing: FEATURES
                .into_iter()
                .map(|feature| (feature, rename))
                .collect(),
        }
    }
    fn fixture() -> (tempfile::TempDir, Settings) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("2026")).unwrap();
        fs::write(root.path().join("2026/20261031.md"), "journal").unwrap();
        fs::create_dir(root.path().join("excel")).unwrap();
        fs::write(
            root.path()
                .join("excel/总结_20261001-20261031_20261031_203000.xlsx"),
            "report",
        )
        .unwrap();
        fs::write(root.path().join("excel/其他文件.txt"), "keep").unwrap();
        (root, Settings::default())
    }
    #[test]
    fn moves_directories_and_renames_only_when_chosen() {
        for rename in [false, true] {
            let (root, old) = fixture();
            let mut new = old.clone();
            new.journal.directory = "日报".into();
            new.journal.filename_template = "日报_{YYYY}-{MM}-{DD}".into();
            new.excel.directory = "报告/表格".into();
            new.excel.filename_template = "项目_{date}".into();
            let decision = confirm(root.path(), &old, &new, rename);
            apply(root.path(), &old, &new, Some(&decision), || Ok(())).unwrap();
            let journal = if rename {
                "日报_2026-10-31.md"
            } else {
                "20261031.md"
            };
            assert_eq!(
                fs::read_to_string(root.path().join("日报/2026").join(journal)).unwrap(),
                "journal"
            );
            assert_eq!(
                fs::read_to_string(root.path().join("报告/表格/其他文件.txt")).unwrap(),
                "keep"
            );
            assert!(!root.path().join("excel").exists());
            assert_eq!(
                file_rules::journal_records(root.path(), &new)
                    .unwrap()
                    .len(),
                1
            );
        }
    }
    #[test]
    fn save_failure_restores_files_settings_catalog_and_empty_destinations() {
        let (root, old) = fixture();
        let mut new = old.clone();
        fs::write(
            root.path().join(crate::settings::FILE_NAME),
            "original settings",
        )
        .unwrap();
        new.excel.directory = "new".into();
        new.journal.filename_template = "新_{date}".into();
        fs::create_dir(root.path().join("new")).unwrap();
        let decision = confirm(root.path(), &old, &new, true);
        let error = apply(root.path(), &old, &new, Some(&decision), || {
            fs::write(root.path().join(crate::settings::FILE_NAME), "partial").unwrap();
            Err("injected failure".into())
        })
        .unwrap_err();
        assert!(error.contains("已恢复"), "{error}");
        assert!(root.path().join("2026/20261031.md").is_file());
        assert!(root.path().join("excel/其他文件.txt").is_file());
        assert!(root.path().join("new").is_dir());
        assert!(!root.path().join(file_rules::CATALOG_FILE).exists());
        assert_eq!(
            fs::read_to_string(root.path().join(crate::settings::FILE_NAME)).unwrap(),
            "original settings"
        );
        assert!(!fs::read_dir(root.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".workreport-migration-")));
    }
    #[test]
    fn stale_plans_and_collisions_do_not_modify_files() {
        let (root, old) = fixture();
        let mut new = old.clone();
        new.journal.filename_template = "新_{date}".into();
        let decision = confirm(root.path(), &old, &new, true);
        fs::write(root.path().join("2026/20261031.md"), "changed").unwrap();
        assert!(apply(root.path(), &old, &new, Some(&decision), || Ok(()))
            .unwrap_err()
            .contains("已变化"));
        fs::write(root.path().join("2026/新_20261031.md"), "existing").unwrap();
        let decision = confirm(root.path(), &old, &new, true);
        assert!(apply(root.path(), &old, &new, Some(&decision), || Ok(())).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("2026/新_20261031.md")).unwrap(),
            "existing"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("2026/20261031.md")).unwrap(),
            "changed"
        );
    }
    #[test]
    fn literal_report_names_keep_every_export_and_original_generation_dates() {
        let (root, old) = fixture();
        fs::write(
            root.path()
                .join("excel/总结_20261001-20261031_20261031_203001.xlsx"),
            "second report",
        )
        .unwrap();
        file_rules::register(
            root.path(),
            file_rules::ManagedFile::report(
                Feature::Excel,
                "excel/固定名称.xlsx",
                "20260901",
                "20260930",
                "20260930_000000",
            )
            .unwrap(),
        )
        .unwrap();
        let mut new = old.clone();
        new.excel.filename_template = "固定名称".into();
        let decision = confirm(root.path(), &old, &new, true);
        apply(root.path(), &old, &new, Some(&decision), || Ok(())).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("excel/固定名称.xlsx")).unwrap(),
            "report"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("excel/固定名称_1.xlsx")).unwrap(),
            "second report"
        );
        let records = file_rules::catalog(root.path()).unwrap();
        assert_eq!(
            records
                .iter()
                .filter(|record| record.feature == Feature::Excel)
                .count(),
            2
        );
        assert!(records
            .iter()
            .filter(|record| record.feature == Feature::Excel)
            .all(|record| record.date == "20261031" && record.start_date == "20261001"));
    }
    #[test]
    fn populated_destination_is_rejected_and_root_keeps_unrelated_files() {
        let (root, old) = fixture();
        let mut new = old.clone();
        new.excel.directory = "reports".into();
        fs::create_dir(root.path().join("reports")).unwrap();
        fs::write(root.path().join("reports/keep.txt"), "target").unwrap();
        assert!(preview(root.path(), &old, &new)
            .unwrap_err()
            .contains("已有内容"));
        assert!(root.path().join("excel/其他文件.txt").is_file());
        new.excel = old.excel.clone();
        new.journal.directory = "日志".into();
        fs::write(root.path().join("2026/说明.txt"), "unrelated").unwrap();
        let decision = confirm(root.path(), &old, &new, false);
        apply(root.path(), &old, &new, Some(&decision), || Ok(())).unwrap();
        assert!(root.path().join("2026/说明.txt").is_file());
        assert!(root.path().join("日志/2026/20261031.md").is_file());
    }
    #[test]
    fn adopting_populated_directories_requires_confirmation_and_keeps_custom_journal_identity() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("已有日报/2026")).unwrap();
        fs::write(root.path().join("已有日报/2026/旧_20261031.md"), "existing").unwrap();
        let mut old = Settings::default();
        old.journal.filename_template = "旧_{date}".into();
        let mut new = old.clone();
        new.journal.directory = "已有日报".into();
        new.journal.filename_template = "新_{date}".into();
        let plan = preview(root.path(), &old, &new).unwrap();
        assert_eq!(plan.directories[0].file_count, 0);
        assert_eq!(plan.directories[0].target_file_count, 1);
        assert_eq!(plan.filenames[0].file_count, 1);
        assert!(apply(root.path(), &old, &new, None, || Ok(())).is_err());
        let decision = confirm(root.path(), &old, &new, false);
        apply(root.path(), &old, &new, Some(&decision), || Ok(())).unwrap();
        assert_eq!(
            file_rules::journal_records(root.path(), &new).unwrap()[0].date,
            "20261031"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("已有日报/2026/旧_20261031.md")).unwrap(),
            "existing"
        );
    }
    #[test]
    fn empty_source_can_adopt_an_existing_target_and_case_only_renames_do_not_duplicate_records() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("excel")).unwrap();
        fs::create_dir(root.path().join("报表")).unwrap();
        fs::write(root.path().join("报表/existing.xlsx"), "existing").unwrap();
        let old = Settings::default();
        let mut new = old.clone();
        new.excel.directory = "报表".into();
        let decision = confirm(root.path(), &old, &new, false);
        apply(root.path(), &old, &new, Some(&decision), || Ok(())).unwrap();
        assert!(!root.path().join("excel").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("报表/existing.xlsx")).unwrap(),
            "existing"
        );
        let (root, old) = fixture();
        let mut new = old.clone();
        new.excel.directory = "EXCEL".into();
        let decision = confirm(root.path(), &old, &new, false);
        apply(root.path(), &old, &new, Some(&decision), || Ok(())).unwrap();
        let records = file_rules::catalog(root.path()).unwrap();
        assert_eq!(
            records
                .iter()
                .filter(|record| record.feature == Feature::Excel)
                .count(),
            1
        );
        assert!(root
            .path()
            .join("EXCEL/总结_20261001-20261031_20261031_203000.xlsx")
            .is_file());
    }
    #[cfg(unix)]
    #[test]
    fn links_in_source_or_destination_cannot_redirect_migration() {
        use std::os::unix::fs::symlink;
        let (root, old) = fixture();
        let outside = tempfile::tempdir().unwrap();
        let mut new = old.clone();
        new.excel.directory = "linked".into();
        symlink(outside.path(), root.path().join("linked")).unwrap();
        assert!(preview(root.path(), &old, &new).is_err());
        new.excel.directory = "reports".into();
        symlink(outside.path(), root.path().join("excel/linked")).unwrap();
        assert!(preview(root.path(), &old, &new).is_err());
        assert!(root.path().join("excel/其他文件.txt").is_file());
    }
}

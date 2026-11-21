use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use chrono::NaiveDate;
use gpui_kit::{AppContext, TestAppContext};
use workreport_core::ai::{
    DetectedCli, ModelInfo, Provider, ProviderKind, Runtime as AiRuntime, Settings as AiSettings,
    TestProviderResult,
};

use crate::{
    ai_panel::AiBackend,
    workspace_edit_tests::{click, drain_ready_tasks, open_window, open_workspace},
};

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn minimal_xlsx_bytes() -> Vec<u8> {
    let files: [(&str, &[u8]); 5] = [
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="inlineStr"><is><t>Task</t></is></c>
      <c r="B1" t="inlineStr"><is><t></t></is></c>
    </row>
  </sheetData>
</worksheet>"#,
        ),
    ];

    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content) in files {
        let offset = out.len() as u32;
        let name_bytes = name.as_bytes();
        let crc = crc32_ieee(content);
        let size = content.len() as u32;

        // Local file header
        out.extend_from_slice(&0x0403_4B50_u32.to_le_bytes());
        out.extend_from_slice(&20_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(content);

        // Central directory header
        central.extend_from_slice(&0x0201_4B50_u32.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }

    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);

    // End of central directory
    out.extend_from_slice(&0x0605_4B50_u32.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

struct ExcelMockAi {
    captured_fill_inputs: Mutex<Vec<String>>,
}

impl AiBackend for ExcelMockAi {
    fn detect(&self) -> Vec<DetectedCli> {
        vec![]
    }

    fn list_models(&self, _: &str, _: &str, _: ProviderKind) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![])
    }

    fn list_cli_models(&self, _: ProviderKind, _: &str) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![])
    }

    fn test_provider(&self, _: &Provider) -> Result<TestProviderResult, String> {
        unreachable!()
    }

    fn polish(&self, _: &AiSettings, _: &str, _: &str, _: &str, _: &AiRuntime) -> Result<String, String> {
        unreachable!()
    }

    fn summarize(&self, _: &AiSettings, _: &str, _: Option<String>, _: &str, _: &AiRuntime) -> Result<String, String> {
        unreachable!()
    }

    fn generate_excel_content(
        &self,
        _: &Provider,
        content: &str,
        _: &str,
        _: &str,
        _: &AiRuntime,
    ) -> Result<String, String> {
        Ok(format!("阶段1初稿：{}", content.trim()))
    }

    fn fill_excel(
        &self,
        _: &Provider,
        _: &str,
        content: &str,
        _: &str,
        _: &str,
        _: &AiRuntime,
    ) -> Result<String, String> {
        self.captured_fill_inputs
            .lock()
            .unwrap()
            .push(content.to_string());
        Ok(r#"{"cells":[{"sheet":"Sheet1","cell":"B1","value":"人工核对后的汇报"}]}"#.into())
    }

    fn git_commit_message(
        &self,
        _: &AiSettings,
        _: &str,
        _: &str,
        _: &str,
        _: &AiRuntime,
    ) -> Result<String, String> {
        unreachable!()
    }
}

fn setup_excel_workspace(root: &Path, template: &Path) {
    fs::create_dir_all(root.join("2026")).unwrap();
    fs::write(root.join("2026/20261119.md"), "完成 GPUI Excel 双阶段迁移\n").unwrap();
    fs::write(template, minimal_xlsx_bytes()).unwrap();

    let mut stored = workreport_core::settings::StoredSettings::default();
    stored.ai = AiSettings {
        default_provider_id: Some("openai".into()),
        providers: vec![Provider {
            id: "openai".into(),
            name: "MockOpenAI".into(),
            kind: ProviderKind::Http,
            executable: String::new(),
            model: "gpt-4o-mini".into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: "sk-test-123456".into(),
        }],
        ..Default::default()
    };
    stored.excel = workreport_core::excel::Settings {
        enabled: true,
        template_path: template.to_string_lossy().into(),
        content_prompt: "归纳要点".into(),
        prompt: "填入 Sheet1 B1".into(),
    };
    workreport_core::settings::save(&root.join(workreport_core::settings::FILE_NAME), &stored).unwrap();
}

#[gpui_kit::test]
fn excel_two_phase_workflow_edits_validates_provenance_and_exports_xlsx(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let tpl_dir = tempfile::tempdir().unwrap();
    let tpl_path = tpl_dir.path().join("template.xlsx");
    setup_excel_workspace(dir.path(), &tpl_path);

    let mock_ai = Arc::new(ExcelMockAi {
        captured_fill_inputs: Mutex::new(Vec::new()),
    });

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, dir.path());

    click(cx, window, "rail-module-excel");
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.ai.backend = mock_ai.clone();
            app.calendar = crate::calendar::CalendarState::new(
                NaiveDate::from_ymd_opt(2026, 11, 19).unwrap(),
            );
            app.start_excel_content_generation(window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);

    // Phase 1 draft populated & editable before template filling
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            assert_eq!(app.excel.error, None);
            let draft = app.excel.content_editor.read(cx).value().to_string();
            assert!(draft.contains("阶段1初稿"), "unexpected draft: {draft:?}");
            app.excel.content_editor.update(cx, |ed, cx| {
                ed.set_value("人工核对后的正文内容".to_string(), window, cx);
            });
            app.prepare_excel_fill_preview(window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);

    // Phase 2 preview prepared using edited Phase 1 content
    cx.update(|cx| {
        let app = prototype.read(cx);
        let captured = mock_ai.captured_fill_inputs.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].contains("人工核对后的正文内容"));
        let (preview, _) = app.excel.preview.as_ref().expect("preview should be ready");
        assert!(
            preview.sheets[0].regions[0]
                .cells
                .iter()
                .any(|cell| cell.address == "B1" && cell.text == "人工核对后的汇报")
        );
    });

    // External source modification before confirmation must invalidate stale preview
    fs::write(dir.path().join("2026/20261119.md"), "日报中途被外部修改了\n").unwrap();
    click(cx, window, "excel-confirm-export-btn");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.excel.error.is_some());
        assert!(app.excel.last_export.is_none());
    });

    // Re-prepare & export cleanly
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.prepare_excel_fill_preview(window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    click(cx, window, "excel-confirm-export-btn");

    cx.update(|cx| {
        let app = prototype.read(cx);
        let export = app.excel.last_export.as_ref().expect("export should succeed");
        assert_eq!(export.cells, 1);
        assert!(Path::new(&export.path).is_file());
        assert!(!app.excel.exports.is_empty());
    });
}

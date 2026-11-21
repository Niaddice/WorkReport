use std::{
    fs,
    sync::{Arc, Mutex},
};

use chrono::NaiveDate;
use gpui_kit::{AppContext, TestAppContext, test::TestWindowExt};
use workreport_core::{
    ai::{self, DetectedCli, ModelInfo, Provider, ProviderKind, Runtime as AiRuntime, Settings, TestProviderResult},
    workspace::Format,
};

use crate::{
    ai_panel::AiBackend,
    summary_view::{
        ReportDocument, SummaryMode, ai_summary_source, markdown_to_plain_text,
        plain_text_summary, summary_export_content,
    },
    workspace_edit_tests::{click, drain_ready_tasks, editor_text, open_window, open_workspace},
};

struct MockAiBackend {
    summary_reply: String,
    polish_reply: String,
    captured_prompts: Mutex<Vec<Option<String>>>,
}

impl AiBackend for MockAiBackend {
    fn detect(&self) -> Vec<DetectedCli> {
        vec![DetectedCli {
            kind: ProviderKind::Codex,
            name: "Codex".into(),
            executable: "/usr/local/bin/codex".into(),
            version: "1.0.0".into(),
            cli_available: true,
            app_path: None,
            message: None,
        }]
    }

    fn list_models(
        &self,
        _base_url: &str,
        _api_key: &str,
        _kind: ProviderKind,
    ) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![ModelInfo {
            id: "gpt-4o-mini".into(),
            name: "GPT-4o mini".into(),
        }])
    }

    fn list_cli_models(
        &self,
        _kind: ProviderKind,
        _executable: &str,
    ) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![])
    }

    fn test_provider(&self, _provider: &Provider) -> Result<TestProviderResult, String> {
        Ok(TestProviderResult {
            success: true,
            latency_ms: 42,
            reply: "OK".into(),
            error: None,
        })
    }

    fn polish(
        &self,
        settings: &Settings,
        _content: &str,
        _format: &str,
        _request_id: &str,
        _runtime: &AiRuntime,
    ) -> Result<String, String> {
        ai::default_provider(settings)?;
        Ok(self.polish_reply.clone())
    }

    fn summarize(
        &self,
        settings: &Settings,
        _content: &str,
        prompt: Option<String>,
        _request_id: &str,
        _runtime: &AiRuntime,
    ) -> Result<String, String> {
        ai::default_provider(settings)?;
        self.captured_prompts.lock().unwrap().push(prompt);
        Ok(self.summary_reply.clone())
    }

    fn generate_excel_content(
        &self,
        _provider: &Provider,
        _content: &str,
        _prompt: &str,
        _request_id: &str,
        _runtime: &AiRuntime,
    ) -> Result<String, String> {
        Ok("# 报告正文\n- 完成项".into())
    }

    fn fill_excel(
        &self,
        _provider: &Provider,
        _context: &str,
        _content: &str,
        _prompt: &str,
        _request_id: &str,
        _runtime: &AiRuntime,
    ) -> Result<String, String> {
        Ok(r#"{"cells":[]}"#.into())
    }

    fn git_commit_message(
        &self,
        settings: &Settings,
        _content: &str,
        _prompt: &str,
        _request_id: &str,
        _runtime: &AiRuntime,
    ) -> Result<String, String> {
        ai::default_provider(settings)?;
        Ok("feat: 提交测试变更".into())
    }
}

#[test]
fn summary_formatters_preserve_numbering_and_strip_markdown_only_for_ai_txt() {
    let docs = vec![
        ReportDocument {
            date: "20261118".into(),
            content: "# 2026年11月18日\n- 完成 **核心接口**\n- 修复回归\n".into(),
        },
        ReportDocument {
            date: "20261119".into(),
            content: "2026年11月19日日报\n推进 GPUI 迁移\n".into(),
        },
    ];
    let direct = plain_text_summary(&docs);
    assert!(direct.contains("2026年11月18日\n\n1. 完成 **核心接口**\n2. 修复回归"));
    assert!(direct.contains("2026年11月19日\n\n1. 推进 GPUI 迁移"));
    assert_eq!(
        summary_export_content(&direct, Format::Txt, SummaryMode::Direct),
        direct
    );

    let ai_source = ai_summary_source(&docs);
    assert!(ai_source.contains("### 2026年11月18日 星期三"));
    assert!(ai_source.contains("---"));

    let md = "## 工作进展\n\n- 完成 `workreport-core` **集成**";
    assert_eq!(markdown_to_plain_text(md), "工作进展\n\n- 完成 workreport-core 集成");
    assert_eq!(summary_export_content(md, Format::Md, SummaryMode::Ai), md);
    assert_eq!(
        summary_export_content(md, Format::Txt, SummaryMode::Ai),
        "工作进展\n\n- 完成 workreport-core 集成"
    );
}

#[gpui_kit::test]
fn summary_exports_direct_and_ai_results_and_editor_applies_ai_polish(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    fs::write(
        folder.join("2026/20261118.md"),
        "# 18日\n- 完成基础分栏\n",
    )
    .unwrap();
    fs::write(
        folder.join("2026/20261119.md"),
        "# 19日\n- 完成目录与总结\n",
    )
    .unwrap();

    let mock_backend = Arc::new(MockAiBackend {
        summary_reply: "## 本阶段进展\n\n- 完成 **基础分栏** 与总结模块".into(),
        polish_reply: "# 19日\n- 完成工作空间目录与总结模块的阶段 3 迁移\n".into(),
        captured_prompts: Mutex::new(Vec::new()),
    });

    let (window, prototype) = open_window(cx);
    cx.update(|cx| {
        prototype.update(cx, |app, _| {
            app.ai.backend = mock_backend.clone();
        });
    });
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "rail-module-summary");

    // Select 18..=19 range and prepare summary documents.
    let d1 = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
    let d2 = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| app.select_calendar_date(d1, false, window, cx));
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| app.select_calendar_date(d2, true, window, cx));
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    drain_ready_tasks(cx);

    // 1. Direct summary + Markdown export into workspace summary directory.
    click(cx, window, "summary-direct-btn");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert_eq!(app.summary.mode, SummaryMode::Direct);
        assert!(app.summary.result.contains("完成基础分栏"));
        assert!(app.summary.result.contains("完成目录与总结"));
    });
    click(cx, window, "summary-export-btn");
    let first_export = cx.update(|cx| prototype.read(cx).summary.exported_path.clone().unwrap());
    assert!(std::path::Path::new(&first_export).is_file());
    assert!(fs::read_to_string(&first_export).unwrap().contains("完成目录与总结"));

    // 2. Configure AI HTTP provider through AI Settings modal, fetch models, test & persist.
    click(cx, window, "summary-open-ai-settings");
    click(cx, window, "ai-add-http");
    click(cx, window, "ai-fetch-models");
    click(cx, window, "ai-model-gpt-4o-mini");
    click(cx, window, "ai-test-provider");
    cx.update(|cx| {
        let dialog = prototype.read(cx).ai.dialog.as_ref().unwrap();
        assert!(dialog.test_result.as_ref().unwrap().success);
    });
    click(cx, window, "ai-settings-save");
    cx.update(|cx| {
        assert!(prototype.read(cx).ai.dialog.is_none());
        assert_eq!(prototype.read(cx).ai.default_provider().unwrap().model, "gpt-4o-mini");
    });

    // 3. AI Summary + TXT export strips markdown formatting cleanly.
    click(cx, window, "summary-ai-btn");
    click(cx, window, "summary-format-txt");
    click(cx, window, "summary-export-btn");
    let txt_export = cx.update(|cx| prototype.read(cx).summary.exported_path.clone().unwrap());
    assert!(txt_export.ends_with(".txt"));
    let exported_txt = fs::read_to_string(&txt_export).unwrap();
    assert!(!exported_txt.contains("**"));
    assert!(exported_txt.contains("完成 基础分栏 与总结模块"));

    // 4. AI Polish in Journal Editor -> opens diff modal, applies polished text cleanly.
    click(cx, window, "close-range");
    click(cx, window, "edit-journal");
    click(cx, window, "ai-polish-journal");
    cx.update(|cx| {
        assert!(prototype.read(cx).ai.polish_diff.is_some());
    });
    click(cx, window, "polish-diff-apply");
    assert_eq!(editor_text(cx, &prototype), mock_backend.polish_reply);
}

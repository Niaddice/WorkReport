use std::sync::Arc;

use gpui_kit::{
    Context, Entity, MouseButton, Task, Window,
    base::Disableable,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Input, InputState},
    },
    div, prelude::*, px,
};
use workreport_core::{
    ai::{
        self, DetectedCli, ModelInfo, Provider, ProviderKind, Runtime as AiRuntime,
        Settings as AiSettings, TestProviderResult,
    },
    settings,
    workspace::Format,
};

use crate::{
    Prototype,
    history_diff::{HistoryDiffMode, render_diff_body},
    line_diff::{DiffResult, compute_line_diff},
};

pub(crate) trait AiBackend: Send + Sync {
    fn detect(&self) -> Vec<DetectedCli>;
    fn list_models(
        &self,
        base_url: &str,
        api_key: &str,
        kind: ProviderKind,
    ) -> Result<Vec<ModelInfo>, String>;
    fn list_cli_models(
        &self,
        kind: ProviderKind,
        executable: &str,
    ) -> Result<Vec<ModelInfo>, String>;
    fn test_provider(&self, provider: &Provider) -> Result<TestProviderResult, String>;
    fn polish(
        &self,
        settings: &AiSettings,
        content: &str,
        format: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String>;
    fn summarize(
        &self,
        settings: &AiSettings,
        content: &str,
        prompt: Option<String>,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String>;
    fn generate_excel_content(
        &self,
        provider: &Provider,
        content: &str,
        prompt: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String>;
    fn fill_excel(
        &self,
        provider: &Provider,
        context: &str,
        content: &str,
        prompt: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String>;
    fn git_commit_message(
        &self,
        settings: &AiSettings,
        content: &str,
        prompt: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String>;
}

pub(crate) struct ProductionAiBackend;

pub(crate) fn run_tokio<T>(future: impl std::future::Future<Output = T>) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("无法初始化异步运行时：{error}"))?;
    Ok(runtime.block_on(future))
}

impl AiBackend for ProductionAiBackend {
    fn detect(&self) -> Vec<DetectedCli> {
        run_tokio(ai::detect()).unwrap_or_default()
    }

    fn list_models(
        &self,
        base_url: &str,
        api_key: &str,
        kind: ProviderKind,
    ) -> Result<Vec<ModelInfo>, String> {
        run_tokio(async move {
            match kind {
                ProviderKind::Anthropic => ai::list_anthropic_models(base_url, api_key).await,
                _ => ai::list_models(base_url, api_key).await,
            }
        })?
    }

    fn list_cli_models(
        &self,
        kind: ProviderKind,
        executable: &str,
    ) -> Result<Vec<ModelInfo>, String> {
        run_tokio(ai::list_cli_models(&kind, executable))?
    }

    fn test_provider(&self, provider: &Provider) -> Result<TestProviderResult, String> {
        run_tokio(ai::test_provider(provider))?
    }

    fn polish(
        &self,
        settings: &AiSettings,
        content: &str,
        format: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String> {
        let provider = ai::default_provider(settings)?;
        let token = runtime.begin(request_id.to_string())?;
        let outcome = run_tokio(ai::polish(
            &provider,
            content,
            format,
            &settings.generation_preference,
            token,
        ))?;
        runtime.finish(request_id);
        outcome
    }

    fn summarize(
        &self,
        settings: &AiSettings,
        content: &str,
        prompt: Option<String>,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String> {
        let provider = ai::default_provider(settings)?;
        let prompt = ai::summary_request_prompt(&settings.summary_prompt, prompt)?;
        let token = runtime.begin(request_id.to_string())?;
        let outcome = run_tokio(ai::summarize(&provider, content, &prompt, token))?;
        runtime.finish(request_id);
        outcome
    }

    fn generate_excel_content(
        &self,
        provider: &Provider,
        content: &str,
        prompt: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String> {
        let token = runtime.begin(request_id.to_string())?;
        let outcome = run_tokio(ai::generate_excel_content(provider, content, prompt, token))?;
        runtime.finish(request_id);
        outcome
    }

    fn fill_excel(
        &self,
        provider: &Provider,
        context: &str,
        content: &str,
        prompt: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String> {
        let token = runtime.begin(request_id.to_string())?;
        let outcome = run_tokio(ai::fill_excel(provider, context, content, prompt, token))?;
        runtime.finish(request_id);
        outcome
    }

    fn git_commit_message(
        &self,
        settings: &AiSettings,
        content: &str,
        prompt: &str,
        request_id: &str,
        runtime: &AiRuntime,
    ) -> Result<String, String> {
        let provider = ai::default_provider(settings)?;
        let token = runtime.begin(request_id.to_string())?;
        let outcome = run_tokio(ai::git_commit_message(&provider, content, prompt, token))?;
        runtime.finish(request_id);
        outcome
    }
}

pub(crate) fn legacy_config_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            std::path::PathBuf::from(home)
                .join("Library/Application Support/com.workreport.desktop")
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .or_else(|| std::env::var_os("APPDATA"))
            .map(|dir| std::path::PathBuf::from(dir).join("com.workreport.desktop"))
    }
}

pub(crate) fn load_effective_ai_settings(workspace_ai: Option<AiSettings>) -> AiSettings {
    if let Some(settings) = workspace_ai {
        return settings;
    }
    if let Some(legacy) = legacy_config_dir() {
        if let Ok(settings) = ai::load(&legacy) {
            return settings;
        }
    }
    AiSettings::default()
}

pub(crate) struct PolishDiffState {
    pub(crate) date: String,
    pub(crate) polished: String,
    pub(crate) mode: HistoryDiffMode,
    pub(crate) diff: DiffResult,
}

pub(crate) struct AiSettingsDialogState {
    pub(crate) draft: AiSettings,
    pub(crate) selected_id: Option<String>,
    pub(crate) name_input: Entity<InputState>,
    pub(crate) base_url_input: Entity<InputState>,
    pub(crate) secret_or_exec_input: Entity<InputState>,
    pub(crate) model_input: Entity<InputState>,
    pub(crate) preference_input: Entity<InputState>,
    pub(crate) summary_prompt_input: Entity<InputState>,
    pub(crate) detected_clis: Vec<DetectedCli>,
    pub(crate) detecting_clis: bool,
    pub(crate) models: Vec<ModelInfo>,
    pub(crate) loading_models: bool,
    pub(crate) models_error: Option<String>,
    pub(crate) testing: bool,
    pub(crate) test_result: Option<TestProviderResult>,
    pub(crate) saving: bool,
    pub(crate) error: Option<String>,
    next_id: u64,
}

pub(crate) struct AiState {
    pub(crate) backend: Arc<dyn AiBackend>,
    pub(crate) runtime: Arc<AiRuntime>,
    pub(crate) settings: AiSettings,
    pub(crate) dialog: Option<AiSettingsDialogState>,
    pub(crate) polishing: bool,
    pub(crate) polish_request_id: Option<String>,
    pub(crate) polish_seq: u64,
    pub(crate) polish_diff: Option<PolishDiffState>,
    pub(crate) polish_error: Option<String>,
    pub(crate) polish_task: Option<Task<()>>,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            backend: Arc::new(ProductionAiBackend),
            runtime: Arc::new(AiRuntime::default()),
            settings: AiSettings::default(),
            dialog: None,
            polishing: false,
            polish_request_id: None,
            polish_seq: 0,
            polish_diff: None,
            polish_error: None,
            polish_task: None,
        }
    }
}

impl AiState {
    pub(crate) fn reset_for_workspace(&mut self, settings: AiSettings) {
        self.cancel_polish();
        self.settings = settings;
        self.dialog = None;
        self.polish_diff = None;
        self.polish_error = None;
    }

    pub(crate) fn cancel_polish(&mut self) {
        if let Some(id) = self.polish_request_id.take() {
            self.runtime.cancel(&id);
        }
        self.polishing = false;
        self.polish_task = None;
    }

    pub(crate) fn default_provider(&self) -> Option<&Provider> {
        let id = self.settings.default_provider_id.as_deref()?;
        self.settings.providers.iter().find(|p| p.id == id)
    }
}

pub(crate) fn provider_kind_label(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Http => "HTTP (OpenAI 兼容)",
        ProviderKind::Anthropic => "Anthropic",
        ProviderKind::Codex => "Codex CLI",
        ProviderKind::Claude => "Claude CLI",
        ProviderKind::Opencode => "OpenCode CLI",
        ProviderKind::Cursor => "Cursor CLI",
        ProviderKind::Grok => "Grok CLI",
    }
}

pub(crate) fn is_http_kind(kind: &ProviderKind) -> bool {
    matches!(kind, ProviderKind::Http | ProviderKind::Anthropic)
}

impl Prototype {
    pub(crate) fn open_ai_settings_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = self.ai.settings.clone();
        let selected_id = draft
            .default_provider_id
            .clone()
            .or_else(|| draft.providers.first().map(|p| p.id.clone()));
        let selected = selected_id
            .as_deref()
            .and_then(|id| draft.providers.iter().find(|p| p.id == id))
            .cloned();

        let name_val = selected.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        let url_val = selected
            .as_ref()
            .map(|p| p.base_url.clone())
            .unwrap_or_default();
        let sec_val = selected
            .as_ref()
            .map(|p| {
                if is_http_kind(&p.kind) {
                    p.api_key.clone()
                } else {
                    p.executable.clone()
                }
            })
            .unwrap_or_default();
        let model_val = selected
            .as_ref()
            .map(|p| p.model.clone())
            .unwrap_or_default();
        let pref_val = draft.generation_preference.clone();
        let sum_val = draft.summary_prompt.clone();

        let name_input = cx.new(|cx| InputState::new(window, cx).default_value(name_val));
        let base_url_input = cx.new(|cx| InputState::new(window, cx).default_value(url_val));
        let secret_or_exec_input = cx.new(|cx| InputState::new(window, cx).default_value(sec_val));
        let model_input = cx.new(|cx| InputState::new(window, cx).default_value(model_val));
        let preference_input = cx.new(|cx| InputState::new(window, cx).default_value(pref_val));
        let summary_prompt_input = cx.new(|cx| InputState::new(window, cx).default_value(sum_val));

        let next_id = draft.providers.len() as u64 + 1;
        self.ai.dialog = Some(AiSettingsDialogState {
            draft,
            selected_id,
            name_input,
            base_url_input,
            secret_or_exec_input,
            model_input,
            preference_input,
            summary_prompt_input,
            detected_clis: Vec::new(),
            detecting_clis: false,
            models: Vec::new(),
            loading_models: false,
            models_error: None,
            testing: false,
            test_result: None,
            saving: false,
            error: None,
            next_id,
        });
        cx.notify();
    }

    fn commit_active_provider_fields(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.ai.dialog else {
            return;
        };
        dialog.draft.generation_preference = dialog.preference_input.read(cx).value().to_string();
        dialog.draft.summary_prompt = dialog.summary_prompt_input.read(cx).value().to_string();
        let Some(selected_id) = dialog.selected_id.clone() else {
            return;
        };
        if let Some(provider) = dialog
            .draft
            .providers
            .iter_mut()
            .find(|p| p.id == selected_id)
        {
            provider.name = dialog.name_input.read(cx).value().trim().to_string();
            provider.model = dialog.model_input.read(cx).value().trim().to_string();
            if is_http_kind(&provider.kind) {
                provider.base_url = dialog.base_url_input.read(cx).value().trim().to_string();
                provider.api_key = dialog
                    .secret_or_exec_input
                    .read(cx)
                    .value()
                    .trim()
                    .to_string();
            } else {
                provider.executable = dialog
                    .secret_or_exec_input
                    .read(cx)
                    .value()
                    .trim()
                    .to_string();
            }
        }
    }

    fn load_provider_fields_into_inputs(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.ai.dialog else {
            return;
        };
        dialog.selected_id = id.clone();
        dialog.models.clear();
        dialog.models_error = None;
        dialog.test_result = None;
        let selected = id
            .as_deref()
            .and_then(|target| dialog.draft.providers.iter().find(|p| p.id == target))
            .cloned();
        let name = selected.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        let url = selected
            .as_ref()
            .map(|p| p.base_url.clone())
            .unwrap_or_default();
        let sec = selected
            .as_ref()
            .map(|p| {
                if is_http_kind(&p.kind) {
                    p.api_key.clone()
                } else {
                    p.executable.clone()
                }
            })
            .unwrap_or_default();
        let model = selected.as_ref().map(|p| p.model.clone()).unwrap_or_default();
        let name_in = dialog.name_input.clone();
        let url_in = dialog.base_url_input.clone();
        let sec_in = dialog.secret_or_exec_input.clone();
        let model_in = dialog.model_input.clone();
        name_in.update(cx, |i, cx| i.set_value(name, window, cx));
        url_in.update(cx, |i, cx| i.set_value(url, window, cx));
        sec_in.update(cx, |i, cx| i.set_value(sec, window, cx));
        model_in.update(cx, |i, cx| i.set_value(model, window, cx));
    }

    pub(crate) fn add_ai_provider(
        &mut self,
        kind: ProviderKind,
        name: String,
        executable: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_active_provider_fields(cx);
        let Some(dialog) = &mut self.ai.dialog else {
            return;
        };
        dialog.next_id += 1;
        let id = format!("provider-{}", dialog.next_id);
        let base_url = match kind {
            ProviderKind::Http => "https://api.openai.com/v1".to_string(),
            ProviderKind::Anthropic => "https://api.anthropic.com".to_string(),
            _ => String::new(),
        };
        let provider = Provider {
            id: id.clone(),
            name,
            kind,
            executable,
            model: String::new(),
            base_url,
            api_key: String::new(),
        };
        dialog.draft.providers.push(provider);
        if dialog.draft.default_provider_id.is_none() {
            dialog.draft.default_provider_id = Some(id.clone());
        }
        self.load_provider_fields_into_inputs(Some(id), window, cx);
        cx.notify();
    }

    pub(crate) fn detect_ai_clis(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.ai.dialog else {
            return;
        };
        if dialog.detecting_clis {
            return;
        }
        dialog.detecting_clis = true;
        let backend = self.ai.backend.clone();
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { backend.detect() });
        cx.spawn_in(window, async move |this, cx| {
            let clis = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(dialog) = &mut this.ai.dialog {
                    dialog.detecting_clis = false;
                    dialog.detected_clis = clis;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn fetch_ai_models(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.commit_active_provider_fields(cx);
        let Some(dialog) = &mut self.ai.dialog else {
            return;
        };
        let Some(provider) = dialog
            .selected_id
            .as_deref()
            .and_then(|id| dialog.draft.providers.iter().find(|p| p.id == id))
            .cloned()
        else {
            return;
        };
        dialog.loading_models = true;
        dialog.models_error = None;
        let backend = self.ai.backend.clone();
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            if is_http_kind(&provider.kind) {
                backend.list_models(&provider.base_url, &provider.api_key, provider.kind)
            } else {
                backend.list_cli_models(provider.kind, &provider.executable)
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(dialog) = &mut this.ai.dialog {
                    dialog.loading_models = false;
                    match result {
                        Ok(models) => dialog.models = models,
                        Err(error) => dialog.models_error = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn test_selected_ai_provider(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.commit_active_provider_fields(cx);
        let Some(dialog) = &mut self.ai.dialog else {
            return;
        };
        let Some(provider) = dialog
            .selected_id
            .as_deref()
            .and_then(|id| dialog.draft.providers.iter().find(|p| p.id == id))
            .cloned()
        else {
            return;
        };
        dialog.testing = true;
        dialog.test_result = None;
        let backend = self.ai.backend.clone();
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { backend.test_provider(&provider) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(dialog) = &mut this.ai.dialog {
                    dialog.testing = false;
                    match result {
                        Ok(res) => dialog.test_result = Some(res),
                        Err(error) => dialog.models_error = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn save_ai_settings_dialog(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.commit_active_provider_fields(cx);
        let Some(workspace) = &self.workspace else {
            return;
        };
        let Some(dialog) = &mut self.ai.dialog else {
            return;
        };
        if dialog.saving {
            return;
        }
        let next = dialog.draft.clone();
        if let Err(error) = ai::validate(&next) {
            dialog.error = Some(error);
            cx.notify();
            return;
        }
        dialog.saving = true;
        dialog.error = None;
        let root = workspace.reader.root().to_path_buf();
        let expected_path = workspace.info_path();
        let save_next = next.clone();
        cx.notify();

        let save = cx
            .background_executor()
            .spawn(async move { settings::save_ai_settings(&root, &expected_path, save_next) });
        cx.spawn_in(window, async move |this, cx| {
            let result = save.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.ai.settings = next;
                        this.ai.dialog = None;
                        this.status = "AI 配置已保存".into();
                    }
                    Err(error) => {
                        if let Some(dialog) = &mut this.ai.dialog {
                            dialog.saving = false;
                            dialog.error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn start_polish_journal(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.ai.polishing {
            return;
        }
        let Some(edit) = &self.workspace_edit else {
            return;
        };
        let draft = edit.editor.read(cx).value().to_string();
        if draft.trim().is_empty() {
            self.ai.polish_error = Some("日报内容为空，无法润色".into());
            cx.notify();
            return;
        }
        if let Err(error) = ai::default_provider(&self.ai.settings) {
            self.ai.polish_error = Some(error.clone());
            self.status = error.into();
            cx.notify();
            return;
        }

        let date = edit.date().to_string();
        let format_str = match edit.format() {
            Format::Md => "md",
            Format::Txt => "txt",
        };
        self.ai.polish_seq += 1;
        let request_id = format!("polish-{}-{}", date, self.ai.polish_seq);
        self.ai.polishing = true;
        self.ai.polish_request_id = Some(request_id.clone());
        self.ai.polish_error = None;

        let backend = self.ai.backend.clone();
        let runtime = self.ai.runtime.clone();
        let settings = self.ai.settings.clone();
        let req_copy = request_id.clone();
        let source_copy = draft.clone();
        self.status = "正在调用 AI 润色日报…".into();
        cx.notify();

        let task = cx.background_executor().spawn(async move {
            backend.polish(&settings, &source_copy, format_str, &req_copy, &runtime)
        });
        self.ai.polish_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.ai.polish_request_id.as_deref() != Some(request_id.as_str()) {
                    return;
                }
                this.ai.polishing = false;
                this.ai.polish_request_id = None;
                this.ai.polish_task = None;
                match result {
                    Ok(polished) => {
                        let diff = compute_line_diff(&draft, &polished);
                        this.ai.polish_diff = Some(PolishDiffState {
                            date,
                            polished,
                            mode: HistoryDiffMode::Split,
                            diff,
                        });
                        this.status = "AI 润色结果已生成，请核对差异".into();
                    }
                    Err(error) => {
                        this.ai.polish_error = Some(error.clone());
                        this.status = error.into();
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn apply_polish_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(polish) = self.ai.polish_diff.take() else {
            return;
        };
        let Some(edit) = &mut self.workspace_edit else {
            return;
        };
        if edit.date() != polish.date {
            self.status = "编辑日期已变化，未覆盖当前草稿".into();
            cx.notify();
            return;
        }
        edit.editor.update(cx, |state, cx| {
            state.replace_all(polish.polished, window, cx);
        });
        self.status = "已应用 AI 润色内容，等待自动保存".into();
        cx.notify();
    }
}

pub(crate) fn render_ai_settings_dialog(
    ai_state: &AiState,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let Some(dialog) = &ai_state.dialog else {
        return div().into_any_element();
    };
    let theme = cx.theme();
    let background = theme.background;
    let border = theme.border;
    let muted = theme.muted_foreground;
    let accent = theme.primary;
    let secondary = theme.secondary;

    let selected_provider = dialog
        .selected_id
        .as_deref()
        .and_then(|id| dialog.draft.providers.iter().find(|p| p.id == id));

    div()
        .id("ai-settings-backdrop")
        .occlude()
        .absolute()
        .inset_0()
        .bg(background.opacity(0.75))
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("ai-settings-dialog")
                .size_full()
                .max_w(px(900.))
                .max_h(px(640.))
                .bg(background)
                .border_1()
                .border_color(accent)
                .rounded_lg()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_base().child("AI 模型与提示词配置"))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("ai-settings-close")
                                        .label("取消")
                                        .disabled(dialog.saving)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.ai.dialog = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("ai-settings-save")
                                        .label(if dialog.saving { "保存中…" } else { "保存配置" })
                                        .primary()
                                        .disabled(dialog.saving)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.save_ai_settings_dialog(window, cx);
                                        })),
                                ),
                        ),
                )
                .when_some(dialog.error.as_ref(), |el, error| {
                    el.child(div().text_size(px(12.)).text_color(theme.red).child(error.clone()))
                })
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .gap_3()
                        .child(
                            div()
                                .w(px(240.))
                                .flex()
                                .flex_col()
                                .gap_2()
                                .border_r_1()
                                .border_color(border)
                                .pr_3()
                                .child(div().text_xs().text_color(muted).child("服务列表"))
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap_1()
                                        .child(
                                            Button::new("ai-add-http")
                                                .label("+ HTTP")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.add_ai_provider(
                                                        ProviderKind::Http,
                                                        "OpenAI 兼容接口".into(),
                                                        String::new(),
                                                        window,
                                                        cx,
                                                    );
                                                })),
                                        )
                                        .child(
                                            Button::new("ai-add-anthropic")
                                                .label("+ Anthropic")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.add_ai_provider(
                                                        ProviderKind::Anthropic,
                                                        "Anthropic".into(),
                                                        String::new(),
                                                        window,
                                                        cx,
                                                    );
                                                })),
                                        )
                                        .child(
                                            Button::new("ai-detect-cli")
                                                .label(if dialog.detecting_clis {
                                                    "探测中…"
                                                } else {
                                                    "探测本机 CLI"
                                                })
                                                .disabled(dialog.detecting_clis)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.detect_ai_clis(window, cx);
                                                })),
                                        ),
                                )
                                .when(!dialog.detected_clis.is_empty(), |col| {
                                    col.child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .p_1p5()
                                            .rounded_sm()
                                            .bg(secondary.opacity(0.45))
                                            .children(dialog.detected_clis.iter().map(|cli| {
                                                let kind = cli.kind.clone();
                                                let name = cli.name.clone();
                                                let exec = cli.executable.clone();
                                                Button::new(format!("ai-detected-{}", cli.name))
                                                    .label(if cli.cli_available {
                                                        format!("+ {} ({})", cli.name, cli.version)
                                                    } else {
                                                        format!("{} (未就绪)", cli.name)
                                                    })
                                                    .disabled(!cli.cli_available)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.add_ai_provider(
                                                                kind.clone(),
                                                                name.clone(),
                                                                exec.clone(),
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                            })),
                                    )
                                })
                                .child(
                                    div()
                                        .id("ai-providers-scroll")
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_y_scroll()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .children(dialog.draft.providers.iter().map(|provider| {
                                            let is_sel = dialog.selected_id.as_deref()
                                                == Some(provider.id.as_str());
                                            let is_def = dialog.draft.default_provider_id.as_deref()
                                                == Some(provider.id.as_str());
                                            let id = provider.id.clone();
                                            let title = if is_def {
                                                format!("★ {}", provider.name)
                                            } else {
                                                provider.name.clone()
                                            };
                                            Button::new(format!("ai-provider-{}", provider.id))
                                                .label(title)
                                                .toggled(is_sel)
                                                .when(is_sel, Button::primary)
                                                .justify_start()
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.commit_active_provider_fields(cx);
                                                        this.load_provider_fields_into_inputs(
                                                            Some(id.clone()),
                                                            window,
                                                            cx,
                                                        );
                                                        cx.notify();
                                                    },
                                                ))
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("ai-provider-form-scroll")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .when_some(selected_provider, |col, provider| {
                                    let is_def = dialog.draft.default_provider_id.as_deref()
                                        == Some(provider.id.as_str());
                                    let provider_id = provider.id.clone();
                                    let del_id = provider.id.clone();
                                    let http = is_http_kind(&provider.kind);
                                    col.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(provider_kind_label(&provider.kind)),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        Button::new(format!(
                                                            "ai-provider-default-{}",
                                                            provider.id
                                                        ))
                                                        .label(if is_def {
                                                            "默认模型"
                                                        } else {
                                                            "设为默认"
                                                        })
                                                        .toggled(is_def)
                                                        .when(is_def, Button::primary)
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                if let Some(dialog) =
                                                                    &mut this.ai.dialog
                                                                {
                                                                    dialog.draft.default_provider_id =
                                                                        Some(provider_id.clone());
                                                                    cx.notify();
                                                                }
                                                            },
                                                        )),
                                                    )
                                                    .child(
                                                        Button::new("ai-delete-provider")
                                                            .label("删除")
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    let next_sel = {
                                                                        let Some(dialog) =
                                                                            &mut this.ai.dialog
                                                                        else {
                                                                            return;
                                                                        };
                                                                        dialog
                                                                            .draft
                                                                            .providers
                                                                            .retain(|p| p.id != del_id);
                                                                        if dialog
                                                                            .draft
                                                                            .default_provider_id
                                                                            .as_deref()
                                                                            == Some(del_id.as_str())
                                                                        {
                                                                            dialog
                                                                                .draft
                                                                                .default_provider_id =
                                                                                dialog
                                                                                    .draft
                                                                                    .providers
                                                                                    .first()
                                                                                    .map(|p| {
                                                                                        p.id.clone()
                                                                                    });
                                                                        }
                                                                        dialog
                                                                            .draft
                                                                            .providers
                                                                            .first()
                                                                            .map(|p| p.id.clone())
                                                                    };
                                                                    this.load_provider_fields_into_inputs(
                                                                        next_sel,
                                                                        window,
                                                                        cx,
                                                                    );
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    ),
                                            ),
                                    )
                                    .child(div().text_xs().text_color(muted).child("名称"))
                                    .child(Input::new(&dialog.name_input).aria_label("服务名称"))
                                    .when(http, |c| {
                                        c.child(
                                            div().text_xs().text_color(muted).child("接口地址 (Base URL)"),
                                        )
                                        .child(
                                            Input::new(&dialog.base_url_input)
                                                .aria_label("接口地址"),
                                        )
                                    })
                                    .child(
                                        div().text_xs().text_color(muted).child(if http {
                                            "API Key"
                                        } else {
                                            "可执行文件绝对路径"
                                        }),
                                    )
                                    .child(
                                        Input::new(&dialog.secret_or_exec_input).aria_label(
                                            if http { "API Key" } else { "CLI 路径" },
                                        ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(div().text_xs().text_color(muted).child("模型 ID"))
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_1()
                                                    .child(
                                                        Button::new("ai-fetch-models")
                                                            .label(if dialog.loading_models {
                                                                "拉取中…"
                                                            } else {
                                                                "获取模型列表"
                                                            })
                                                            .disabled(dialog.loading_models)
                                                            .on_click(cx.listener(
                                                                |this, _, window, cx| {
                                                                    this.fetch_ai_models(window, cx);
                                                                },
                                                            )),
                                                    )
                                                    .child(
                                                        Button::new("ai-test-provider")
                                                            .label(if dialog.testing {
                                                                "测试中…"
                                                            } else {
                                                                "测试连接"
                                                            })
                                                            .disabled(dialog.testing)
                                                            .on_click(cx.listener(
                                                                |this, _, window, cx| {
                                                                    this.test_selected_ai_provider(
                                                                        window, cx,
                                                                    );
                                                                },
                                                            )),
                                                    ),
                                            ),
                                    )
                                    .child(Input::new(&dialog.model_input).aria_label("模型名称"))
                                    .when(!dialog.models.is_empty(), |c| {
                                        c.child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap_1()
                                                .children(dialog.models.iter().take(12).map(|m| {
                                                    let id = m.id.clone();
                                                    Button::new(format!("ai-model-{}", m.id))
                                                        .label(m.name.clone())
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                if let Some(dialog) =
                                                                    &this.ai.dialog
                                                                {
                                                                    dialog.model_input.update(
                                                                        cx,
                                                                        |input, cx| {
                                                                            input.set_value(
                                                                                id.clone(),
                                                                                window,
                                                                                cx,
                                                                            );
                                                                        },
                                                                    );
                                                                    cx.notify();
                                                                }
                                                            },
                                                        ))
                                                })),
                                        )
                                    })
                                    .when_some(dialog.models_error.as_ref(), |c, err| {
                                        c.child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(theme.red)
                                                .child(err.clone()),
                                        )
                                    })
                                    .when_some(dialog.test_result.as_ref(), |c, res| {
                                        let msg = if res.success {
                                            format!(
                                                "连接成功 ({}ms)：{}",
                                                res.latency_ms, res.reply
                                            )
                                        } else {
                                            format!(
                                                "连接失败：{}",
                                                res.error.as_deref().unwrap_or("未知错误")
                                            )
                                        };
                                        c.child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(if res.success {
                                                    theme.green
                                                } else {
                                                    theme.red
                                                })
                                                .child(msg),
                                        )
                                    })
                                })
                                .child(
                                    div()
                                        .pt_2()
                                        .border_t_1()
                                        .border_color(border)
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(muted)
                                                .child("日报润色偏好（可选）"),
                                        )
                                        .child(
                                            Input::new(&dialog.preference_input)
                                                .aria_label("AI 生成倾向"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(muted)
                                                .child("默认框选总结提示词（可选）"),
                                        )
                                        .child(
                                            Input::new(&dialog.summary_prompt_input)
                                                .aria_label("框选总结提示词"),
                                        ),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

pub(crate) fn render_polish_diff_dialog(
    ai_state: &AiState,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let Some(polish) = &ai_state.polish_diff else {
        return div().into_any_element();
    };
    let theme = cx.theme();
    let background = theme.background;
    let border = theme.border;
    let accent = theme.primary;
    let muted = theme.muted_foreground;

    div()
        .id("polish-diff-backdrop")
        .occlude()
        .absolute()
        .inset_0()
        .bg(background.opacity(0.75))
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("polish-diff-dialog")
                .size_full()
                .max_w(px(980.))
                .max_h(px(640.))
                .bg(background)
                .border_1()
                .border_color(accent)
                .rounded_lg()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(border)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(div().text_base().child("AI 润色结果对比"))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(muted)
                                        .child(format!("日期 {} · 应用后将进入草稿并触发自动保存", polish.date)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("polish-diff-mode-split")
                                        .label("并排")
                                        .toggled(polish.mode == HistoryDiffMode::Split)
                                        .when(polish.mode == HistoryDiffMode::Split, Button::primary)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(diff) = &mut this.ai.polish_diff {
                                                diff.mode = HistoryDiffMode::Split;
                                                cx.notify();
                                            }
                                        })),
                                )
                                .child(
                                    Button::new("polish-diff-mode-unified")
                                        .label("逐行")
                                        .toggled(polish.mode == HistoryDiffMode::Unified)
                                        .when(polish.mode == HistoryDiffMode::Unified, Button::primary)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(diff) = &mut this.ai.polish_diff {
                                                diff.mode = HistoryDiffMode::Unified;
                                                cx.notify();
                                            }
                                        })),
                                )
                                .child(
                                    Button::new("polish-diff-close")
                                        .label("放弃")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.ai.polish_diff = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("polish-diff-apply")
                                        .label("应用润色")
                                        .primary()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.apply_polish_diff(window, cx);
                                        })),
                                ),
                        ),
                )
                .child(render_diff_body(
                    "polish-diff",
                    &polish.diff,
                    polish.mode,
                    "润色前原稿",
                    "润色后正文",
                    cx,
                )),
        )
        .into_any_element()
}

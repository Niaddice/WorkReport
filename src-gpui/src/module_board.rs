use std::{cell::Cell, path::PathBuf, rc::Rc, time::Duration};

use gpui_kit::{
    Animation, AnimationExt, Bounds, Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, SharedString, Window,
    base::{Button as FlatButton, Disableable},
    canvas,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        input::{Input, InputState},
    },
    div, ease_out_quint,
    prelude::*,
    px,
};
use workreport_core::{settings, ui::Preferences as CorePreferences};

use crate::{
    Prototype,
    icons::{PhIcon, module_ph_icon, ph_icon},
    layout::{
        BOARD_GAP, DropSide, EDGE_BAND, EdgeSide, LAYOUT_PRESETS, Layout, LayoutGeometry,
        LayoutGutter, LayoutMetrics, LayoutNode, LayoutSlot, ModuleLayoutState, ModuleType,
        RailSide, Rect, SKINS, SplitDirection, clamp_split_ratio, drop_preview_rect,
        drop_side_for_point, find_drop_slot, layout_geometry, to_core_preferences,
    },
    theme_skin::scaled_px,
    workspace_edit::WorkspaceEdit,
    workspace_view::WorkspaceView,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingModuleDrag {
    pub(super) id: ModuleType,
    pub(super) origin: Point<Pixels>,
}

#[derive(Clone, Debug)]
pub(super) enum DropTarget {
    Module { slot: LayoutSlot, side: DropSide },
    Gutter { gutter: LayoutGutter },
    Edge { side: EdgeSide },
}

#[derive(Clone, Debug)]
pub(super) struct ModuleDrag {
    pub(super) id: ModuleType,
    pub(super) pointer: Point<Pixels>,
    pub(super) drop_target: Option<DropTarget>,
}

#[derive(Clone, Debug)]
pub(super) struct GutterDrag {
    pub(super) path: Vec<usize>,
    pub(super) direction: SplitDirection,
    pub(super) parent: Rect,
    pub(super) ratio: f64,
    pub(super) preview_layout: Option<Layout>,
}

pub(super) struct ModuleBoardState {
    pub(super) board_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(super) pending_drag: Option<PendingModuleDrag>,
    pub(super) module_drag: Option<ModuleDrag>,
    pub(super) gutter_drag: Option<GutterDrag>,
    pub(super) help_module: Option<ModuleType>,
    pub(super) layout_dialog_open: bool,
    pub(super) layout_name_input: Entity<InputState>,
    pub(super) skin_dialog_open: bool,
    pub(super) save_in_flight: bool,
    pub(super) save_pending: Option<(PathBuf, u64, CorePreferences)>,
    pub(super) pending_editor_removal: bool,
}

impl ModuleBoardState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Prototype>) -> Self {
        let layout_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("输入布局名称"));
        Self {
            board_bounds: Rc::new(Cell::new(None)),
            pending_drag: None,
            module_drag: None,
            gutter_drag: None,
            help_module: None,
            layout_dialog_open: false,
            layout_name_input,
            skin_dialog_open: false,
            save_in_flight: false,
            save_pending: None,
            pending_editor_removal: false,
        }
    }

    pub(super) fn reset_transient(&mut self) {
        self.pending_drag = None;
        self.module_drag = None;
        self.gutter_drag = None;
        self.help_module = None;
        self.layout_dialog_open = false;
        self.skin_dialog_open = false;
        self.save_pending = None;
        self.pending_editor_removal = false;
    }

    pub(super) fn board_size(&self) -> (f64, f64) {
        self.board_bounds
            .get()
            .map(|bounds| (f32::from(bounds.size.width) as f64, f32::from(bounds.size.height) as f64))
            .unwrap_or((0.0, 0.0))
    }

    pub(super) fn metrics(&self) -> LayoutMetrics {
        let (width, height) = self.board_size();
        LayoutMetrics {
            width: if width > 0.0 { width } else { 1.0 },
            height: if height > 0.0 { height } else { 1.0 },
            gap: BOARD_GAP as f64,
        }
    }
}

pub(super) fn render_app_rail(
    layout: &ModuleLayoutState,
    board: &ModuleBoardState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let palette = crate::theme_skin::palette_for(&layout.preferences.skin);
    let rail_left = layout.rail_side == RailSide::Left;
    div()
        .id("app-rail")
        .flex()
        .flex_col()
        .items_center()
        .w(px(40.))
        .flex_shrink_0()
        .h_full()
        .py_1()
        .px_1()
        .gap_1()
        .bg(palette.canvas)
        .when(rail_left, |el| el.mr_0p5())
        .children(ModuleType::ALL.into_iter().map(|module| {
            let active = layout.has_module(module);
            let icon_color = if active {
                palette.accent
            } else {
                palette.text_muted
            };
            FlatButton::new(format!("rail-module-{}", module.id()))
                .accessibility_label(module.title())
                .selected(active)
                .disabled(busy)
                .w(px(30.))
                .h(px(30.))
                .rounded(px(6.))
                .bg(if active {
                    palette.tab_active()
                } else {
                    palette.canvas
                })
                .hover(move |s| s.bg(palette.tab_active()))
                .flex()
                .items_center()
                .justify_center()
                .child(ph_icon(module_ph_icon(module), px(18.), icon_color))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.begin_module_drag_pending(module, event.position, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _, window, cx| {
                        this.remove_module_from_layout(module, window, cx);
                    }),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.activate_or_add_module(module, window, cx);
                }))
        }))
        .child(div().h(px(1.)).w(px(20.)).bg(palette.border).my_1())
        .child(
            FlatButton::new("rail-layout")
                .accessibility_label("布局与分栏")
                .selected(board.layout_dialog_open)
                .disabled(busy)
                .w(px(30.))
                .h(px(30.))
                .rounded(px(6.))
                .bg(if board.layout_dialog_open {
                    palette.tab_active()
                } else {
                    palette.canvas
                })
                .hover(move |s| s.bg(palette.tab_active()))
                .flex()
                .items_center()
                .justify_center()
                .child(ph_icon(
                    PhIcon::SquaresFour,
                    px(18.),
                    if board.layout_dialog_open {
                        palette.accent
                    } else {
                        palette.text_muted
                    },
                ))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.module_board.skin_dialog_open = false;
                    this.module_board.layout_dialog_open = !this.module_board.layout_dialog_open;
                    if this.module_board.layout_dialog_open {
                        this.module_board
                            .layout_name_input
                            .update(cx, |input, cx| input.focus(window, cx));
                    }
                    cx.notify();
                })),
        )
        .child(
            FlatButton::new("rail-skin")
                .accessibility_label("切换皮肤")
                .selected(board.skin_dialog_open)
                .disabled(busy)
                .w(px(30.))
                .h(px(30.))
                .rounded(px(6.))
                .bg(if board.skin_dialog_open {
                    palette.tab_active()
                } else {
                    palette.canvas
                })
                .hover(move |s| s.bg(palette.tab_active()))
                .flex()
                .items_center()
                .justify_center()
                .child(ph_icon(
                    PhIcon::Palette,
                    px(18.),
                    if board.skin_dialog_open {
                        palette.accent
                    } else {
                        palette.text_muted
                    },
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.module_board.layout_dialog_open = false;
                    this.module_board.skin_dialog_open = !this.module_board.skin_dialog_open;
                    cx.notify();
                })),
        )
        .child(
            FlatButton::new("rail-refresh")
                .accessibility_label("刷新")
                .disabled(busy)
                .w(px(30.))
                .h(px(30.))
                .rounded(px(6.))
                .bg(palette.canvas)
                .hover(move |s| s.bg(palette.tab_active()))
                .flex()
                .items_center()
                .justify_center()
                .child(ph_icon(PhIcon::ArrowClockwise, px(18.), palette.text_muted))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.refresh_workspace_entries(window, cx);
                    this.directory.invalidate_tree();
                    this.ensure_directory_tree(window, cx);
                    this.git.invalidate();
                    this.status = "已刷新工作空间内容".into();
                    cx.notify();
                })),
        )
        .child(div().flex_1())
        .child(
            FlatButton::new("rail-settings")
                .accessibility_label("打开设置")
                .disabled(busy)
                .w(px(30.))
                .h(px(30.))
                .rounded(px(6.))
                .bg(palette.canvas)
                .hover(move |s| s.bg(palette.tab_active()))
                .flex()
                .items_center()
                .justify_center()
                .child(ph_icon(PhIcon::GearSix, px(18.), palette.text_muted))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.module_board.layout_dialog_open = false;
                    this.module_board.skin_dialog_open = false;
                    this.open_settings_section(
                        crate::settings_view::SettingsSection::Appearance,
                        window,
                        cx,
                    );
                })),
        )
}

#[inline(never)]
pub(super) fn render_workspace_shell(
    app: &Prototype,
    workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let rail = render_app_rail(&app.module_layout, &app.module_board, busy, cx);
    let board = render_module_board(app, workspace, busy, cx);
    let rail_left = app.module_layout.rail_side == RailSide::Left;

    let body = if rail_left {
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(px(6.))
            .child(rail)
            .child(board)
    } else {
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap(px(6.))
            .child(board)
            .child(rail)
    };

    body.into_any_element()
}

#[inline(never)]
fn render_module_board(
    app: &Prototype,
    workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let board = &app.module_board;
    let layout_state = &app.module_layout;
    let active_layout = board
        .gutter_drag
        .as_ref()
        .and_then(|drag| drag.preview_layout.as_ref())
        .unwrap_or(layout_state.layout())
        .clone();
    let bounds_cell = board.board_bounds.clone();
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let primary = cx.theme().primary;
    let background = cx.theme().background;
    let card = cx.theme().background;

    let board_content = if let Some(maximized) = layout_state.maximized {
        div()
            .size_full()
            .p_1()
            .bg(background)
            .child(render_module_panel(app, maximized, true, workspace, busy, cx))
            .into_any_element()
    } else {
        match &active_layout {
            Layout::Empty => div()
                .id("module-board-empty")
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .border_1()
                .border_color(border)
                .rounded_md()
                .child(div().text_sm().child("暂无打开的模块"))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(muted)
                        .child("点击侧边功能栏中的模块按钮即可打开。"),
                )
                .into_any_element(),
            Layout::Node(root) => {
                render_layout_node(app, root, Vec::new(), workspace, busy, cx).into_any_element()
            }
        }
    };

    let (board_w, board_h) = board.board_size();
    let drop_overlay = board.module_drag.as_ref().and_then(|drag| {
        drag.drop_target.as_ref().map(|target| {
            let (rect, label) = match target {
                DropTarget::Module { slot, side } => (
                    drop_preview_rect(slot, *side),
                    match side {
                        DropSide::Left => "左侧分栏",
                        DropSide::Right => "右侧分栏",
                        DropSide::Top => "上方分栏",
                        DropSide::Bottom => "下方分栏",
                    },
                ),
                DropTarget::Gutter { gutter } => {
                    let rect = if gutter.direction == SplitDirection::Row {
                        Rect {
                            left: gutter.rect.left + gutter.rect.width / 2.0 - 9.0,
                            top: gutter.rect.top,
                            width: 18.0,
                            height: gutter.rect.height,
                        }
                    } else {
                        Rect {
                            left: gutter.rect.left,
                            top: gutter.rect.top + gutter.rect.height / 2.0 - 9.0,
                            width: gutter.rect.width,
                            height: 18.0,
                        }
                    };
                    (rect, "插入此处")
                }
                DropTarget::Edge { side } => {
                    let rect = match side {
                        EdgeSide::Left => Rect {
                            left: 0.0,
                            top: 0.0,
                            width: EDGE_BAND,
                            height: board_h.max(40.0),
                        },
                        EdgeSide::Right => Rect {
                            left: (board_w - EDGE_BAND).max(0.0),
                            top: 0.0,
                            width: EDGE_BAND,
                            height: board_h.max(40.0),
                        },
                    };
                    (
                        rect,
                        match side {
                            EdgeSide::Left => "放到最左",
                            EdgeSide::Right => "放到最右",
                        },
                    )
                }
            };
            div()
                .id("module-drop-preview")
                .absolute()
                .left(px(rect.left as f32))
                .top(px(rect.top as f32))
                .w(px(rect.width as f32))
                .h(px(rect.height as f32))
                .border_2()
                .border_color(primary)
                .bg(primary.opacity(0.14))
                .rounded_md()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .bg(card)
                        .border_1()
                        .border_color(primary)
                        .text_size(px(12.))
                        .child(label),
                )
        })
    });

    let drag_ghost = board.module_drag.as_ref().and_then(|drag| {
        board.board_bounds.get().map(|bounds| {
            let local_x = f32::from(drag.pointer.x - bounds.origin.x) + 14.0;
            let local_y = f32::from(drag.pointer.y - bounds.origin.y) + 10.0;
            div()
                .id("module-drag-ghost")
                .absolute()
                .left(px(local_x.max(0.0)))
                .top(px(local_y.max(0.0)))
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(card)
                .border_1()
                .border_color(primary)
                .text_size(px(12.))
                .child(drag.id.title())
        })
    });

    div()
        .id("module-board")
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
            this.on_board_mouse_move(event, cx);
        }))
        .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, window, cx| {
            if event.button == MouseButton::Left {
                this.finish_board_pointer(window, cx);
            }
        }))
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                this.finish_board_pointer(window, cx);
            }),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .child(
                    canvas(
                        move |bounds, _, _| bounds,
                        move |bounds, _, _, _| {
                            bounds_cell.set(Some(bounds));
                        },
                    )
                    .size_full(),
                ),
        )
        .child(board_content)
        .children(drop_overlay)
        .children(drag_ghost)
        .when(board.layout_dialog_open, |container| {
            container.child(
                div()
                    .size_full()
                    .absolute()
                    .inset_0()
                    .child(render_layout_dialog(layout_state, board, busy, cx))
                    .with_animation(
                        "dialog-anim-layout",
                        Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                        |el, t| el.opacity(0.25 + 0.75 * t),
                    ),
            )
        })
        .when(board.skin_dialog_open, |container| {
            container.child(
                div()
                    .size_full()
                    .absolute()
                    .inset_0()
                    .child(render_skin_dialog(layout_state, busy, cx))
                    .with_animation(
                        "dialog-anim-skin",
                        Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                        |el, t| el.opacity(0.25 + 0.75 * t),
                    ),
            )
        })
        .into_any_element()
}

fn render_layout_node(
    app: &Prototype,
    node: &LayoutNode,
    path: Vec<usize>,
    workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    match node {
        LayoutNode::Leaf { id } => div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .size_full()
            .child(render_module_panel(app, *id, false, workspace, busy, cx))
            .into_any_element(),
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let mut first_path = path.clone();
            first_path.push(0);
            let mut second_path = path.clone();
            second_path.push(1);
            let gutter_path = path;
            let gutter_key: String = if gutter_path.is_empty() {
                "root".into()
            } else {
                gutter_path
                    .iter()
                    .map(|step| step.to_string())
                    .collect::<Vec<_>>()
                    .join("-")
            };
            let is_row = *direction == SplitDirection::Row;
            let ratio_f32 = *ratio as f32;
            let second_ratio_f32 = (1.0 - *ratio) as f32;
            let accent = cx.theme().primary;

            let gutter_el = FlatButton::new(format!("module-gutter-{gutter_key}"))
                .accessibility_label(if is_row {
                    "左右拖动调整两侧模块宽度"
                } else {
                    "上下拖动调整两侧模块高度"
                })
                .flex_shrink_0()
                .rounded(px(2.))
                .hover(move |s| s.bg(accent.opacity(0.35)))
                .when(is_row, |el| el.w(px(BOARD_GAP)).h_full())
                .when(!is_row, |el| el.h(px(BOARD_GAP)).w_full())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.begin_gutter_drag(gutter_path.clone(), cx);
                    }),
                );

            let first_slot = div()
                .flex_basis(px(0.))
                .flex_grow(ratio_f32)
                .min_w_0()
                .min_h_0()
                .when(is_row, |el| el.h_full())
                .when(!is_row, |el| el.w_full())
                .child(render_layout_node(app, first, first_path, workspace, busy, cx));

            let second_slot = div()
                .flex_basis(px(0.))
                .flex_grow(second_ratio_f32)
                .min_w_0()
                .min_h_0()
                .when(is_row, |el| el.h_full())
                .when(!is_row, |el| el.w_full())
                .child(render_layout_node(app, second, second_path, workspace, busy, cx));

            div()
                .flex()
                .size_full()
                .min_w_0()
                .min_h_0()
                .when(is_row, |el| el.flex_row())
                .when(!is_row, |el| el.flex_col())
                .child(first_slot)
                .child(gutter_el)
                .child(second_slot)
                .into_any_element()
        }
    }
}

#[inline(never)]
fn render_module_panel(
    app: &Prototype,
    module: ModuleType,
    maximized: bool,
    workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    use chrono::Datelike;

    let scale = app.module_layout.scale;
    let active = app.module_layout.active_module == Some(module);
    let dragging = app
        .module_board
        .module_drag
        .as_ref()
        .is_some_and(|drag| drag.id == module);
    let help_open = app.module_board.help_module == Some(module);
    let palette = crate::theme_skin::palette_for(&app.module_layout.preferences.skin);
    let border = if active && !dragging {
        palette.accent_border
    } else {
        palette.border
    };
    let header_bg = palette.surface_header;
    let muted = palette.text_muted;
    let tab_bg = if active {
        palette.tab_active()
    } else {
        header_bg
    };
    let icon_color = if active {
        palette.accent
    } else {
        muted
    };

    let format_cn_date = |d: chrono::NaiveDate| -> SharedString {
        let weekdays = [
            "星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日",
        ];
        let wd = weekdays[d.weekday().num_days_from_monday() as usize];
        format!("{wd} {} 月 {} 日", d.month(), d.day()).into()
    };

    let subtitle: Option<SharedString> = match module {
        ModuleType::Editor => {
            if app.directory.open_doc.is_some() {
                Some("文件".into())
            } else if let Some(range) = &app.workspace_range {
                Some(format!("范围 · {}", range.label()).into())
            } else if let Some(edit) = &app.workspace_edit {
                chrono::NaiveDate::parse_from_str(edit.date(), "%Y%m%d")
                    .ok()
                    .map(format_cn_date)
                    .or_else(|| Some(edit.filename().to_owned().into()))
            } else {
                Some(format_cn_date(app.calendar.selected_date()))
            }
        }
        _ => None,
    };

    let body = render_module_body(app, module, workspace, app.workspace_edit.as_ref(), busy, cx);
    let body_bg = match module {
        ModuleType::Editor => palette.surface,
        _ => palette.surface_header,
    };
    let flush_body = matches!(
        module,
        ModuleType::Calendar | ModuleType::Editor | ModuleType::Summary
    );

    div()
        .id(format!("module-panel-{}", module.id()))
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_w_0()
        .min_h_0()
        .border_1()
        .border_color(border)
        .rounded(px(8.))
        .bg(body_bg)
        .overflow_hidden()
        .when(dragging, |panel| panel.opacity(0.55))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.module_layout.activate(module);
                cx.notify();
            }),
        )
        .child(
            div()
                .id(format!("module-panel-{}-header", module.id()))
                .flex()
                .items_center()
                .justify_between()
                .gap_1()
                .h(px(36.))
                .px_1p5()
                .bg(header_bg)
                .border_b_1()
                .border_color(palette.border)
                .child(
                    FlatButton::new(format!("module-panel-{}-drag", module.id()))
                        .accessibility_label(format!("拖动「{}」模块", module.title()))
                        .flex_1()
                        .min_w_0()
                        .h(px(26.))
                        .flex()
                        .items_center()
                        .justify_start()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .min_w_0()
                                .h(px(26.))
                                .px_2()
                                .rounded(px(6.))
                                .bg(tab_bg)
                                .child(ph_icon(module_ph_icon(module), px(15.), icon_color))
                                .child(
                                    div()
                                        .text_size(scaled_px(13., scale))
                                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                        .text_color(palette.text)
                                        .child(module.title()),
                                )
                                .when_some(subtitle, |row, subtitle| {
                                    row.child(
                                        div()
                                            .text_size(scaled_px(12., scale))
                                            .text_color(muted)
                                            .truncate()
                                            .child(subtitle),
                                    )
                                }),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                this.begin_module_drag_pending(module, event.position, cx);
                            }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_0p5()
                        .when_some(module_settings_section(module), |row, target_section| {
                            row.child(
                                FlatButton::new(format!("module-panel-{}-settings", module.id()))
                                    .accessibility_label(format!("打开「{}」设置", module.title()))
                                    .disabled(busy)
                                    .h(px(24.))
                                    .w(px(24.))
                                    .rounded(px(5.))
                                    .bg(header_bg)
                                    .hover(move |s| s.bg(palette.tab_active()))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(PhIcon::GearSix, px(15.), muted))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_settings_section(target_section, window, cx);
                                    })),
                            )
                        })
                        .child(
                            FlatButton::new(format!("module-panel-{}-help", module.id()))
                                .accessibility_label(format!("查看「{}」使用说明", module.title()))
                                .selected(help_open)
                                .h(px(24.))
                                .w(px(24.))
                                .rounded(px(5.))
                                .bg(if help_open {
                                    palette.tab_active()
                                } else {
                                    header_bg
                                })
                                .hover(move |s| s.bg(palette.tab_active()))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(ph_icon(
                                    PhIcon::Question,
                                    px(15.),
                                    if help_open { palette.accent } else { muted },
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.module_board.help_module =
                                        if this.module_board.help_module == Some(module) {
                                            None
                                        } else {
                                            Some(module)
                                        };
                                    cx.notify();
                                })),
                        )
                        .child(
                            FlatButton::new(format!("module-panel-{}-maximize", module.id()))
                                .accessibility_label(if maximized {
                                    format!("还原「{}」模块大小", module.title())
                                } else {
                                    format!("放大「{}」模块", module.title())
                                })
                                .h(px(24.))
                                .w(px(24.))
                                .rounded(px(5.))
                                .bg(if maximized {
                                    palette.tab_active()
                                } else {
                                    header_bg
                                })
                                .hover(move |s| s.bg(palette.tab_active()))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(ph_icon(
                                    if maximized {
                                        PhIcon::CornersIn
                                    } else {
                                        PhIcon::CornersOut
                                    },
                                    px(15.),
                                    if maximized { palette.accent } else { muted },
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.module_layout.toggle_maximize(module);
                                    cx.notify();
                                })),
                        )
                        .child(
                            FlatButton::new(format!("module-panel-{}-remove", module.id()))
                                .accessibility_label(format!("从首页移除「{}」模块", module.title()))
                                .disabled(busy)
                                .h(px(24.))
                                .w(px(24.))
                                .rounded(px(5.))
                                .bg(header_bg)
                                .hover(move |s| s.bg(palette.danger.opacity(0.14)))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(ph_icon(PhIcon::X, px(15.), muted))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.remove_module_from_layout(module, window, cx);
                                })),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .bg(body_bg)
                .when(!flush_body, |el| el.p_2())
                .child(body),
        )
        .when(help_open, |panel| {
            panel.child(
                div()
                    .id(format!("module-panel-{}-help-body", module.id()))
                    .occlude()
                    .absolute()
                    .top(px(38.))
                    .right(px(8.))
                    .w(px(300.))
                    .max_w_full()
                    .p_3()
                    .rounded(px(8.))
                    .bg(palette.surface)
                    .border_1()
                    .border_color(palette.accent_border)
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(scaled_px(12.5, scale))
                                    .text_color(palette.text)
                                    .child(format!("{} · 使用说明", module.title())),
                            )
                            .child(
                                FlatButton::new(format!("module-panel-{}-help-close", module.id()))
                                    .h(px(20.))
                                    .w(px(20.))
                                    .rounded(px(4.))
                                    .bg(palette.surface)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(ph_icon(PhIcon::X, px(13.), muted))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.module_board.help_module = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .children(module.help().iter().map(|line| {
                        div()
                            .text_size(scaled_px(11.5, scale))
                            .text_color(muted)
                            .child(format!("• {line}"))
                    })),
            )
        })
        .into_any_element()
}

fn module_settings_section(module: ModuleType) -> Option<crate::settings_view::SettingsSection> {
    use crate::settings_view::SettingsSection;
    match module {
        ModuleType::Calendar => Some(SettingsSection::Holidays),
        ModuleType::Editor => Some(SettingsSection::Editor),
        ModuleType::Directory => Some(SettingsSection::WorkspaceFiles),
        ModuleType::Summary => Some(SettingsSection::Summary),
        ModuleType::Excel => Some(SettingsSection::Excel),
        ModuleType::Git => Some(SettingsSection::Git),
        ModuleType::Overview | ModuleType::History => None,
    }
}

#[inline(never)]
fn render_module_body(
    app: &Prototype,
    module: ModuleType,
    workspace: &WorkspaceView,
    edit: Option<&WorkspaceEdit>,
    busy: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    match module {
        ModuleType::Calendar => {
            let (board_w, _) = app.module_board.board_size();
            let panel_width = if board_w > 0.0 {
                layout_geometry(app.module_layout.layout(), board_w, 600.0, BOARD_GAP as f64, None)
                    .slots
                    .into_iter()
                    .find(|slot| slot.id == ModuleType::Calendar)
                    .map(|slot| slot.rect.width)
                    .unwrap_or(board_w)
            } else {
                500.0
            };
            workspace
                .render_calendar_module(
                    busy,
                    crate::calendar_view::CalendarPresentation {
                        selection: app
                            .calendar_drag
                            .as_ref()
                            .map(|drag| &drag.selection)
                            .unwrap_or(&app.calendar),
                        year_scroll: &app.calendar_year_scroll,
                        details: &app.calendar_details,
                    },
                    panel_width >= 430.0,
                    cx,
                )
                .into_any_element()
        }
        ModuleType::Editor => {
            if let Some(edit) = edit {
                edit.render(busy, app.ai.polishing, cx).into_any_element()
            } else if let Some(open_doc) = &app.directory.open_doc {
                crate::directory::render_workspace_document_editor(open_doc, busy, cx)
                    .into_any_element()
            } else {
                workspace
                    .render_editor_module(
                        busy,
                        &app.calendar,
                        app.workspace_range.as_ref(),
                        app.range_source,
                        cx,
                    )
                    .into_any_element()
            }
        }
        ModuleType::Overview => {
            crate::overview::render_overview_module(app, workspace, busy, cx).into_any_element()
        }
        ModuleType::History => workspace.render_history_module(busy, cx).into_any_element(),
        ModuleType::Summary => {
            crate::summary_view::render_summary_module(app, workspace, busy, cx).into_any_element()
        }
        ModuleType::Directory => {
            crate::directory::render_directory_module(app, workspace, busy, cx).into_any_element()
        }
        ModuleType::Excel => {
            crate::excel_view::render_excel_module(app, workspace, busy, cx).into_any_element()
        }
        ModuleType::Git => {
            crate::git_view::render_git_module(app, workspace, busy, cx).into_any_element()
        }
    }
}

fn render_layout_dialog(
    layout_state: &ModuleLayoutState,
    board: &ModuleBoardState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let border = cx.theme().border;
    let card = cx.theme().background;
    let muted = cx.theme().muted_foreground;
    let background = cx.theme().background;
    div()
        .id("layout-dialog-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .bg(background.opacity(0.65))
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("layout-dialog")
                .w(px(440.))
                .max_h(px(520.))
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .bg(card)
                .border_1()
                .border_color(border)
                .rounded_md()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_base().child("布局与分栏"))
                        .child(
                            Button::new("layout-dialog-close")
                                .label("关闭")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.module_board.layout_dialog_open = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child("拖动模块标题栏到其他模块上分栏，拖到间隙处插入，或拖动分栏间隙调整比例。"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().child("推荐布局"))
                        .children(LAYOUT_PRESETS.iter().map(|preset| {
                            let preset_id = preset.id;
                            Button::new(format!("layout-preset-{preset_id}"))
                                .label(format!("{} — {}", preset.name, preset.description))
                                .disabled(busy)
                                .w_full()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.apply_layout_preset(preset_id, cx);
                                }))
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().child("保存当前布局"))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    Input::new(&board.layout_name_input)
                                        .aria_label("布局名称")
                                        .disabled(busy)
                                        .flex_1(),
                                )
                                .child(
                                    Button::new("layout-save-button")
                                        .label("保存布局")
                                        .primary()
                                        .disabled(busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.save_named_layout(window, cx);
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .id("layout-saved-list")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .max_h(px(160.))
                        .overflow_y_scroll()
                        .child(div().text_sm().child("已保存布局"))
                        .when(layout_state.preferences.saved_layouts.is_empty(), |list| {
                            list.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(muted)
                                    .child("还没有保存的布局。"),
                            )
                        })
                        .children(layout_state.preferences.saved_layouts.iter().enumerate().map(
                            |(index, saved)| {
                                let name = saved.name.clone();
                                let apply_name = name.clone();
                                let remove_name = name.clone();
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Button::new(format!("layout-saved-apply-{index}"))
                                            .label(name)
                                            .disabled(busy)
                                            .flex_1()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.apply_saved_layout(&apply_name, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new(format!("layout-saved-remove-{index}"))
                                            .label("删除")
                                            .disabled(busy)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.remove_saved_layout(&remove_name, cx);
                                            })),
                                    )
                            },
                        )),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            Button::new("layout-reset-button")
                                .label("恢复默认布局")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reset_module_layout(cx);
                                })),
                        )
                        .when_some(layout_state.error.as_ref(), |row, error| {
                            row.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(cx.theme().danger)
                                    .child(error.clone()),
                            )
                        }),
                ),
        )
}

fn render_skin_dialog(
    layout_state: &ModuleLayoutState,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let scale = layout_state.scale;
    let active_palette = crate::theme_skin::palette_for(&layout_state.preferences.skin);
    let current_skin = layout_state.preferences.skin.as_str();
    div()
        .id("skin-dialog-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .bg(active_palette.canvas.opacity(0.68))
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("skin-dialog")
                .w(px(580.))
                .max_w_full()
                .max_h(px(520.))
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .bg(active_palette.surface)
                .border_1()
                .border_color(active_palette.border)
                .rounded(px(10.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(ph_icon(PhIcon::Palette, px(18.), active_palette.accent))
                                        .child(
                                            div()
                                                .text_size(scaled_px(16., scale))
                                                .text_color(active_palette.text)
                                                .child("切换皮肤"),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(scaled_px(12., scale))
                                        .text_color(active_palette.text_muted)
                                        .child("选择后立即应用到当前工作空间，无需保存设置。"),
                                ),
                        )
                        .child(
                            Button::new("skin-dialog-close")
                                .label("关闭")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.module_board.skin_dialog_open = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .id("skin-dialog-list")
                        .flex()
                        .flex_col()
                        .gap_2()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(SKINS.chunks(2).map(|pair| {
                            div()
                                .flex()
                                .gap_2()
                                .children(pair.iter().map(|skin| {
                                    let selected = current_skin == skin.id;
                                    let skin_id = skin.id;
                                    let sw = crate::theme_skin::palette_for(skin_id);
                                    let mode_text = if skin.dark { "深色" } else { "浅色" };
                                    FlatButton::new(format!("skin-option-{skin_id}"))
                                        .accessibility_label(format!(
                                            "{}（{}）— {}",
                                            skin.name, mode_text, skin.description
                                        ))
                                        .selected(selected)
                                        .disabled(busy)
                                        .flex_1()
                                        .min_w_0()
                                        .min_h(px(68.))
                                        .p_2()
                                        .rounded(px(8.))
                                        .border_1()
                                        .border_color(if selected {
                                            active_palette.accent
                                        } else {
                                            active_palette.border
                                        })
                                        .bg(if selected {
                                            active_palette.accent_soft
                                        } else {
                                            active_palette.surface
                                        })
                                        .hover(move |s| s.border_color(active_palette.accent_border))
                                        .flex()
                                        .items_center()
                                                .gap_2p5()
                                                .w_full()
                                                .min_w_0()
                                                .child(
                                                    // Mini window preview swatch (canvas + toolbar dots + calendar/editor panes)
                                                    div()
                                                        .w(px(60.))
                                                        .h(px(44.))
                                                        .flex_shrink_0()
                                                        .rounded(px(6.))
                                                        .p_1()
                                                        .bg(sw.canvas)
                                                        .border_1()
                                                        .border_color(sw.border)
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap_0p5()
                                                                .child(
                                                                    div()
                                                                        .w(px(5.))
                                                                        .h(px(5.))
                                                                        .rounded(px(3.))
                                                                        .bg(sw.accent_fill),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .w(px(14.))
                                                                        .h(px(3.))
                                                                        .rounded(px(2.))
                                                                        .bg(sw.text_muted),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .flex_1()
                                                                .gap_1()
                                                                .child(
                                                                    div()
                                                                        .w(px(22.))
                                                                        .h_full()
                                                                        .rounded(px(3.))
                                                                        .bg(sw.surface)
                                                                        .border_1()
                                                                        .border_color(sw.border),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .flex_1()
                                                                        .h_full()
                                                                        .rounded(px(3.))
                                                                        .bg(sw.surface)
                                                                        .border_1()
                                                                        .border_color(sw.accent_border),
                                                                ),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .items_start()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .gap_0p5()
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap_1p5()
                                                                .w_full()
                                                                .child(
                                                                    div()
                                                                        .text_size(scaled_px(13., scale))
                                                                        .text_color(active_palette.text)
                                                                        .truncate()
                                                                        .child(skin.name),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .px_1p5()
                                                                        .py_0p5()
                                                                        .rounded(px(4.))
                                                                        .bg(active_palette.subtle)
                                                                        .text_size(scaled_px(10., scale))
                                                                        .text_color(active_palette.text_muted)
                                                                        .child(mode_text),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(scaled_px(11., scale))
                                                                .text_color(active_palette.text_muted)
                                                                .truncate()
                                                                .child(skin.description),
                                                        ),
                                                )
                                                .when(selected, |row| {
                                                    row.child(ph_icon(
                                                        PhIcon::Check,
                                                        px(15.),
                                                        active_palette.accent,
                                                    ))
                                                })
                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                    this.set_module_skin(skin_id, window, cx);
                                                }))
                                }))
                        })),
                ),
        )
}

impl Prototype {
    pub(super) fn cancel_module_interactions(&mut self, cx: &mut Context<Self>) {
        let had = self.module_board.pending_drag.take().is_some()
            | self.module_board.module_drag.take().is_some()
            | self.module_board.gutter_drag.take().is_some();
        if had {
            cx.notify();
        }
    }

    pub(super) fn begin_module_drag_pending(
        &mut self,
        id: ModuleType,
        origin: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy
            || self.module_layout.maximized.is_some()
            || self.module_board.gutter_drag.is_some()
        {
            return;
        }
        self.module_board.pending_drag = Some(PendingModuleDrag { id, origin });
        cx.notify();
    }

    pub(super) fn begin_gutter_drag(&mut self, path: Vec<usize>, cx: &mut Context<Self>) {
        if self.workspace_busy
            || self.module_layout.maximized.is_some()
            || self.module_board.module_drag.is_some()
        {
            return;
        }
        let (width, height) = self.module_board.board_size();
        let geometry = layout_geometry(
            self.module_layout.layout(),
            width,
            height,
            BOARD_GAP as f64,
            None,
        );
        let Some(gutter) = geometry.gutters.into_iter().find(|g| g.path == path) else {
            return;
        };
        self.module_board.pending_drag = None;
        self.module_board.gutter_drag = Some(GutterDrag {
            path: gutter.path,
            direction: gutter.direction,
            parent: gutter.parent,
            ratio: gutter.ratio,
            preview_layout: None,
        });
        cx.notify();
    }

    fn compute_drop_target(
        layout: &Layout,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        dragged: ModuleType,
    ) -> Option<DropTarget> {
        let width = f32::from(bounds.size.width) as f64;
        let height = f32::from(bounds.size.height) as f64;
        let x = f32::from(position.x - bounds.origin.x) as f64;
        let y = f32::from(position.y - bounds.origin.y) as f64;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        if x <= EDGE_BAND {
            return Some(DropTarget::Edge {
                side: EdgeSide::Left,
            });
        }
        if x >= width - EDGE_BAND {
            return Some(DropTarget::Edge {
                side: EdgeSide::Right,
            });
        }
        let LayoutGeometry { slots, gutters } =
            layout_geometry(layout, width, height, BOARD_GAP as f64, None);
        if let Some(gutter) = gutters.into_iter().find(|item| {
            x >= item.rect.left - 3.0
                && x <= item.rect.left + item.rect.width + 3.0
                && y >= item.rect.top - 3.0
                && y <= item.rect.top + item.rect.height + 3.0
        }) {
            return Some(DropTarget::Gutter { gutter });
        }
        find_drop_slot(&slots, x, y, Some(dragged)).map(|slot| DropTarget::Module {
            side: drop_side_for_point(&slot, x, y),
            slot,
        })
    }

    fn on_board_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.module_board.gutter_drag.is_some() {
            if event.pressed_button != Some(MouseButton::Left) {
                self.finish_gutter_drag(cx);
                return;
            }
            let Some(bounds) = self.module_board.board_bounds.get() else {
                return;
            };
            let metrics = self.module_board.metrics();
            let (direction, parent, prev_ratio, path) = {
                let gutter_drag = self.module_board.gutter_drag.as_ref().unwrap();
                (
                    gutter_drag.direction,
                    gutter_drag.parent,
                    gutter_drag.ratio,
                    gutter_drag.path.clone(),
                )
            };
            let local = if direction == SplitDirection::Row {
                f32::from(event.position.x - bounds.origin.x) as f64
            } else {
                f32::from(event.position.y - bounds.origin.y) as f64
            };
            let origin = if direction == SplitDirection::Row {
                parent.left
            } else {
                parent.top
            };
            let span = if direction == SplitDirection::Row {
                parent.width - BOARD_GAP as f64
            } else {
                parent.height - BOARD_GAP as f64
            };
            if span <= 0.0 {
                return;
            }
            let ratio = clamp_split_ratio((local - origin) / span);
            if (ratio - prev_ratio).abs() < 1e-4 {
                return;
            }
            let preview = crate::layout::resize_layout_locally(
                self.module_layout.layout(),
                &path,
                ratio,
                metrics,
            );
            if let Some(gutter_drag) = &mut self.module_board.gutter_drag {
                gutter_drag.ratio = ratio;
                gutter_drag.preview_layout = Some(preview);
            }
            cx.notify();
            return;
        }

        if let Some(pending) = self.module_board.pending_drag {
            if event.pressed_button != Some(MouseButton::Left) {
                self.module_board.pending_drag = None;
                return;
            }
            let dx = f32::from(event.position.x - pending.origin.x);
            let dy = f32::from(event.position.y - pending.origin.y);
            if (dx * dx + dy * dy).sqrt() < 4.0 {
                return;
            }
            self.module_board.pending_drag = None;
            let drop_target = self.module_board.board_bounds.get().and_then(|bounds| {
                Self::compute_drop_target(
                    self.module_layout.layout(),
                    bounds,
                    event.position,
                    pending.id,
                )
            });
            self.module_board.module_drag = Some(ModuleDrag {
                id: pending.id,
                pointer: event.position,
                drop_target,
            });
            cx.notify();
            return;
        }

        if let Some(drag) = &mut self.module_board.module_drag {
            if event.pressed_button != Some(MouseButton::Left) {
                self.finish_module_drag(cx);
                return;
            }
            let bounds = self.module_board.board_bounds.get();
            let drop_target = bounds.and_then(|bounds| {
                Self::compute_drop_target(
                    self.module_layout.layout(),
                    bounds,
                    event.position,
                    drag.id,
                )
            });
            if let Some(drag) = &mut self.module_board.module_drag {
                drag.pointer = event.position;
                drag.drop_target = drop_target;
            }
            cx.notify();
        }
    }

    fn finish_gutter_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.module_board.gutter_drag.take() else {
            return;
        };
        let metrics = self.module_board.metrics();
        if self.module_layout.resize(&drag.path, drag.ratio, metrics) {
            self.status = "已调整分栏比例".into();
            self.persist_ui_preferences(cx);
        }
        cx.notify();
    }

    fn finish_module_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.module_board.module_drag.take() else {
            return;
        };
        let outside = match self.module_board.board_bounds.get() {
            Some(bounds) => {
                let x = f32::from(drag.pointer.x);
                let y = f32::from(drag.pointer.y);
                let left = f32::from(bounds.origin.x);
                let top = f32::from(bounds.origin.y);
                let right = left + f32::from(bounds.size.width);
                let bottom = top + f32::from(bounds.size.height);
                x < left - 60.0 || x > right + 60.0 || y < top - 60.0 || y > bottom + 60.0
            }
            None => true,
        };
        if outside {
            self.status = format!("已取消移动「{}」模块", drag.id.title()).into();
            cx.notify();
            return;
        }
        let changed = match drag.drop_target {
            Some(DropTarget::Edge { side }) => self.module_layout.move_to_edge(drag.id, side),
            Some(DropTarget::Gutter { gutter }) => {
                self.module_layout.insert_at_gutter(drag.id, &gutter.path)
            }
            Some(DropTarget::Module { slot, side }) if slot.id != drag.id => {
                self.module_layout.split(drag.id, slot.id, side)
            }
            _ => false,
        };
        if changed {
            self.status = format!("已更新「{}」模块布局", drag.id.title()).into();
            self.persist_ui_preferences(cx);
        }
        cx.notify();
    }

    fn finish_board_pointer(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.module_board.pending_drag = None;
        if self.module_board.gutter_drag.is_some() {
            self.finish_gutter_drag(cx);
        } else if self.module_board.module_drag.is_some() {
            self.finish_module_drag(cx);
        }
    }

    pub(super) fn activate_or_add_module(
        &mut self,
        module: ModuleType,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy {
            return;
        }
        if let Some(max) = self.module_layout.maximized {
            if max != module {
                self.module_layout.restore_maximized();
            }
        }
        if self.module_layout.has_module(module) {
            self.module_layout.activate(module);
        } else if self.module_layout.add(module) {
            self.persist_ui_preferences(cx);
        }
        cx.notify();
    }

    pub(super) fn ensure_editor_module_visible(&mut self, cx: &mut Context<Self>) {
        if let Some(max) = self.module_layout.maximized {
            if max != ModuleType::Editor {
                self.module_layout.restore_maximized();
            }
        }
        if !self.module_layout.has_module(ModuleType::Editor) && self.module_layout.add(ModuleType::Editor) {
            self.persist_ui_preferences(cx);
        } else {
            self.module_layout.activate(ModuleType::Editor);
        }
    }

    pub(super) fn remove_module_from_layout(
        &mut self,
        module: ModuleType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy {
            return;
        }
        if module == ModuleType::Editor && self.workspace_edit.is_some() {
            self.module_board.pending_editor_removal = true;
            if !self.allow_workspace_close(false, window, cx) {
                return;
            }
            self.module_board.pending_editor_removal = false;
        }
        if self.module_board.help_module == Some(module) {
            self.module_board.help_module = None;
        }
        if self.module_layout.remove(module) {
            self.persist_ui_preferences(cx);
        }
        cx.notify();
    }

    pub(super) fn complete_pending_editor_removal(&mut self, cx: &mut Context<Self>) {
        if self.module_board.pending_editor_removal {
            self.module_board.pending_editor_removal = false;
            if self.module_board.help_module == Some(ModuleType::Editor) {
                self.module_board.help_module = None;
            }
            if self.module_layout.remove(ModuleType::Editor) {
                self.persist_ui_preferences(cx);
            }
        }
    }

    pub(super) fn save_named_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.module_board.layout_name_input.read(cx).value().to_string();
        if !self.module_layout.save_layout(&name) {
            self.status = "请输入有效的布局名称（1-50 个字符）".into();
            cx.notify();
            return;
        }
        self.module_board
            .layout_name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status = format!("已保存布局「{}」", name.trim()).into();
        self.persist_ui_preferences(cx);
        cx.notify();
    }

    pub(super) fn apply_saved_layout(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.module_layout.apply_saved_layout(name) {
            self.status = format!("已切换到布局「{name}」").into();
            self.persist_ui_preferences(cx);
            cx.notify();
        }
    }

    pub(super) fn remove_saved_layout(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.module_layout.remove_saved_layout(name) {
            self.status = format!("已删除布局「{name}」").into();
            self.persist_ui_preferences(cx);
            cx.notify();
        }
    }

    pub(super) fn apply_layout_preset(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.module_layout.apply_preset(id) {
            self.status = "已应用推荐布局".into();
            self.persist_ui_preferences(cx);
            cx.notify();
        }
    }

    pub(super) fn reset_module_layout(&mut self, cx: &mut Context<Self>) {
        if self.module_layout.reset() {
            self.status = "已恢复默认布局".into();
            self.persist_ui_preferences(cx);
            cx.notify();
        }
    }

    pub(super) fn set_module_skin(
        &mut self,
        skin_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.module_layout.set_skin(skin_id) {
            self.apply_skin_theme(window, cx);
            self.status = format!("已切换皮肤「{}」", crate::layout::skin_by_id(skin_id).name).into();
            self.persist_ui_preferences(cx);
            cx.notify();
        }
    }

    pub(super) fn apply_skin_theme(&self, _window: &mut Window, cx: &mut Context<Self>) {
        crate::theme_skin::apply_skin_palette(
            &self.module_layout.preferences.skin,
            self.module_layout.scale,
            cx,
        );
    }

    pub(super) fn persist_ui_preferences(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let root = workspace.reader.root().to_path_buf();
        let revision = self.module_layout.revision;
        let core = to_core_preferences(&self.module_layout.preferences);
        if self.module_board.save_in_flight {
            self.module_board.save_pending = Some((root, revision, core));
            return;
        }
        self.start_ui_preferences_save(root, revision, core, cx);
    }

    fn start_ui_preferences_save(
        &mut self,
        root: PathBuf,
        revision: u64,
        preferences: CorePreferences,
        cx: &mut Context<Self>,
    ) {
        self.module_board.save_in_flight = true;
        let expected_path = root.to_string_lossy().into_owned();
        let save_expected = expected_path.clone();
        let save = cx.background_executor().spawn(async move {
            settings::save_ui_preferences(&root, &save_expected, preferences)
        });
        cx.spawn(async move |this, cx| {
            let result = save.await;
            let _ = this.update(cx, |this, cx| {
                this.module_board.save_in_flight = false;
                let still_same_workspace = this
                    .workspace
                    .as_ref()
                    .is_some_and(|ws| ws.reader.root() == PathBuf::from(&expected_path))
                    && this.module_layout.revision == revision;
                if let Err(error) = result {
                    if still_same_workspace {
                        let message = format!("保存界面布局失败：{error}");
                        this.module_layout.error = Some(message.clone());
                        this.status = message.into();
                    }
                }
                if let Some((next_root, next_rev, next_prefs)) =
                    this.module_board.save_pending.take()
                {
                    let matches_current = this
                        .workspace
                        .as_ref()
                        .is_some_and(|ws| ws.reader.root() == next_root)
                        && this.module_layout.revision == next_rev;
                    if matches_current {
                        this.start_ui_preferences_save(next_root, next_rev, next_prefs, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

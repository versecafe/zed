use gpui::{
    actions, div, img, list, px, AnyElement, AppContext, AsyncWindowContext, CursorStyle,
    DismissEvent, Element, EventEmitter, FocusHandle, FocusableView, InteractiveElement,
    IntoElement, ListAlignment, ListScrollEvent, ListState, Model, ParentElement, Render,
    StatefulInteractiveElement, Styled, Task, View, ViewContext, VisualContext, WeakView,
    WindowContext,
};

use anyhow::Result;
use db::kvp::KEY_VALUE_STORE;
use project::Fs;
use schemars::JsonSchema;
use serde_derive::{Deserialize, Serialize};
use settings::{Settings, SettingsSources, SettingsStore};
use std::{sync::Arc, time::Duration};
use time::{OffsetDateTime, UtcOffset};
use ui::{h_flex, prelude::*, v_flex, Avatar, Button, Icon, IconButton, IconName, Label, Tooltip};
use util::{ResultExt, TryFutureExt};
use workspace::AppState;
use workspace::{
    dock::{DockPosition, Panel, PanelEvent},
    Workspace,
};

const TOAST_DURATION: Duration = Duration::from_secs(5);
const DEV_CONTAINER_PANEL_KEY: &str = "DockerPanel";

pub struct DockerPanel {
    fs: Arc<dyn Fs>,
    width: Option<Pixels>,
    active: bool,
    pending_serialization: Task<Option<()>>,
    focus_handle: FocusHandle,
    workspace: WeakView<Workspace>,
    local_timezone: UtcOffset,
}

#[derive(Deserialize, Debug)]
pub struct DockerPanelSettings {
    pub button: bool,
    pub dock: DockPosition,
    pub default_width: Pixels,
}

#[derive(Serialize, Deserialize)]
struct SerializedDockerPanel {
    width: Option<Pixels>,
}

#[derive(Debug)]
pub enum Event {
    DockPositionChanged,
    Focus,
    Dismissed,
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema, Debug)]
pub struct PanelSettingsContent {
    /// Whether to show the panel button in the status bar.
    ///
    /// Default: true
    pub button: Option<bool>,
    /// Where to dock the panel.
    ///
    /// Default: left
    pub dock: Option<DockPosition>,
    /// Default width of the panel in pixels.
    ///
    /// Default: 240
    pub default_width: Option<f32>,
}

impl Settings for DockerPanelSettings {
    const KEY: Option<&'static str> = Some("docker_panel");

    type FileContent = PanelSettingsContent;

    fn load(
        sources: SettingsSources<Self::FileContent>,
        _: &mut gpui::AppContext,
    ) -> anyhow::Result<Self> {
        sources.json_merge()
    }
}

impl DockerPanel {
    pub fn new(workspace: &mut Workspace, cx: &mut ViewContext<Workspace>) -> View<Self> {
        let fs = workspace.app_state().fs.clone();
        let workspace_handle = workspace.weak_handle();

        cx.new_view(|cx: &mut ViewContext<Self>| {
            // grab containers update and cx.notify

            let _view = cx.view().downgrade();

            let this = Self {
                fs,
                width: None,
                active: false,
                focus_handle: cx.focus_handle(),
                pending_serialization: Task::ready(None),
                workspace: workspace_handle,
                local_timezone: cx.local_timezone(),
            };

            return this;
        })
    }

    pub fn load(
        workspace: WeakView<Workspace>,
        cx: AsyncWindowContext,
    ) -> Task<Result<View<Self>>> {
        cx.spawn(|mut cx| async move {
            let serialized_panel = if let Some(panel) = cx
                .background_executor()
                .spawn(async move { KEY_VALUE_STORE.read_kvp(DEV_CONTAINER_PANEL_KEY) })
                .await
                .log_err()
                .flatten()
            {
                Some(serde_json::from_str::<SerializedDockerPanel>(&panel)?)
            } else {
                None
            };

            workspace.update(&mut cx, |workspace, cx| {
                let panel = Self::new(workspace, cx);
                if let Some(serialized_panel) = serialized_panel {
                    panel.update(cx, |panel, cx| {
                        panel.width = serialized_panel.width.map(|w| w.round());
                        cx.notify();
                    })
                }
                panel
            })
        })
    }

    fn serialize(&mut self, cx: &mut ViewContext<Self>) {
        let width = self.width;
        self.pending_serialization = cx.background_executor().spawn(
            async move {
                KEY_VALUE_STORE
                    .write_kvp(
                        DEV_CONTAINER_PANEL_KEY.into(),
                        serde_json::to_string(&SerializedDockerPanel { width })?,
                    )
                    .await?;
                anyhow::Ok(())
            }
            .log_err(),
        );
    }
}

impl Render for DockerPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .justify_between()
                    .px_2()
                    .py_1()
                    // Match the height of the tab bar so they line up.
                    .h(rems(ui::Tab::CONTAINER_HEIGHT_IN_REMS))
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(Label::new("Docker")),
            )
            .child(
                v_flex().p_4().child(
                    div().flex().w_full().items_center().child(
                        Label::new("Cannot find running Docker instance.")
                            .color(Color::Muted)
                            .size(LabelSize::Small),
                    ),
                ),
            )
    }
}

impl Panel for DockerPanel {
    fn persistent_name() -> &'static str {
        "DockerPanel"
    }

    fn position(&self, cx: &WindowContext) -> DockPosition {
        DockerPanelSettings::get_global(cx).dock
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, position: DockPosition, cx: &mut ViewContext<Self>) {
        settings::update_settings_file::<DockerPanelSettings>(
            self.fs.clone(),
            cx,
            move |settings| settings.dock = Some(position),
        )
    }

    fn size(&self, cx: &WindowContext) -> Pixels {
        self.width
            .unwrap_or_else(|| DockerPanelSettings::get_global(cx).default_width)
    }

    fn set_size(&mut self, size: Option<Pixels>, cx: &mut ViewContext<Self>) {
        self.width = size;
        self.serialize(cx);
        cx.notify();
    }

    fn set_active(&mut self, active: bool, cx: &mut ViewContext<Self>) {
        self.active = active;

        if self.active {
            // TODO notif handling from containers
            cx.notify()
        }
    }

    fn icon(&self, cx: &WindowContext) -> Option<ui::IconName> {
        // let show_button = DockerPanelSettings::get_global(cx).button;
        // if !show_button {
        //     return None;
        // }

        // Get a docker Icon in
        //
        Some(IconName::Docker)
    }

    fn icon_tooltip(&self, _: &WindowContext) -> Option<&'static str> {
        Some("Dev Containers Panel")
    }

    fn icon_label(&self, _: &WindowContext) -> Option<String> {
        // TODO set count of running containers
        None
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(ToggleFocus)
    }
}

impl FocusableView for DockerPanel {
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<Event> for DockerPanel {}
impl EventEmitter<PanelEvent> for DockerPanel {}

actions!(docker_panel, [ToggleFocus]);

pub fn init(app_state: &Arc<AppState>, cx: &mut AppContext) {
    DockerPanelSettings::register(cx);

    // panel init
    cx.observe_new_views(|workspace: &mut Workspace, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, cx| {
            workspace.toggle_panel_focus::<DockerPanel>(cx);
        });
    })
    .detach();

    println!("Initing docker panel!");
}

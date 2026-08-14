//! Root of the operator shell.
//!
//! The center owns the active instrument. Navigation and context are separate
//! reliquaries, never hidden authorities. One root owns responsive docking,
//! drawer state, screen routing, and local presentation preferences.

use godot::classes::control::SizeFlags;
use godot::classes::{
    BoxContainer, Button, Control, GridContainer, IControl, Input, Label, MarginContainer,
    ScrollContainer,
};
use godot::prelude::*;

use crate::host_session::AthanorHostSession;
use crate::protocol::HostBinding;

const INITIAL_SCREEN_ENV: &str = "ATHANOR_INITIAL_SCREEN";
const WIDE_BREAKPOINT: f32 = 1_200.0;
const COMPACT_BREAKPOINT: f32 = 800.0;
const WIDE_LEFT_MARGIN: i32 = 252;
const WIDE_RIGHT_MARGIN: i32 = 316;
const COMPACT_LEFT_MARGIN: i32 = 232;

#[derive(Copy, Clone, Eq, PartialEq)]
enum Screen {
    Resume,
    RecallPolicy,
    Routing,
    Familiars,
    Dispatch,
    Health,
}

impl Screen {
    fn action_id(self) -> &'static str {
        match self {
            Self::Resume => "screen:resume",
            Self::RecallPolicy => "screen:recall-policy",
            Self::Routing => "screen:routing",
            Self::Familiars => "screen:familiars",
            Self::Dispatch => "screen:dispatch",
            Self::Health => "screen:health",
        }
    }

    fn route_label(self) -> &'static str {
        match self {
            Self::Resume => "S01 · CONVERSA / RETOMADA",
            Self::RecallPolicy => "S02 · MEMÓRIA / RECALL",
            Self::Routing => "HOUSE / WORKER LANES · HOST STATUS",
            Self::Familiars => "S07 · FAMILIARS / SPELLBOOK",
            Self::Dispatch => "S08 · FAMILIARS / DISPATCH",
            Self::Health => "S09 · SISTEMA / SAÚDE",
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum LayoutClass {
    Wide,
    Compact,
    Narrow,
}

impl LayoutClass {
    fn from_width(width: f32) -> Self {
        if width >= WIDE_BREAKPOINT {
            Self::Wide
        } else if width >= COMPACT_BREAKPOINT {
            Self::Compact
        } else {
            Self::Narrow
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum Drawer {
    Left,
    Right,
}

#[derive(GodotClass)]
#[class(base = Control)]
pub struct AthanorProbe {
    #[export]
    resume_page: NodePath,
    #[export]
    recall_policy_page: NodePath,
    #[export]
    routing_page: NodePath,
    #[export]
    familiars_page: NodePath,
    #[export]
    dispatch_page: NodePath,
    #[export]
    health_page: NodePath,
    #[export]
    left_navigator: NodePath,
    #[export]
    right_navigator: NodePath,
    #[export]
    center_frame: NodePath,
    #[export]
    center_scroll: NodePath,
    #[export]
    content_viewport: NodePath,
    #[export]
    drawer_scrim: NodePath,
    #[export]
    menu_toggle: NodePath,
    #[export]
    status_context_button: NodePath,
    #[export]
    status_settings_button: NodePath,
    #[export]
    status_identity: NodePath,
    #[export]
    status_host_state: NodePath,
    #[export]
    host_session: NodePath,
    #[export]
    right_route_label: NodePath,
    #[export]
    recall_columns: NodePath,
    #[export]
    recall_state_grid: NodePath,

    session: Option<Gd<AthanorHostSession>>,
    shell: Option<Shell>,
    layout_class: Option<LayoutClass>,
    layout_override: Option<LayoutClass>,
    active_screen: Screen,
    drawer: Option<Drawer>,
    #[cfg(debug_assertions)]
    test_scroll_frames: u32,
    base: Base<Control>,
}

struct Shell {
    resume_page: Gd<Control>,
    recall_policy_page: Gd<Control>,
    routing_page: Gd<Control>,
    familiars_page: Gd<Control>,
    dispatch_page: Gd<Control>,
    health_page: Gd<Control>,
    left_navigator: Gd<Control>,
    right_navigator: Gd<Control>,
    center_frame: Gd<MarginContainer>,
    center_scroll: Gd<ScrollContainer>,
    content_viewport: Gd<Control>,
    drawer_scrim: Gd<Button>,
    menu_toggle: Gd<Button>,
    status_context_button: Gd<Button>,
    status_settings_button: Gd<Button>,
    status_identity: Gd<Label>,
    status_host_state: Gd<Label>,
    right_route_label: Gd<Label>,
    recall_columns: Gd<BoxContainer>,
    recall_state_grid: Gd<GridContainer>,
}

#[godot_api]
impl IControl for AthanorProbe {
    fn init(base: Base<Control>) -> Self {
        Self {
            resume_page: NodePath::default(),
            recall_policy_page: NodePath::default(),
            routing_page: NodePath::default(),
            familiars_page: NodePath::default(),
            dispatch_page: NodePath::default(),
            health_page: NodePath::default(),
            left_navigator: NodePath::default(),
            right_navigator: NodePath::default(),
            center_frame: NodePath::default(),
            center_scroll: NodePath::default(),
            content_viewport: NodePath::default(),
            drawer_scrim: NodePath::default(),
            menu_toggle: NodePath::default(),
            status_context_button: NodePath::default(),
            status_settings_button: NodePath::default(),
            status_identity: NodePath::default(),
            status_host_state: NodePath::default(),
            host_session: NodePath::default(),
            right_route_label: NodePath::default(),
            recall_columns: NodePath::default(),
            recall_state_grid: NodePath::default(),
            session: None,
            shell: None,
            layout_class: None,
            layout_override: None,
            active_screen: Screen::Resume,
            drawer: None,
            #[cfg(debug_assertions)]
            test_scroll_frames: 0,
            base,
        }
    }

    fn ready(&mut self) {
        let Some(mut shell) = self.resolve_shell() else {
            godot_error!("AthanorProbe: reliquary shell bindings are incomplete");
            return;
        };

        let this = self.to_gd();
        for navigator in [&mut shell.left_navigator, &mut shell.right_navigator] {
            navigator.connect(
                "action_requested",
                &Callable::from_object_method(&this, "on_reliquary_action"),
            );
            navigator.connect(
                "close_requested",
                &Callable::from_object_method(&this, "on_drawer_close_requested"),
            );
        }
        for (button, method) in [
            (&mut shell.menu_toggle, "on_menu_toggle_pressed"),
            (
                &mut shell.status_context_button,
                "on_context_toggle_pressed",
            ),
            (&mut shell.status_settings_button, "on_settings_pressed"),
            (&mut shell.drawer_scrim, "on_drawer_close_requested"),
        ] {
            button.connect("pressed", &Callable::from_object_method(&this, method));
        }

        // The bottom rail mirrors the shared session's real link and binding
        // state; it never invents identity and clears on close.
        if let Some(mut session) = self
            .base()
            .try_get_node_as::<AthanorHostSession>(&self.host_session)
        {
            for (signal, method) in [
                ("opened", "on_session_opened"),
                ("closed", "on_session_closed"),
                ("unavailable", "on_session_unavailable"),
                ("message", "on_session_message"),
            ] {
                session.connect(signal, &Callable::from_object_method(&this, method));
            }
            self.session = Some(session);
        } else {
            godot_warn!("AthanorProbe: shared Host session not bound; status rail stays unbound");
        }

        // Every instrument shares one vertical scroll owner. Reparenting does
        // not alter any child projection binding or create another transport.
        let center_node = shell.center_scroll.clone().upcast::<godot::classes::Node>();
        shell.content_viewport.reparent(&center_node);
        shell
            .content_viewport
            .set_custom_minimum_size(Vector2::ZERO);
        shell
            .content_viewport
            .set_h_size_flags(SizeFlags::EXPAND_FILL);
        shell
            .content_viewport
            .set_v_size_flags(SizeFlags::EXPAND_FILL);

        self.base_mut().connect(
            "resized",
            &Callable::from_object_method(&this, "on_shell_resized"),
        );
        self.shell = Some(shell);
        self.active_screen = match std::env::var(INITIAL_SCREEN_ENV).ok().as_deref() {
            Some("recall-policy" | "s02") => Screen::RecallPolicy,
            Some("routing" | "worker-lanes") => Screen::Routing,
            Some("familiars" | "spellbook" | "s07") => Screen::Familiars,
            Some("dispatch" | "s08") => Screen::Dispatch,
            Some("health" | "saude" | "saúde" | "s09") => Screen::Health,
            Some("resume" | "conversation" | "chat" | "s01") | None => Screen::Resume,
            Some(other) => {
                godot_warn!("unknown {INITIAL_SCREEN_ENV} ({other}); using S01");
                Screen::Resume
            }
        };
        self.show_screen(self.active_screen);
        self.apply_layout(true);
        self.base_mut().set_process(true);

        #[cfg(debug_assertions)]
        if std::env::var("ATHANOR_TEST_SCROLL_TO_BOTTOM").as_deref() == Ok("1") {
            self.test_scroll_frames = 90;
        }
    }

    fn process(&mut self, _delta: f64) {
        if Input::singleton().is_action_just_pressed("ui_cancel") {
            self.handle_escape();
        }

        #[cfg(debug_assertions)]
        if self.test_scroll_frames > 0 {
            self.test_scroll_frames -= 1;
            if let Some(shell) = self.shell.as_mut() {
                shell.center_scroll.set_v_scroll(i32::MAX);
            }
        }
    }
}

#[godot_api]
impl AthanorProbe {
    #[func]
    fn on_reliquary_action(&mut self, action_id: StringName) {
        match action_id.to_string().as_str() {
            "screen:resume" => self.select_screen(Screen::Resume),
            "screen:recall-policy" => self.select_screen(Screen::RecallPolicy),
            "screen:routing" => self.select_screen(Screen::Routing),
            "screen:familiars" => self.select_screen(Screen::Familiars),
            "screen:dispatch" => self.select_screen(Screen::Dispatch),
            "screen:health" => self.select_screen(Screen::Health),
            "open:settings" => self.open_right_pane("Settings"),
            "open:context" => self.open_right_pane("Root"),
            "layout:auto" => self.set_layout_override(None, "layout:auto"),
            "layout:desktop" => self.set_layout_override(Some(LayoutClass::Wide), "layout:desktop"),
            "layout:narrow" => self.set_layout_override(Some(LayoutClass::Narrow), "layout:narrow"),
            other => godot_warn!("unknown reliquary action: {other}"),
        }
    }

    #[func]
    fn on_menu_toggle_pressed(&mut self) {
        if self.drawer == Some(Drawer::Left) {
            self.close_drawer();
        } else {
            if let Some(shell) = self.shell.as_mut() {
                shell.left_navigator.call("open_root", &[]);
            }
            self.drawer = Some(Drawer::Left);
            self.apply_layout(true);
        }
    }

    #[func]
    fn on_context_toggle_pressed(&mut self) {
        self.open_right_pane("Root");
    }

    #[func]
    fn on_settings_pressed(&mut self) {
        self.open_right_pane("Settings");
    }

    #[func]
    fn on_drawer_close_requested(&mut self) {
        self.close_drawer();
    }

    #[func]
    fn on_shell_resized(&mut self) {
        self.apply_layout(false);
    }

    #[func]
    fn on_session_opened(&mut self) {
        self.render_rail("HOST LINK OPEN", None);
    }

    #[func]
    fn on_session_closed(&mut self, _detail: GString) {
        self.render_rail("HOST UNBOUND", None);
    }

    #[func]
    fn on_session_unavailable(&mut self, _detail: GString) {
        self.render_rail("HOST UNBOUND", None);
    }

    #[func]
    fn on_session_message(&mut self, _envelope: VarDictionary) {
        let binding = self.session.as_ref().and_then(|s| s.bind().binding());
        match binding {
            Some(binding) => self.render_rail("HOST BOUND", Some(&binding)),
            None => self.render_rail("HOST LINK OPEN", None),
        }
    }
}

impl AthanorProbe {
    fn resolve_shell(&self) -> Option<Shell> {
        Some(Shell {
            resume_page: self.base().try_get_node_as(&self.resume_page)?,
            recall_policy_page: self.base().try_get_node_as(&self.recall_policy_page)?,
            routing_page: self.base().try_get_node_as(&self.routing_page)?,
            familiars_page: self.base().try_get_node_as(&self.familiars_page)?,
            dispatch_page: self.base().try_get_node_as(&self.dispatch_page)?,
            health_page: self.base().try_get_node_as(&self.health_page)?,
            left_navigator: self.base().try_get_node_as(&self.left_navigator)?,
            right_navigator: self.base().try_get_node_as(&self.right_navigator)?,
            center_frame: self.base().try_get_node_as(&self.center_frame)?,
            center_scroll: self.base().try_get_node_as(&self.center_scroll)?,
            content_viewport: self.base().try_get_node_as(&self.content_viewport)?,
            drawer_scrim: self.base().try_get_node_as(&self.drawer_scrim)?,
            menu_toggle: self.base().try_get_node_as(&self.menu_toggle)?,
            status_context_button: self.base().try_get_node_as(&self.status_context_button)?,
            status_settings_button: self.base().try_get_node_as(&self.status_settings_button)?,
            status_identity: self.base().try_get_node_as(&self.status_identity)?,
            status_host_state: self.base().try_get_node_as(&self.status_host_state)?,
            right_route_label: self.base().try_get_node_as(&self.right_route_label)?,
            recall_columns: self.base().try_get_node_as(&self.recall_columns)?,
            recall_state_grid: self.base().try_get_node_as(&self.recall_state_grid)?,
        })
    }

    fn render_rail(&mut self, state: &str, binding: Option<&HostBinding>) {
        let Some(shell) = self.shell.as_mut() else {
            return;
        };
        shell.status_host_state.set_text(state);
        match binding {
            Some(binding) => shell.status_identity.set_text(&format!(
                "HOUSE {}  ·  ROOM {}  ·  SPIRIT {}  ·  SESSION {}",
                binding.house_id, binding.room, binding.spirit, binding.session
            )),
            None => shell
                .status_identity
                .set_text("HOUSE —  ·  ROOM —  ·  SPIRIT —  ·  NO SNAPSHOT"),
        }
    }

    fn select_screen(&mut self, screen: Screen) {
        self.show_screen(screen);
        if self.layout_class == Some(LayoutClass::Narrow) {
            self.close_drawer();
        }
    }

    fn show_screen(&mut self, active: Screen) {
        let Some(shell) = self.shell.as_mut() else {
            return;
        };
        self.active_screen = active;
        for (screen, page) in [
            (Screen::Resume, &mut shell.resume_page),
            (Screen::RecallPolicy, &mut shell.recall_policy_page),
            (Screen::Routing, &mut shell.routing_page),
            (Screen::Familiars, &mut shell.familiars_page),
            (Screen::Dispatch, &mut shell.dispatch_page),
            (Screen::Health, &mut shell.health_page),
        ] {
            page.set_visible(screen == active);
        }
        shell.left_navigator.call(
            "set_active_action",
            &[StringName::from(active.action_id()).to_variant()],
        );
        shell.right_route_label.set_text(active.route_label());
        shell
            .center_scroll
            .set_deferred("scroll_vertical", &0_i64.to_variant());
    }

    fn open_right_pane(&mut self, pane: &str) {
        if let Some(shell) = self.shell.as_mut() {
            shell.right_navigator.call("open_root", &[]);
            if pane != "Root" {
                shell
                    .right_navigator
                    .call("open_pane", &[StringName::from(pane).to_variant()]);
            }
        }
        if self.layout_class != Some(LayoutClass::Wide) {
            self.drawer = Some(Drawer::Right);
        }
        self.apply_layout(true);
    }

    fn set_layout_override(&mut self, class: Option<LayoutClass>, action_id: &str) {
        self.layout_override = class;
        if let Some(shell) = self.shell.as_mut() {
            shell.right_navigator.call(
                "set_active_action",
                &[StringName::from(action_id).to_variant()],
            );
        }
        self.apply_layout(true);
    }

    fn close_drawer(&mut self) {
        self.drawer = None;
        self.apply_layout(true);
    }

    fn handle_escape(&mut self) {
        let active_drawer = self.drawer;
        let consumed = if let Some(shell) = self.shell.as_mut() {
            let navigator = match active_drawer {
                Some(Drawer::Left) => Some(&mut shell.left_navigator),
                Some(Drawer::Right) => Some(&mut shell.right_navigator),
                None if self.layout_class == Some(LayoutClass::Wide) => {
                    Some(&mut shell.right_navigator)
                }
                None => None,
            };
            navigator
                .map(|nav| nav.call("handle_escape", &[]).to::<bool>())
                .unwrap_or(false)
        } else {
            false
        };
        if !consumed && active_drawer.is_some() {
            self.close_drawer();
        }
    }

    fn apply_layout(&mut self, force: bool) {
        let measured = LayoutClass::from_width(self.base().get_viewport_rect().size.x);
        let class = self.layout_override.unwrap_or(measured);
        if !force && self.layout_class == Some(class) {
            return;
        }
        let Some(shell) = self.shell.as_mut() else {
            return;
        };

        let (left_margin, right_margin) = match class {
            LayoutClass::Wide => (WIDE_LEFT_MARGIN, WIDE_RIGHT_MARGIN),
            LayoutClass::Compact => (COMPACT_LEFT_MARGIN, 0),
            LayoutClass::Narrow => (0, 0),
        };
        shell
            .center_frame
            .add_theme_constant_override("margin_left", left_margin);
        shell
            .center_frame
            .add_theme_constant_override("margin_right", right_margin);

        let left_overlay = class == LayoutClass::Narrow && self.drawer == Some(Drawer::Left);
        let right_overlay = class != LayoutClass::Wide && self.drawer == Some(Drawer::Right);
        shell
            .left_navigator
            .set_visible(class != LayoutClass::Narrow || left_overlay);
        shell
            .right_navigator
            .set_visible(class == LayoutClass::Wide || right_overlay);
        shell
            .drawer_scrim
            .set_visible(left_overlay || right_overlay);
        shell
            .left_navigator
            .call("set_close_visible", &[left_overlay.to_variant()]);
        shell
            .right_navigator
            .call("set_close_visible", &[right_overlay.to_variant()]);

        shell.menu_toggle.set_visible(class == LayoutClass::Narrow);
        shell
            .status_identity
            .set_visible(class != LayoutClass::Narrow);
        shell
            .recall_columns
            .set_vertical(class != LayoutClass::Wide);
        shell.recall_state_grid.set_columns(match class {
            LayoutClass::Wide => 4,
            LayoutClass::Compact => 2,
            LayoutClass::Narrow => 1,
        });
        self.layout_class = Some(class);
    }
}

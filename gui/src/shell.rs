//! Root of the operator shell.
//!
//! The center belongs to the active conversation. Auxiliary screens replace
//! that center explicitly; left navigation and right inspection never become
//! hidden authorities. One root also owns responsive composition.

use godot::classes::control::SizeFlags;
use godot::classes::{
    BoxContainer, Button, Control, GridContainer, IControl, PanelContainer, ScrollContainer,
};
use godot::prelude::*;

const VARIATION_ACTIVE: &str = "AthanorTabActive";
const VARIATION_IDLE: &str = "AthanorTab";
const INITIAL_SCREEN_ENV: &str = "ATHANOR_INITIAL_SCREEN";
const WIDE_BREAKPOINT: f32 = 1_200.0;
const COMPACT_BREAKPOINT: f32 = 800.0;

#[derive(Copy, Clone, Eq, PartialEq)]
enum Screen {
    Resume,
    RecallPolicy,
    Routing,
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

#[derive(GodotClass)]
#[class(base = Control)]
pub struct AthanorProbe {
    #[export]
    resume_screen_button: NodePath,
    #[export]
    recall_policy_screen_button: NodePath,
    #[export]
    routing_screen_button: NodePath,
    #[export]
    resume_page: NodePath,
    #[export]
    recall_policy_page: NodePath,
    #[export]
    routing_page: NodePath,
    #[export]
    body: NodePath,
    #[export]
    left_sidebar: NodePath,
    #[export]
    right_sidebar: NodePath,
    #[export]
    center_scroll: NodePath,
    #[export]
    content_viewport: NodePath,
    #[export]
    prototype_disclosure: NodePath,
    #[export]
    system_states: NodePath,
    #[export]
    view_controls: NodePath,
    #[export]
    navigation: NodePath,
    #[export]
    group_strip: NodePath,
    #[export]
    screen_context: NodePath,
    #[export]
    recall_columns: NodePath,
    #[export]
    recall_state_grid: NodePath,
    #[export]
    status_dock: NodePath,
    #[export]
    status_grid: NodePath,

    shell: Option<Shell>,
    layout_class: Option<LayoutClass>,
    #[cfg(debug_assertions)]
    test_scroll_frames: u32,
    base: Base<Control>,
}

struct Shell {
    resume_button: Gd<Button>,
    recall_policy_button: Gd<Button>,
    routing_button: Gd<Button>,
    resume_page: Gd<Control>,
    recall_policy_page: Gd<Control>,
    routing_page: Gd<Control>,
    body: Gd<BoxContainer>,
    left_sidebar: Gd<PanelContainer>,
    right_sidebar: Gd<PanelContainer>,
    center_scroll: Gd<ScrollContainer>,
    content_viewport: Gd<Control>,
    prototype_disclosure: Gd<Control>,
    system_states: Gd<Control>,
    view_controls: Gd<Control>,
    navigation: Gd<Control>,
    group_strip: Gd<BoxContainer>,
    screen_context: Gd<Control>,
    recall_columns: Gd<BoxContainer>,
    recall_state_grid: Gd<GridContainer>,
    status_dock: Gd<Control>,
    status_grid: Gd<GridContainer>,
}

#[godot_api]
impl IControl for AthanorProbe {
    fn init(base: Base<Control>) -> Self {
        Self {
            resume_screen_button: NodePath::default(),
            recall_policy_screen_button: NodePath::default(),
            routing_screen_button: NodePath::default(),
            resume_page: NodePath::default(),
            recall_policy_page: NodePath::default(),
            routing_page: NodePath::default(),
            body: NodePath::default(),
            left_sidebar: NodePath::default(),
            right_sidebar: NodePath::default(),
            center_scroll: NodePath::default(),
            content_viewport: NodePath::default(),
            prototype_disclosure: NodePath::default(),
            system_states: NodePath::default(),
            view_controls: NodePath::default(),
            navigation: NodePath::default(),
            group_strip: NodePath::default(),
            screen_context: NodePath::default(),
            recall_columns: NodePath::default(),
            recall_state_grid: NodePath::default(),
            status_dock: NodePath::default(),
            status_grid: NodePath::default(),
            shell: None,
            layout_class: None,
            #[cfg(debug_assertions)]
            test_scroll_frames: 0,
            base,
        }
    }

    fn ready(&mut self) {
        let resolved = self.resolve_shell();
        let Some(mut shell) = resolved else {
            godot_error!("AthanorProbe: responsive shell bindings are incomplete");
            return;
        };

        let this = self.to_gd();
        for (button, method) in [
            (&mut shell.resume_button, "on_resume_screen_pressed"),
            (
                &mut shell.recall_policy_button,
                "on_recall_policy_screen_pressed",
            ),
            (&mut shell.routing_button, "on_routing_screen_pressed"),
        ] {
            button.connect("pressed", &Callable::from_object_method(&this, method));
        }

        // Children resolve their own scene-local paths before the root runs.
        // Reparenting here gives every screen one vertical scroll owner without
        // changing projection bindings or opening another transport.
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

        let resize_callable = Callable::from_object_method(&this, "on_shell_resized");
        self.base_mut().connect("resized", &resize_callable);
        self.shell = Some(shell);

        let screen = match std::env::var(INITIAL_SCREEN_ENV).ok().as_deref() {
            Some("recall-policy" | "s02") => Screen::RecallPolicy,
            Some("routing" | "familiars" | "s03") => Screen::Routing,
            Some("resume" | "conversation" | "chat" | "s01") | None => Screen::Resume,
            Some(other) => {
                godot_warn!("unknown {INITIAL_SCREEN_ENV} ({other}); using S01");
                Screen::Resume
            }
        };
        self.show_screen(screen);
        self.apply_layout(true);
        self.base_mut().set_process(false);
        #[cfg(debug_assertions)]
        if std::env::var("ATHANOR_TEST_SCROLL_TO_BOTTOM").as_deref() == Ok("1") {
            self.test_scroll_frames = 90;
            self.base_mut().set_process(true);
        }
    }

    fn process(&mut self, _delta: f64) {
        #[cfg(debug_assertions)]
        if self.test_scroll_frames > 0 {
            self.test_scroll_frames -= 1;
            if let Some(shell) = self.shell.as_mut() {
                shell.center_scroll.set_v_scroll(i32::MAX);
            }
            if self.test_scroll_frames == 0 {
                self.base_mut().set_process(false);
            }
        }
    }
}

#[godot_api]
impl AthanorProbe {
    #[func]
    fn on_resume_screen_pressed(&mut self) {
        self.show_screen(Screen::Resume);
    }

    #[func]
    fn on_recall_policy_screen_pressed(&mut self) {
        self.show_screen(Screen::RecallPolicy);
    }

    #[func]
    fn on_routing_screen_pressed(&mut self) {
        self.show_screen(Screen::Routing);
    }

    #[func]
    fn on_shell_resized(&mut self) {
        self.apply_layout(false);
    }
}

impl AthanorProbe {
    fn resolve_shell(&self) -> Option<Shell> {
        Some(Shell {
            resume_button: self.base().try_get_node_as(&self.resume_screen_button)?,
            recall_policy_button: self
                .base()
                .try_get_node_as(&self.recall_policy_screen_button)?,
            routing_button: self.base().try_get_node_as(&self.routing_screen_button)?,
            resume_page: self.base().try_get_node_as(&self.resume_page)?,
            recall_policy_page: self.base().try_get_node_as(&self.recall_policy_page)?,
            routing_page: self.base().try_get_node_as(&self.routing_page)?,
            body: self.base().try_get_node_as(&self.body)?,
            left_sidebar: self.base().try_get_node_as(&self.left_sidebar)?,
            right_sidebar: self.base().try_get_node_as(&self.right_sidebar)?,
            center_scroll: self.base().try_get_node_as(&self.center_scroll)?,
            content_viewport: self.base().try_get_node_as(&self.content_viewport)?,
            prototype_disclosure: self.base().try_get_node_as(&self.prototype_disclosure)?,
            system_states: self.base().try_get_node_as(&self.system_states)?,
            view_controls: self.base().try_get_node_as(&self.view_controls)?,
            navigation: self.base().try_get_node_as(&self.navigation)?,
            group_strip: self.base().try_get_node_as(&self.group_strip)?,
            screen_context: self.base().try_get_node_as(&self.screen_context)?,
            recall_columns: self.base().try_get_node_as(&self.recall_columns)?,
            recall_state_grid: self.base().try_get_node_as(&self.recall_state_grid)?,
            status_dock: self.base().try_get_node_as(&self.status_dock)?,
            status_grid: self.base().try_get_node_as(&self.status_grid)?,
        })
    }

    fn apply_layout(&mut self, force: bool) {
        let width = self.base().get_viewport_rect().size.x;
        let class = LayoutClass::from_width(width);
        if !force && self.layout_class == Some(class) {
            return;
        }
        let Some(shell) = self.shell.as_mut() else {
            return;
        };

        shell.body.set_vertical(false);
        shell.left_sidebar.set_visible(class != LayoutClass::Narrow);
        shell.right_sidebar.set_visible(class == LayoutClass::Wide);
        shell
            .prototype_disclosure
            .set_visible(class == LayoutClass::Wide);
        shell.system_states.set_visible(class == LayoutClass::Wide);
        shell.view_controls.set_visible(class == LayoutClass::Wide);
        shell.navigation.set_visible(true);
        shell.group_strip.set_visible(class != LayoutClass::Narrow);
        shell.screen_context.set_visible(class == LayoutClass::Wide);
        shell
            .recall_columns
            .set_vertical(class != LayoutClass::Wide);
        shell.recall_state_grid.set_columns(match class {
            LayoutClass::Wide => 4,
            LayoutClass::Compact => 2,
            LayoutClass::Narrow => 1,
        });
        shell.status_dock.set_visible(class != LayoutClass::Narrow);
        shell
            .status_grid
            .set_columns(if class == LayoutClass::Wide { 5 } else { 2 });

        shell.left_sidebar.set_custom_minimum_size(Vector2::new(
            if class == LayoutClass::Wide {
                220.0
            } else {
                190.0
            },
            0.0,
        ));
        shell
            .right_sidebar
            .set_custom_minimum_size(Vector2::new(280.0, 0.0));
        self.layout_class = Some(class);
    }

    fn show_screen(&mut self, active: Screen) {
        let Some(shell) = self.shell.as_mut() else {
            return;
        };
        for (screen, button, page) in [
            (
                Screen::Resume,
                &mut shell.resume_button,
                &mut shell.resume_page,
            ),
            (
                Screen::RecallPolicy,
                &mut shell.recall_policy_button,
                &mut shell.recall_policy_page,
            ),
            (
                Screen::Routing,
                &mut shell.routing_button,
                &mut shell.routing_page,
            ),
        ] {
            let selected = screen == active;
            page.set_visible(selected);
            button.set_theme_type_variation(if selected {
                VARIATION_ACTIVE
            } else {
                VARIATION_IDLE
            });
        }
        shell
            .center_scroll
            .set_deferred("scroll_vertical", &0_i64.to_variant());
    }
}

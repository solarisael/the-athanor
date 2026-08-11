//! Root of the operator shell.
//!
//! One root owns the document. The shell keeps the declarative S01 gallery and
//! the S02 Recall Policy instrument as two screens of one content viewport;
//! screen selection is local presentation state and never Host state.

use godot::classes::{Button, Control, IControl};
use godot::prelude::*;

const VARIATION_ACTIVE: &str = "AthanorTabActive";
const VARIATION_IDLE: &str = "AthanorTab";
const INITIAL_SCREEN_ENV: &str = "ATHANOR_INITIAL_SCREEN";

#[derive(GodotClass)]
#[class(base = Control)]
pub struct AthanorProbe {
    #[export]
    resume_screen_button: NodePath,
    #[export]
    recall_policy_screen_button: NodePath,
    #[export]
    resume_page: NodePath,
    #[export]
    recall_policy_page: NodePath,

    screens: Option<Screens>,
    base: Base<Control>,
}

struct Screens {
    resume_button: Gd<Button>,
    recall_policy_button: Gd<Button>,
    resume_page: Gd<Control>,
    recall_policy_page: Gd<Control>,
}

#[godot_api]
impl IControl for AthanorProbe {
    fn init(base: Base<Control>) -> Self {
        Self {
            resume_screen_button: NodePath::default(),
            recall_policy_screen_button: NodePath::default(),
            resume_page: NodePath::default(),
            recall_policy_page: NodePath::default(),
            screens: None,
            base,
        }
    }

    fn ready(&mut self) {
        let resume_button = self
            .base()
            .try_get_node_as::<Button>(&self.resume_screen_button);
        let recall_policy_button = self
            .base()
            .try_get_node_as::<Button>(&self.recall_policy_screen_button);
        let resume_page = self.base().try_get_node_as::<Control>(&self.resume_page);
        let recall_policy_page = self
            .base()
            .try_get_node_as::<Control>(&self.recall_policy_page);

        let (
            Some(resume_button),
            Some(recall_policy_button),
            Some(resume_page),
            Some(recall_policy_page),
        ) = (
            resume_button,
            recall_policy_button,
            resume_page,
            recall_policy_page,
        )
        else {
            godot_error!(
                "AthanorProbe: vínculos de tela ausentes; verifique resume_screen_button, recall_policy_screen_button, resume_page e recall_policy_page"
            );
            return;
        };

        let mut screens = Screens {
            resume_button,
            recall_policy_button,
            resume_page,
            recall_policy_page,
        };

        let this = self.to_gd();
        screens.resume_button.connect(
            "pressed",
            &Callable::from_object_method(&this, "on_resume_screen_pressed"),
        );
        screens.recall_policy_button.connect(
            "pressed",
            &Callable::from_object_method(&this, "on_recall_policy_screen_pressed"),
        );

        self.screens = Some(screens);
        let resume = match std::env::var(INITIAL_SCREEN_ENV).ok().as_deref() {
            Some("recall-policy" | "s02") => false,
            Some("resume" | "s01") | None => true,
            Some(other) => {
                godot_warn!("{INITIAL_SCREEN_ENV} desconhecida ({other}); usando a tela S01");
                true
            }
        };
        self.show_screen(resume);
    }
}

#[godot_api]
impl AthanorProbe {
    #[func]
    fn on_resume_screen_pressed(&mut self) {
        self.show_screen(true);
    }

    #[func]
    fn on_recall_policy_screen_pressed(&mut self) {
        self.show_screen(false);
    }
}

impl AthanorProbe {
    fn show_screen(&mut self, resume: bool) {
        let Some(screens) = self.screens.as_mut() else {
            return;
        };
        screens.resume_page.set_visible(resume);
        screens.recall_policy_page.set_visible(!resume);
        screens.resume_button.set_theme_type_variation(if resume {
            VARIATION_ACTIVE
        } else {
            VARIATION_IDLE
        });
        screens
            .recall_policy_button
            .set_theme_type_variation(if resume {
                VARIATION_IDLE
            } else {
                VARIATION_ACTIVE
            });
    }
}

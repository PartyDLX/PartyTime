//! The splash screen: what the console is doing while it starts.
//!
//! Every line on this screen is a real step. There is no scripted delay and no
//! decorative animation: the screen appears because work is genuinely in flight, and it
//! leaves when the work is done or has failed. The fade the shell applies on a route
//! change is the only motion here, and it explains a real transition.

use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, spinner::Spinner};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, Styled as _, TestSupportExt as _, Window, div, px,
};

/// Where one bootstrap step stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepState {
    /// Not started.
    Pending,
    /// Running now.
    Running,
    /// Finished.
    Done,
    /// Failed, with the reason.
    Failed(SharedString),
}

/// One real step in bringing the console up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootStep {
    /// What the step is doing, phrased as the action.
    pub label: SharedString,
    /// Where the step stands.
    pub state: StepState,
}

impl BootStep {
    /// A step that has not started.
    pub fn pending(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            state: StepState::Pending,
        }
    }

    /// A step running now.
    pub fn running(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            state: StepState::Running,
        }
    }

    /// A step that finished.
    pub fn done(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            state: StepState::Done,
        }
    }

    /// A step that failed.
    pub fn failed(label: impl Into<SharedString>, reason: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            state: StepState::Failed(reason.into()),
        }
    }

    /// Whether this step failed.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        matches!(self.state, StepState::Failed(_))
    }

    /// Whether this step is still outstanding.
    #[must_use]
    pub fn is_outstanding(&self) -> bool {
        matches!(self.state, StepState::Pending | StepState::Running)
    }
}

/// Whether every step has settled, one way or the other.
///
/// An empty list is *not* complete: the shell must not flash past the splash before
/// the first step has even been queued.
#[must_use]
pub fn bootstrap_complete(steps: &[BootStep]) -> bool {
    !steps.is_empty() && !steps.iter().any(BootStep::is_outstanding)
}

/// The failure on the first step that has one, if any.
#[must_use]
pub fn first_failure(steps: &[BootStep]) -> Option<SharedString> {
    steps.iter().find_map(|step| match &step.state {
        StepState::Failed(reason) => Some(reason.clone()),
        _ => None,
    })
}

/// How far the splash has got, as a fraction in `0.0..=1.0`.
///
/// Only completed steps advance the bar. A failed step does not, because the console
/// has not moved forward.
#[must_use]
pub fn bootstrap_progress(steps: &[BootStep]) -> f32 {
    if steps.is_empty() {
        return 0.0;
    }
    let settled = steps
        .iter()
        .filter(|step| matches!(step.state, StepState::Done))
        .count();
    settled as f32 / steps.len() as f32
}

/// The splash screen.
///
/// Stateless: the shell owns the steps and hands them in, so a redraw of the splash
/// can never disagree with the bootstrap driving it.
#[derive(IntoElement)]
pub struct Splash {
    /// The steps, in the order they run.
    pub steps: Vec<BootStep>,
}

impl RenderOnce for Splash {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_8()
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Video).large())
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child("PartyTime"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Creator publishing console for OpenParty"),
                    ),
            )
            .child(self::step_list(self.steps, cx))
    }
}

fn step_list(steps: Vec<BootStep>, cx: &mut App) -> impl IntoElement {
    v_flex()
        .w(px(320.))
        .gap_3()
        .children(steps.into_iter().enumerate().map(|(index, step)| {
            let (icon, color) = match &step.state {
                StepState::Pending => (IconName::Circle, cx.theme().muted_foreground),
                StepState::Running => (IconName::LoaderCircle, cx.theme().primary),
                StepState::Done => (IconName::CircleCheck, cx.theme().success),
                StepState::Failed(_) => (IconName::CircleAlert, cx.theme().danger),
            };
            v_flex()
                .id(format!("boot-step-{index}"))
                .test_support()
                .gap_1()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Icon::new(icon).small().text_color(color))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(step.label.clone()),
                        )
                        .when(matches!(step.state, StepState::Running), |this| {
                            this.child(Spinner::new().xsmall())
                        }),
                )
                // The refusal, verbatim, under the step that hit it — not in a separate
                // banner the user has to connect back to a line.
                .when(matches!(step.state, StepState::Failed(_)), |this| {
                    let reason = match &step.state {
                        StepState::Failed(reason) => reason.clone(),
                        _ => SharedString::default(),
                    };
                    this.child(
                        div()
                            .pl_6()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(reason),
                    )
                })
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps() -> Vec<BootStep> {
        vec![
            BootStep::running("Read configuration"),
            BootStep::pending("Load profile"),
        ]
    }

    #[test]
    fn a_step_with_work_outstanding_is_not_complete() {
        assert!(!bootstrap_complete(&steps()));
    }

    #[test]
    fn no_steps_at_all_is_not_complete_so_the_splash_never_flashes_past() {
        assert!(!bootstrap_complete(&[]));
    }

    #[test]
    fn a_finished_run_is_complete() {
        let done = vec![
            BootStep::done("Read configuration"),
            BootStep::done("Load profile"),
        ];
        assert!(bootstrap_complete(&done));
    }

    #[test]
    fn a_failed_run_is_complete_so_the_console_is_not_stuck_on_the_splash() {
        let failed = vec![BootStep::failed("Load profile", "no profile")];
        assert!(bootstrap_complete(&failed));
        assert_eq!(first_failure(&failed).as_deref(), Some("no profile"));
    }

    #[test]
    fn progress_counts_only_completed_steps() {
        assert_eq!(bootstrap_progress(&[]), 0.0);
        assert_eq!(bootstrap_progress(&steps()), 0.0);

        let half = vec![
            BootStep::done("Read configuration"),
            BootStep::running("Load profile"),
        ];
        assert_eq!(bootstrap_progress(&half), 0.5);

        let failed = vec![
            BootStep::done("Read configuration"),
            BootStep::failed("Load profile", "x"),
        ];
        assert_eq!(
            bootstrap_progress(&failed),
            0.5,
            "a failure does not advance the bar"
        );
    }

    #[test]
    fn the_first_failure_is_reported_not_the_last() {
        let steps = vec![
            BootStep::failed("Read configuration", "first"),
            BootStep::failed("Load profile", "second"),
        ];
        assert_eq!(first_failure(&steps).as_deref(), Some("first"));
        assert_eq!(first_failure(&[BootStep::done("ok")]), None);
    }
}

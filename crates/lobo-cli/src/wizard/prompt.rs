use inquire::{
    Password, PasswordDisplayMode, Select, Text, error::InquireError, validator::Validation,
};
use std::fmt;

#[derive(Debug)]
pub enum PromptError {
    Aborted,
    Failed(anyhow::Error),
}
impl From<InquireError> for PromptError {
    fn from(e: InquireError) -> Self {
        match e {
            InquireError::OperationCanceled | InquireError::OperationInterrupted => Self::Aborted,
            other => Self::Failed(other.into()),
        }
    }
}
pub type PromptResult<T> = Result<T, PromptError>;
pub trait Prompter {
    fn note(&mut self, title: &str, body: &str) -> PromptResult<()>;
    fn secret(
        &mut self,
        title: &str,
        desc: &str,
        check: &dyn Fn(&str) -> Result<(), String>,
    ) -> PromptResult<String>;
    fn text(
        &mut self,
        title: &str,
        desc: &str,
        initial: &str,
        check: &dyn Fn(&str) -> Result<(), String>,
    ) -> PromptResult<String>;
    fn select(
        &mut self,
        title: &str,
        desc: &str,
        options: &[(&str, &str)],
        current: &str,
    ) -> PromptResult<String>;
    fn confirm(
        &mut self,
        title: &str,
        desc: &str,
        yes: &str,
        no: &str,
        default: bool,
    ) -> PromptResult<bool>;
}
pub struct InquirePrompter;
#[derive(Clone)]
struct Choice<'a>(&'a str, &'a str);
impl fmt::Display for Choice<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
fn render_config() -> inquire::ui::RenderConfig<'static> {
    let mut cfg = inquire::ui::RenderConfig::default();
    cfg.prompt = cfg.prompt.with_fg(inquire::ui::Color::Rgb {
        r: 125,
        g: 86,
        b: 244,
    });
    cfg
}
impl Prompter for InquirePrompter {
    fn note(&mut self, title: &str, body: &str) -> PromptResult<()> {
        use std::io::Write;
        writeln!(std::io::stdout(), "{title}\n{body}\n").map_err(|e| PromptError::Failed(e.into()))
    }
    fn secret(
        &mut self,
        title: &str,
        desc: &str,
        check: &dyn Fn(&str) -> Result<(), String>,
    ) -> PromptResult<String> {
        // Inquire's password validator is 'static. Validate the borrowed form state
        // here, and never put the typed secret in the diagnostic or next prompt.
        use std::io::Write;
        loop {
            let value = Password::new(title)
                .with_help_message(desc)
                .without_confirmation()
                .with_display_mode(PasswordDisplayMode::Masked)
                .with_render_config(render_config())
                .prompt()
                .map_err(PromptError::from)?;
            match check(&value) {
                Ok(()) => return Ok(value),
                Err(e) => {
                    writeln!(std::io::stderr(), "{e}").map_err(|e| PromptError::Failed(e.into()))?
                }
            }
        }
    }
    fn text(
        &mut self,
        title: &str,
        desc: &str,
        initial: &str,
        check: &dyn Fn(&str) -> Result<(), String>,
    ) -> PromptResult<String> {
        Text::new(title)
            .with_help_message(desc)
            .with_initial_value(initial)
            .with_render_config(render_config())
            .with_validator(move |v: &str| {
                Ok(match check(v) {
                    Ok(()) => Validation::Valid,
                    Err(e) => Validation::Invalid(e.into()),
                })
            })
            .prompt()
            .map_err(Into::into)
    }
    fn select(
        &mut self,
        title: &str,
        desc: &str,
        options: &[(&str, &str)],
        current: &str,
    ) -> PromptResult<String> {
        let start = options.iter().position(|(_, v)| *v == current).unwrap_or(0);
        Select::new(
            title,
            options
                .iter()
                .map(|(label, value)| Choice(label, value))
                .collect(),
        )
        .with_help_message(desc)
        .with_starting_cursor(start)
        .with_render_config(render_config())
        .prompt()
        .map(|o| o.1.into())
        .map_err(Into::into)
    }
    fn confirm(
        &mut self,
        title: &str,
        desc: &str,
        yes: &str,
        no: &str,
        default: bool,
    ) -> PromptResult<bool> {
        self.select(
            title,
            desc,
            &[(yes, "yes"), (no, "no")],
            if default { "yes" } else { "no" },
        )
        .map(|s| s == "yes")
    }
}

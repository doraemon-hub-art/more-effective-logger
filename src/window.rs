use crate::AppWindow;
use slint::ComponentHandle;

#[derive(Debug, Clone, Copy)]
pub enum WindowKind {
    Main,
    Log,
    Session,
    Terminal,
}

pub struct UiWindow {
    ui: AppWindow,
    kind: WindowKind,
}

impl UiWindow {
    pub fn new(kind: WindowKind) -> Result<Self, slint::PlatformError> {
        let ui = AppWindow::new()?;
        Ok(Self { ui, kind })
    }

    pub fn kind(&self) -> WindowKind {
        self.kind
    }

    pub fn as_inner(&self) -> &AppWindow {
        &self.ui
    }

    pub fn run(self) -> Result<(), slint::PlatformError> {
        self.ui.run()
    }
}

pub struct WindowFactory;

impl WindowFactory {
    pub fn create_main_window() -> Result<UiWindow, slint::PlatformError> {
        UiWindow::new(WindowKind::Main)
    }

    pub fn create_log_window() -> Result<UiWindow, slint::PlatformError> {
        UiWindow::new(WindowKind::Log)
    }

    pub fn create_session_window() -> Result<UiWindow, slint::PlatformError> {
        UiWindow::new(WindowKind::Session)
    }

    pub fn create_terminal_window() -> Result<UiWindow, slint::PlatformError> {
        UiWindow::new(WindowKind::Terminal)
    }
}

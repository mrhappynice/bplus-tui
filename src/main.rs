// ================================================
// FILE: src/main.rs
// ================================================
mod api;
mod app;
mod ui;

use std::{io, time::Duration};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use app::{App, AppAction, CurrentScreen, InputMode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let _ = app.action_tx.send(AppAction::LoadApps);
    let _ = app.action_tx.send(AppAction::LoadSearchState);

    let mut interval = tokio::time::interval(Duration::from_millis(250));

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        tokio::select! {
            _ = interval.tick() => { app.update(AppAction::Tick).await; }
            event = tokio::task::spawn_blocking(|| event::poll(Duration::from_millis(10))) => {
                if let Ok(Ok(true)) = event {
                    if let Event::Key(key) = event::read()? {
                        
                        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                            app.update(AppAction::Quit).await;
                        }

                        match app.input_mode {
                            InputMode::Normal => {
                                match key.code {
                                    KeyCode::Tab => app.update(AppAction::SwitchTab).await,
                                    KeyCode::Char('q') => app.update(AppAction::Quit).await,
                                    
                                    _ => match app.current_screen {
                                        CurrentScreen::Launcher => {
                                            match key.code {
                                                KeyCode::Down | KeyCode::Char('j') => app.update(AppAction::SelectNext).await,
                                                KeyCode::Up | KeyCode::Char('k') => app.update(AppAction::SelectPrev).await,
                                                KeyCode::Enter => app.update(AppAction::LaunchSelected).await,
                                                KeyCode::Char('/') => app.update(AppAction::ToggleFilter).await,
                                                KeyCode::Char('a') => app.update(AppAction::OpenAddModal).await,
                                                KeyCode::Char('e') => app.update(AppAction::OpenEditModal).await,
                                                KeyCode::Char('d') => app.update(AppAction::ConfirmDelete).await,
                                                KeyCode::Char(':') => app.update(AppAction::OpenAdHocModal).await,
                                                _ => {}
                                            }
                                        },
                                        CurrentScreen::Search => {
                                            // Fallback
                                        }
                                    }
                                }
                            },
                            
                            // --- SEARCH MODES ---
                            InputMode::SearchInput => {
                                match key.code {
                                    // Esc exits Search Tab back to Launcher
                                    KeyCode::Esc => app.update(AppAction::SwitchTab).await,
                                    // Tab cycles focus within Search (Input -> Sidebar -> History)
                                    KeyCode::Tab => app.update(AppAction::CycleSearchFocus).await,
                                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => app.update(AppAction::ToggleSearchSidebar).await,
                                    
                                    KeyCode::Enter => app.update(AppAction::SubmitSearch).await,
                                    KeyCode::Backspace => app.update(AppAction::DeleteSearchChar).await,
                                    KeyCode::Char(c) => app.update(AppAction::EnterSearchChar(c)).await,
                                    _ => {}
                                }
                            },
                            InputMode::SearchSidebar => {
                                match key.code {
                                    KeyCode::Esc => app.update(AppAction::SwitchTab).await,
                                    KeyCode::Tab => app.update(AppAction::CycleSearchFocus).await,
                                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => app.update(AppAction::ToggleSearchSidebar).await,
                                    
                                    KeyCode::Down | KeyCode::Char('j') => app.update(AppAction::SidebarNext).await,
                                    KeyCode::Up | KeyCode::Char('k') => app.update(AppAction::SidebarPrev).await,
                                    KeyCode::Enter | KeyCode::Char(' ') => app.update(AppAction::SidebarSelect).await,
                                    _ => {}
                                }
                            },
                            InputMode::ChatHistory => {
                                match key.code {
                                    KeyCode::Esc => app.update(AppAction::SwitchTab).await,
                                    KeyCode::Tab => app.update(AppAction::CycleSearchFocus).await,
                                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => app.update(AppAction::ToggleSearchSidebar).await,
                                    
                                    KeyCode::Up | KeyCode::Char('k') => app.update(AppAction::ScrollChat(-1)).await,
                                    KeyCode::Down | KeyCode::Char('j') => app.update(AppAction::ScrollChat(1)).await,
                                    KeyCode::PageUp => app.update(AppAction::ScrollChat(-10)).await,
                                    KeyCode::PageDown => app.update(AppAction::ScrollChat(10)).await,
                                    _ => {}
                                }
                            },

                            // --- MODALS ---
                            InputMode::Filtering => {
                                match key.code {
                                    KeyCode::Enter | KeyCode::Esc => app.update(AppAction::ToggleFilter).await,
                                    KeyCode::Backspace => app.update(AppAction::BackspaceFilter).await,
                                    KeyCode::Char(c) => app.update(AppAction::EnterFilterChar(c)).await,
                                    _ => {}
                                }
                            },
                            InputMode::Editing => {
                                match key.code {
                                    KeyCode::Esc => app.update(AppAction::CloseModal).await,
                                    KeyCode::Tab => app.update(AppAction::CycleFormFocus).await,
                                    KeyCode::Enter => app.update(AppAction::SubmitForm).await,
                                    KeyCode::Backspace => app.update(AppAction::FormBackspace).await,
                                    KeyCode::Char(c) => app.update(AppAction::FormChar(c)).await,
                                    _ => {}
                                }
                            },
                            InputMode::AdHocCmd => {
                                match key.code {
                                    KeyCode::Esc => app.update(AppAction::CloseModal).await,
                                    KeyCode::Enter => { let c = app.adhoc_input.clone(); app.update(AppAction::SubmitAdHoc(c)).await; },
                                    KeyCode::Backspace => { app.adhoc_input.pop(); },
                                    KeyCode::Char(c) => { app.adhoc_input.push(c); },
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            Some(action) = app.action_rx.recv() => {
                app.update(action).await;
            }
        }
        if app.should_quit { break; }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
// ================================================
// FILE: src/ui.rs
// ================================================
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap, Tabs, Clear},
    Frame,
};
use crate::app::{App, CurrentScreen, InputMode, SearchSidebarState};
use pulldown_cmark::{Parser, Event, Tag};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(f.size());

    render_tabs(f, app, chunks[0]);
    
    match app.current_screen {
        CurrentScreen::Launcher => render_launcher(f, app, chunks[1]),
        CurrentScreen::Search => render_search(f, app, chunks[1]),
    }

    render_footer(f, app, chunks[2]);

    if app.input_mode == InputMode::Editing { render_edit_modal(f, app); }
    if app.input_mode == InputMode::AdHocCmd { render_adhoc_modal(f, app); }
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec![" [L]auncher ", " [S]earch "];
    let idx = match app.current_screen { CurrentScreen::Launcher => 0, CurrentScreen::Search => 1 };
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" bplus-tui "))
        .select(idx)
        .highlight_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);
}

fn render_launcher(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area);
    let left_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(chunks[0]);
    
    let filter_style = if app.input_mode == InputMode::Filtering { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) };
    let filter_text = if app.filter_input.is_empty() { if app.input_mode == InputMode::Filtering { "" } else { "Press '/' to filter" } } else { &app.filter_input };
    f.render_widget(Paragraph::new(filter_text).style(filter_style).block(Block::default().borders(Borders::ALL).title(" Filter ")), left_chunks[0]);

    let items: Vec<ListItem> = app.filtered_apps.iter().map(|&idx| {
        let item = &app.apps[idx];
        let tags = item.description.as_deref().unwrap_or("").split_whitespace().filter(|s| s.starts_with('#')).collect::<Vec<_>>().join(" ");
        ListItem::new(vec![Line::from(Span::styled(&item.name, Style::default().add_modifier(Modifier::BOLD))), Line::from(Span::styled(tags, Style::default().fg(Color::DarkGray)))])
    }).collect();
    let mut state = ListState::default(); state.select(Some(app.apps_idx));
    f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).title(" Apps ")).highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White)), left_chunks[1], &mut state);

    let right_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(8), Constraint::Min(0)]).split(chunks[1]);
    let details = if let Some(a) = app.get_selected_app() {
        vec![Line::from(format!("Name: {}", a.name)), Line::from(format!("Cmd : {}", a.command)), Line::from(format!("URL : {}", a.url)), Line::from(format!("Desc: {}", a.description.as_deref().unwrap_or("")))]
    } else { vec![Line::from("No app selected")] };
    f.render_widget(Paragraph::new(details).block(Block::default().borders(Borders::ALL).title(" Details ")), right_chunks[0]);
    
    let log_start = if app.launcher_logs.len() > 15 { app.launcher_logs.len() - 15 } else { 0 };
    let logs: Vec<ListItem> = app.launcher_logs[log_start..].iter().map(|l| ListItem::new(Line::from(l.as_str()))).collect();
    f.render_widget(List::new(logs).block(Block::default().borders(Borders::ALL).title(" Output ")), right_chunks[1]);
}

fn markdown_to_text<'a>(markdown: &str) -> Vec<Line<'a>> {
    let parser = Parser::new(markdown);
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut style_stack = Vec::new();

    for event in parser {
        match event {
            Event::Text(text) => {
                let mut style = Style::default();
                for s in &style_stack { style = style.patch(*s); }
                current_line.push(Span::styled(text.to_string(), style));
            },
            Event::SoftBreak | Event::HardBreak => {
                lines.push(Line::from(current_line.clone()));
                current_line.clear();
            },
            Event::Start(tag) => match tag {
                Tag::Paragraph => {},
                Tag::Heading(_, _, _) => style_stack.push(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Tag::BlockQuote => style_stack.push(Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)),
                Tag::CodeBlock(_) => {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                    style_stack.push(Style::default().bg(Color::Rgb(40,40,40)).fg(Color::Cyan));
                },
                Tag::List(_) => {},
                Tag::Item => { current_line.push(Span::raw(" • ")); },
                Tag::Emphasis => style_stack.push(Style::default().add_modifier(Modifier::ITALIC)),
                Tag::Strong => style_stack.push(Style::default().add_modifier(Modifier::BOLD)),
                _ => {}
            },
            Event::End(tag) => match tag {
                Tag::Paragraph | Tag::Heading(_,_,_) | Tag::BlockQuote | Tag::List(_) | Tag::Item => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    if matches!(tag, Tag::Heading(_,_,_) | Tag::BlockQuote) { style_stack.pop(); }
                },
                Tag::CodeBlock(_) | Tag::Emphasis | Tag::Strong => { style_stack.pop(); },
                _ => {}
            },
            Event::Code(text) => {
                let style = Style::default().bg(Color::DarkGray).fg(Color::White);
                current_line.push(Span::styled(text.to_string(), style));
            },
            _ => {}
        }
    }
    if !current_line.is_empty() { lines.push(Line::from(current_line)); }
    lines
}

fn render_search(f: &mut Frame, app: &mut App, area: Rect) {
    let main_layout = Layout::default().direction(Direction::Horizontal)
        .constraints(if app.search_sidebar != SearchSidebarState::Hidden {
            [Constraint::Percentage(25), Constraint::Percentage(75)]
        } else {
            [Constraint::Percentage(0), Constraint::Percentage(100)]
        }).split(area);

    let sidebar_area = main_layout[0];
    let chat_area = main_layout[1];

    if app.search_sidebar != SearchSidebarState::Hidden {
        let block_style = if app.input_mode == InputMode::SearchSidebar { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) };
        let block = Block::default().borders(Borders::ALL).border_style(block_style);
        
        match app.search_sidebar {
            SearchSidebarState::History => {
                let mut items = vec![ListItem::new(Span::styled("[+] New Chat", Style::default().fg(Color::Green)))];
                items.extend(app.conversations.iter().map(|c| ListItem::new(c.title.clone())));
                
                let mut state = ListState::default(); 
                state.select(Some(app.conversation_idx));
                f.render_stateful_widget(List::new(items).block(block.title(" History ")).highlight_style(Style::default().bg(Color::Blue)), sidebar_area, &mut state);
            },
            SearchSidebarState::Settings => {
                let mut items = Vec::new();
                items.push(ListItem::new(format!("Provider: < {} >", app.selected_llm_provider)));
                items.push(ListItem::new(format!("Model:    < {} >", app.selected_model)));
                items.push(ListItem::new("--- Search Sources ---"));
                for p in &app.search_providers {
                    let check = if p.is_enabled { "[x]" } else { "[ ]" };
                    items.push(ListItem::new(format!("{} {}", check, p.name)));
                }
                let mut state = ListState::default(); state.select(Some(app.settings_idx));
                f.render_stateful_widget(List::new(items).block(block.title(" Settings ")).highlight_style(Style::default().bg(Color::Blue)), sidebar_area, &mut state);
            },
            _ => {}
        }
    }

    let chat_chunks = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)]).split(chat_area);

    let mut messages_visual = Vec::new();
    for msg in &app.messages {
        let role_style = match msg.role.as_str() {
            "user" => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            "assistant" => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::Red),
        };
        messages_visual.push(Line::from(Span::styled(format!("{}:", msg.role.to_uppercase()), role_style)));
        messages_visual.extend(markdown_to_text(&msg.content));
        if !msg.sources.is_empty() {
            messages_visual.push(Line::from(""));
            messages_visual.push(Line::from(Span::styled("Sources:", Style::default().fg(Color::Magenta).add_modifier(Modifier::UNDERLINED))));
            for (i, source) in msg.sources.iter().enumerate() {
                messages_visual.push(Line::from(vec![
                    Span::styled(format!(" [{}] ", i+1), Style::default().fg(Color::Magenta)),
                    Span::styled(&source.title, Style::default().fg(Color::White)),
                    Span::styled(format!(" ({})", source.engine), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
        messages_visual.push(Line::from(""));
    }

    // FIX: Add visual padding at the bottom so auto-scroll reveals the last line clearly
    // This helps prevents text from being "cut off" by the bottom border or input box
    for _ in 0..4 {
        messages_visual.push(Line::from(""));
    }

    let total_lines = messages_visual.len() as u16;
    let view_height = chat_chunks[0].height.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(view_height);

    if app.chat_auto_scroll {
        app.chat_scroll = max_scroll;
    } else if app.chat_scroll > max_scroll {
        app.chat_scroll = max_scroll;
    }

    let chat_style = if app.input_mode == InputMode::ChatHistory { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) };
    f.render_widget(Paragraph::new(messages_visual).block(Block::default().borders(Borders::ALL).title(" Conversation ").border_style(chat_style)).wrap(Wrap { trim: false }).scroll((app.chat_scroll, 0)), chat_chunks[0]);

    let input_block = Block::default().borders(Borders::ALL)
        .border_style(if app.input_mode == InputMode::SearchInput { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) })
        .title(" Message ");
    f.render_widget(Paragraph::new(app.search_input.clone()).block(input_block), chat_chunks[1]);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let msg = match app.current_screen {
        CurrentScreen::Launcher => match app.input_mode {
            InputMode::Normal => "Tab:Switch | q:Quit | Enter:Launch | /:Filter | a:Add | e:Edit",
            _ => "Esc:Cancel | Enter:Confirm"
        },
        CurrentScreen::Search => match app.input_mode {
            InputMode::SearchInput => "Tab:Cycle Focus | Esc:Launcher | Enter:Send | Ctrl+s:Sidebar",
            InputMode::SearchSidebar => "Tab:Cycle Focus | Esc:Launcher | Up/Down:Nav | Enter:Select",
            InputMode::ChatHistory => "Tab:Cycle Focus | Esc:Launcher | Up/Down:Scroll | PgUp/PgDn:Page Scroll",
            _ => "Esc:Back"
        }
    };
    f.render_widget(Paragraph::new(msg).style(Style::default().bg(Color::Blue).fg(Color::White)), area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)]).split(popup_layout[1])[1]
}
fn render_edit_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, f.size()); f.render_widget(Clear, area);
    f.render_widget(Block::default().borders(Borders::ALL).title(" Editor ").style(Style::default().bg(Color::Black)), area);
    let chunks = Layout::default().direction(Direction::Vertical).margin(1).constraints([Constraint::Length(3),Constraint::Length(3),Constraint::Length(3),Constraint::Length(3),Constraint::Min(0)]).split(area);
    let fields = [("Name",&app.active_form.name),("Desc",&app.active_form.desc),("Cmd",&app.active_form.cmd),("URL",&app.active_form.url)];
    for (i,(l,v)) in fields.iter().enumerate() {
        let style = if app.active_form.focus_idx==i { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) };
        f.render_widget(Paragraph::new(v.as_str()).block(Block::default().borders(Borders::ALL).title(*l)).style(style), chunks[i]);
    }
}
fn render_adhoc_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.size()); f.render_widget(Clear, area);
    f.render_widget(Block::default().borders(Borders::ALL).title(" Ad-Hoc ").style(Style::default().bg(Color::Black)), area);
    let chunks = Layout::default().direction(Direction::Vertical).margin(2).constraints([Constraint::Length(3)]).split(area);
    f.render_widget(Paragraph::new(app.adhoc_input.clone()).style(Style::default().fg(Color::Yellow)).block(Block::default().borders(Borders::ALL)), chunks[0]);
}
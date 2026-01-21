use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, EditorState, View};
use crate::parser::render_template;

const STATUS_DURATION_MS: u128 = 1500;

pub(crate) fn render_app(frame: &mut Frame, app: &mut App) {
    match app.view {
        View::List => render_list(frame, app),
        View::Editor => render_editor(frame, app),
        View::Error => render_error(frame, app),
    }
}

fn render_error(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let message = app
        .error_message
        .clone()
        .unwrap_or_else(|| "未知错误".to_string());
    let block = Block::bordered().title("错误");
    let paragraph = Paragraph::new(message)
        .block(block)
        .style(Style::new().fg(Color::Red))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_list(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(area);

    let list_area = layout[0];
    let help_area = layout[1];

    let title = format!("模板列表 ({})", app.templates.len());
    let block = Block::bordered().title(title);
    let inner = inner_rect(list_area);
    app.tree_area = inner;

    let view_height = inner.height as usize;
    app.list_scroll = ensure_visible(
        app.list_scroll,
        app.list_state.selected().unwrap_or(0),
        app.tree_items.len(),
        view_height,
    );

    let start = app.list_scroll;
    let end = (start + view_height).min(app.tree_items.len());
    let visible = &app.tree_items[start..end];

    let items: Vec<ListItem> = visible
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.depth);
            let name = if item.template_index.is_some() {
                item.label.clone()
            } else {
                format!("{}/", item.label)
            };
            ListItem::new(format!("{indent}{name}"))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if let Some(selected) = app.list_state.selected() {
        if selected >= start && selected < end {
            state.select(Some(selected - start));
        }
    }
    frame.render_stateful_widget(list, list_area, &mut state);

    let mut help = "↑↓/j k 选择  Enter/双击 打开  e 编辑  q 退出".to_string();
    if let Some(message) = app
        .list_status
        .as_ref()
        .filter(|msg| msg.since.elapsed().as_millis() <= STATUS_DURATION_MS)
    {
        help.push_str("  |  ");
        help.push_str(&message.text);
    }
    let help = Paragraph::new(help).style(Style::new().fg(Color::DarkGray));
    frame.render_widget(help, help_area);
}

fn render_editor(frame: &mut Frame, app: &mut App) {
    let title = app
        .editor
        .as_ref()
        .and_then(|editor| app.templates.get(editor.template_index))
        .map(|template| format!("预览: {}", template.name))
        .unwrap_or_else(|| "预览".to_string());

    let editor = match app.editor.as_mut() {
        Some(editor) => editor,
        None => return,
    };

    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(area);

    let content_area = layout[0];
    let status_area = layout[1];

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(content_area);

    let form_area = horizontal[0];
    let preview_area = horizontal[1];

    render_fields(frame, editor, form_area);
    let rendered = render_template(&editor.tokens, &editor.fields);
    render_preview(frame, &title, &rendered, preview_area);

    let mut status = "Esc 返回  Tab/↑↓ 切换  Ctrl+C 复制  F5 重随".to_string();
    if let Some(message) = editor
        .status
        .as_ref()
        .filter(|msg| msg.since.elapsed().as_millis() <= STATUS_DURATION_MS)
    {
        status.push_str("  |  ");
        status.push_str(&message.text);
    }
    let status = Paragraph::new(status).style(Style::new().fg(Color::DarkGray));
    frame.render_widget(status, status_area);
}

fn render_fields(frame: &mut Frame, editor: &mut EditorState, area: Rect) {
    let block = Block::bordered().title("参数");
    let inner = inner_rect(area);

    let view_height = inner.height as usize;
    editor.field_scroll = ensure_visible(
        editor.field_scroll,
        editor.active_field,
        editor.fields.len(),
        view_height,
    );

    let start = editor.field_scroll;
    let end = (start + view_height).min(editor.fields.len());
    let visible = &editor.fields[start..end];

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let is_active = start + idx == editor.active_field;
            let mut line = format!("{}: {}", field.label, field.value);
            if is_active {
                line.push_str(" |");
            }
            ListItem::new(Line::from(Span::raw(line)))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Blue).fg(Color::White));

    let mut state = ListState::default();
    if editor.active_field >= start && editor.active_field < end {
        state.select(Some(editor.active_field - start));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_preview(frame: &mut Frame, title: &str, rendered: &str, area: Rect) {
    let paragraph = Paragraph::new(rendered)
        .block(Block::bordered().title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn inner_rect(area: Rect) -> Rect {
    let mut inner = area;
    if inner.width >= 2 {
        inner.x += 1;
        inner.width -= 2;
    }
    if inner.height >= 2 {
        inner.y += 1;
        inner.height -= 2;
    }
    inner
}

fn ensure_visible(current_scroll: usize, selected: usize, total: usize, view_height: usize) -> usize {
    if total == 0 || view_height == 0 {
        return 0;
    }
    let mut scroll = current_scroll.min(total.saturating_sub(1));
    if selected < scroll {
        scroll = selected;
    } else if selected >= scroll + view_height {
        scroll = selected + 1 - view_height;
    }
    scroll
}

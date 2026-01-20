use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use arboard::Clipboard;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use rand::seq::IndexedRandom;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

const DOUBLE_CLICK_MS: u128 = 400;
const STATUS_DURATION_MS: u128 = 1500;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;

    let app = App::load();
    let result = run_app(terminal, app);

    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn run_app(mut terminal: DefaultTerminal, mut app: App) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(100);
    loop {
        terminal.draw(|frame| ui(frame, &mut app))?;

        if app.should_quit {
            break;
        }

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app.on_key(key);
                    }
                }
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Template {
    name: String,
    body: String,
}

#[derive(Clone, Debug)]
struct TreeItem {
    label: String,
    depth: usize,
    template_index: Option<usize>,
}

#[derive(Clone, Debug)]
struct Field {
    name: String,
    label: String,
    value: String,
}

#[derive(Clone, Debug)]
enum Token {
    Text(String),
    Var {
        name: String,
        desc: Option<String>,
        raw: String,
    },
    Random {
        options: Vec<String>,
        choice: String,
        raw: String,
    },
}

#[derive(Clone, Debug)]
struct EditorState {
    template_index: usize,
    tokens: Vec<Token>,
    fields: Vec<Field>,
    active_field: usize,
    field_scroll: usize,
    status: Option<StatusMessage>,
}

#[derive(Clone, Debug)]
struct StatusMessage {
    text: String,
    since: Instant,
}

#[derive(Clone, Debug)]
enum View {
    List,
    Editor,
    Error,
}

#[derive(Clone, Debug)]
struct App {
    templates: Vec<Template>,
    tree_items: Vec<TreeItem>,
    list_state: ListState,
    list_scroll: usize,
    view: View,
    editor: Option<EditorState>,
    error_message: Option<String>,
    last_click: Option<(usize, Instant)>,
    tree_area: Rect,
    should_quit: bool,
}

impl App {
    fn load() -> Self {
        match load_templates() {
            Ok(templates) => {
                let tree_items = build_tree_items(&templates);
                let mut list_state = ListState::default();
                if !tree_items.is_empty() {
                    list_state.select(Some(0));
                }
                Self {
                    templates,
                    tree_items,
                    list_state,
                    list_scroll: 0,
                    view: View::List,
                    editor: None,
                    error_message: None,
                    last_click: None,
                    tree_area: Rect::default(),
                    should_quit: false,
                }
            }
            Err(err) => Self {
                templates: Vec::new(),
                tree_items: Vec::new(),
                list_state: ListState::default(),
                list_scroll: 0,
                view: View::Error,
                editor: None,
                error_message: Some(err),
                last_click: None,
                tree_area: Rect::default(),
                should_quit: false,
            },
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        match self.view {
            View::List => self.on_key_list(key),
            View::Editor => self.on_key_editor(key),
            View::Error => self.on_key_error(key),
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) {
        match self.view {
            View::List => self.on_mouse_list(mouse),
            View::Editor => {}
            View::Error => {}
        }
    }

    fn on_key_error(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }

    fn on_key_list(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_list(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_list(-1),
            KeyCode::Enter => self.open_selected_template(),
            _ => {}
        }
    }

    fn on_mouse_list(&mut self, mouse: MouseEvent) {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        if let Some(index) = self.index_from_mouse(mouse) {
            self.list_state.select(Some(index));
            let now = Instant::now();
            if let Some((last_index, last_time)) = self.last_click {
                if last_index == index && last_time.elapsed().as_millis() <= DOUBLE_CLICK_MS {
                    self.open_selected_template();
                }
            }
            self.last_click = Some((index, now));
        }
    }

    fn on_key_editor(&mut self, key: KeyEvent) {
        let editor = match self.editor.as_mut() {
            Some(editor) => editor,
            None => return,
        };

        match key.code {
            KeyCode::Esc => {
                self.view = View::List;
                return;
            }
            KeyCode::Tab | KeyCode::Down => {
                editor.next_field();
            }
            KeyCode::Up => {
                editor.prev_field();
            }
            KeyCode::Backspace => {
                editor.backspace();
            }
            KeyCode::F(5) => {
                editor.reroll_random();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.copy_rendered();
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                editor.reroll_random();
            }
            KeyCode::Char(ch) => {
                editor.push_char(ch);
            }
            _ => {}
        }
    }

    fn move_list(&mut self, delta: isize) {
        let len = self.tree_items.len();
        if len == 0 {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, (len - 1) as isize) as usize;
        self.list_state.select(Some(next));
    }

    fn open_selected_template(&mut self) {
        let index = match self.list_state.selected() {
            Some(index) => index,
            None => return,
        };
        let template_index = match self.tree_items.get(index).and_then(|item| item.template_index) {
            Some(template_index) => template_index,
            None => return,
        };
        let template = match self.templates.get(template_index) {
            Some(template) => template.clone(),
            None => return,
        };
        let editor = EditorState::new(template_index, &template.body);
        self.editor = Some(editor);
        self.view = View::Editor;
    }

    fn copy_rendered(&mut self) {
        let editor = match self.editor.as_mut() {
            Some(editor) => editor,
            None => return,
        };
        let rendered = render_template(&editor.tokens, &editor.fields);
        match Clipboard::new().and_then(|mut cb| cb.set_text(rendered)) {
            Ok(_) => editor.set_status("已复制"),
            Err(err) => editor.set_status(&format!("复制失败: {err}")),
        }
    }

    fn index_from_mouse(&self, mouse: MouseEvent) -> Option<usize> {
        let area = self.tree_area;
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if mouse.column < area.x
            || mouse.column >= area.x + area.width
            || mouse.row < area.y
            || mouse.row >= area.y + area.height
        {
            return None;
        }
        let row_offset = (mouse.row - area.y) as usize;
        let index = self.list_scroll + row_offset;
        if index >= self.tree_items.len() {
            return None;
        }
        Some(index)
    }
}

impl EditorState {
    fn new(template_index: usize, body: &str) -> Self {
        let tokens = parse_tokens(body);
        let fields = collect_fields(&tokens);
        Self {
            template_index,
            tokens,
            fields,
            active_field: 0,
            field_scroll: 0,
            status: None,
        }
    }

    fn next_field(&mut self) {
        if self.fields.is_empty() {
            return;
        }
        self.active_field = (self.active_field + 1) % self.fields.len();
    }

    fn prev_field(&mut self) {
        if self.fields.is_empty() {
            return;
        }
        if self.active_field == 0 {
            self.active_field = self.fields.len() - 1;
        } else {
            self.active_field -= 1;
        }
    }

    fn push_char(&mut self, ch: char) {
        if let Some(field) = self.fields.get_mut(self.active_field) {
            field.value.push(ch);
        }
    }

    fn backspace(&mut self) {
        if let Some(field) = self.fields.get_mut(self.active_field) {
            field.value.pop();
        }
    }

    fn reroll_random(&mut self) {
        let mut rng = rand::rng();
        for token in &mut self.tokens {
            if let Token::Random {
                options, choice, ..
            } = token
            {
                if let Some(pick) = options.choose(&mut rng) {
                    *choice = pick.clone();
                }
            }
        }
        self.set_status("已重随");
    }

    fn set_status(&mut self, text: &str) {
        self.status = Some(StatusMessage {
            text: text.to_string(),
            since: Instant::now(),
        });
    }
}

fn ui(frame: &mut Frame, app: &mut App) {
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

    let help = Paragraph::new("↑↓/j k 选择  Enter/双击 打开  q 退出")
        .style(Style::new().fg(Color::DarkGray));
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

fn load_templates() -> Result<Vec<Template>, String> {
    let path = prompts_path().ok_or_else(|| "无法定位用户目录".to_string())?;
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("读取失败: {} ({err})", path.display()))?;
    let templates = parse_templates(&content);
    if templates.is_empty() {
        return Err("未找到任何模板，请检查是否有 `## 标题` 段落。".to_string());
    }
    Ok(templates)
}

fn prompts_path() -> Option<PathBuf> {
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)?;
    Some(
        home.join(".config")
            .join("pmt")
            .join("prompts.md"),
    )
}

fn parse_templates(content: &str) -> Vec<Template> {
    let mut templates = Vec::new();
    let mut current_title: Option<String> = None;
    let mut body = String::new();

    for line in content.lines() {
        if let Some(title) = parse_heading(line) {
            if let Some(prev) = current_title.take() {
                let trimmed = trim_trailing_newline(&body);
                templates.push(Template {
                    name: prev,
                    body: trimmed.to_string(),
                });
                body.clear();
            }
            current_title = Some(title);
        } else if current_title.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }

    if let Some(title) = current_title {
        let trimmed = trim_trailing_newline(&body);
        templates.push(Template {
            name: title,
            body: trimmed.to_string(),
        });
    }
    templates
}

fn parse_heading(line: &str) -> Option<String> {
    let rest = line.strip_prefix("##")?;
    if !(rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    let title = rest.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn trim_trailing_newline(input: &str) -> &str {
    let trimmed = input.strip_suffix('\n').unwrap_or(input);
    trimmed.strip_suffix('\r').unwrap_or(trimmed)
}

fn build_tree_items(templates: &[Template]) -> Vec<TreeItem> {
    let mut root = TreeNode::new("");
    for (index, template) in templates.iter().enumerate() {
        let parts: Vec<&str> = template
            .name
            .split('/')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .collect();
        root.insert(&parts, index);
    }

    let mut items = Vec::new();
    root.flatten(0, &mut items);
    items
}

#[derive(Clone, Debug)]
struct TreeNode {
    name: String,
    template_index: Option<usize>,
    children: Vec<TreeNode>,
}

impl TreeNode {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            template_index: None,
            children: Vec::new(),
        }
    }

    fn insert(&mut self, parts: &[&str], template_index: usize) {
        if parts.is_empty() {
            self.template_index = Some(template_index);
            return;
        }
        let part = parts[0];
        let child = self
            .children
            .iter_mut()
            .find(|child| child.name == part);
        match child {
            Some(node) => node.insert(&parts[1..], template_index),
            None => {
                let mut node = TreeNode::new(part);
                node.insert(&parts[1..], template_index);
                self.children.push(node);
            }
        }
    }

    fn flatten(&self, depth: usize, items: &mut Vec<TreeItem>) {
        for child in &self.children {
            items.push(TreeItem {
                label: child.name.clone(),
                depth,
                template_index: child.template_index,
            });
            child.flatten(depth + 1, items);
        }
    }
}

fn parse_tokens(body: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while let Some(start) = body[index..].find('{') {
        let start_idx = index + start;
        if start_idx > index {
            tokens.push(Token::Text(body[index..start_idx].to_string()));
        }
        let after = &body[start_idx + 1..];
        if let Some(end_rel) = after.find('}') {
            let end_idx = start_idx + 1 + end_rel;
            let inner = &body[start_idx + 1..end_idx];
            let raw = body[start_idx..=end_idx].to_string();
            if let Some(token) = parse_placeholder(inner, &raw) {
                tokens.push(token);
            } else {
                tokens.push(Token::Text(raw));
            }
            index = end_idx + 1;
        } else {
            tokens.push(Token::Text(body[start_idx..].to_string()));
            index = body.len();
        }
    }
    if index < body.len() {
        tokens.push(Token::Text(body[index..].to_string()));
    }
    tokens
}

fn parse_placeholder(inner: &str, raw: &str) -> Option<Token> {
    let trimmed = inner.trim();
    if let Some(rest) = trimmed.strip_prefix("random|") {
        let options = parse_random_options(rest);
        if options.is_empty() {
            return Some(Token::Text(raw.to_string()));
        }
        let mut rng = rand::rng();
        let choice = options
            .choose(&mut rng)
            .cloned()
            .unwrap_or_default();
        return Some(Token::Random {
            options,
            choice,
            raw: raw.to_string(),
        });
    }

    let mut parts = trimmed.splitn(2, '|');
    let name = parts.next()?.trim();
    if name.is_empty() {
        return None;
    }
    let desc = parts.next().map(|value| value.trim().to_string());
    Some(Token::Var {
        name: name.to_string(),
        desc,
        raw: raw.to_string(),
    })
}

fn parse_random_options(input: &str) -> Vec<String> {
    let mut options = Vec::new();
    let mut in_quote = false;
    let mut current = String::new();
    for ch in input.chars() {
        if ch == '"' {
            if in_quote {
                options.push(current.clone());
                current.clear();
                in_quote = false;
            } else {
                in_quote = true;
            }
        } else if in_quote {
            current.push(ch);
        }
    }
    if in_quote && !current.is_empty() {
        options.push(current);
    }
    if options.is_empty() {
        options = input
            .split_whitespace()
            .map(|part| part.trim_matches(',').to_string())
            .filter(|part| !part.is_empty())
            .collect();
    }
    options
}

fn collect_fields(tokens: &[Token]) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();
    for token in tokens {
        if let Token::Var { name, desc, .. } = token {
            if fields.iter().any(|field| field.name.as_str() == name.as_str()) {
                continue;
            }
            let label = match desc {
                Some(desc) if !desc.is_empty() => format!("{name} ({desc})"),
                _ => name.clone(),
            };
            fields.push(Field {
                name: name.clone(),
                label,
                value: String::new(),
            });
        }
    }
    fields
}

fn render_template(tokens: &[Token], fields: &[Field]) -> String {
    let mut output = String::new();
    for token in tokens {
        match token {
            Token::Text(text) => output.push_str(text),
            Token::Var { name, raw, .. } => {
                let value = fields
                    .iter()
                    .find(|field| field.name == *name)
                    .map(|field| field.value.as_str())
                    .unwrap_or("");
                if value.is_empty() {
                    output.push_str(raw);
                } else {
                    output.push_str(value);
                }
            }
            Token::Random {
                choice, raw, ..
            } => {
                if choice.is_empty() {
                    output.push_str(raw);
                } else {
                    output.push_str(choice);
                }
            }
        }
    }
    output
}

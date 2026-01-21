use std::env;
use std::time::Instant;

use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use rand::seq::IndexedRandom;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::models::{Field, Template, Token, TreeItem};
use crate::parser::{build_tree_items, collect_fields, parse_tokens, render_template};
use crate::system::{ensure_prompts_file, load_templates, run_editor_command, set_clipboard};

const DOUBLE_CLICK_MS: u128 = 400;

#[derive(Clone, Debug)]
pub(crate) enum View {
    List,
    Editor,
    Error,
}

#[derive(Clone, Debug)]
pub(crate) struct StatusMessage {
    pub(crate) text: String,
    pub(crate) since: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct EditorState {
    pub(crate) template_index: usize,
    pub(crate) tokens: Vec<Token>,
    pub(crate) fields: Vec<Field>,
    pub(crate) active_field: usize,
    pub(crate) field_scroll: usize,
    pub(crate) status: Option<StatusMessage>,
}

#[derive(Clone, Debug)]
pub(crate) struct App {
    pub(crate) templates: Vec<Template>,
    pub(crate) tree_items: Vec<TreeItem>,
    pub(crate) list_state: ListState,
    pub(crate) list_scroll: usize,
    pub(crate) view: View,
    pub(crate) editor: Option<EditorState>,
    pub(crate) error_message: Option<String>,
    pub(crate) last_click: Option<(usize, Instant)>,
    pub(crate) tree_area: Rect,
    pub(crate) should_quit: bool,
    pub(crate) list_status: Option<StatusMessage>,
    pub(crate) needs_redraw: bool,
}

impl App {
    pub(crate) fn load() -> Self {
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
                    list_status: None,
                    needs_redraw: false,
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
                list_status: None,
                needs_redraw: false,
            },
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) {
        match self.view {
            View::List => self.on_key_list(key),
            View::Editor => self.on_key_editor(key),
            View::Error => self.on_key_error(key),
        }
    }

    pub(crate) fn on_mouse(&mut self, mouse: MouseEvent) {
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
            KeyCode::Char('e') => self.open_prompts_in_editor(),
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
        match set_clipboard(&rendered) {
            Ok(_) => editor.set_status("已复制"),
            Err(err) => editor.set_status(&err),
        }
    }

    fn set_list_status(&mut self, text: &str) {
        self.list_status = Some(StatusMessage {
            text: text.to_string(),
            since: Instant::now(),
        });
    }

    fn open_prompts_in_editor(&mut self) {
        let editor = match env::var("EDITOR") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                self.set_list_status("未设置 EDITOR 环境变量");
                return;
            }
        };

        let path = match ensure_prompts_file() {
            Ok(path) => path,
            Err(err) => {
                self.set_list_status(&err);
                return;
            }
        };

        if let Err(err) = run_editor_command(&editor, &path) {
            self.set_list_status(&err);
            return;
        }

        self.needs_redraw = true;

        match load_templates() {
            Ok(templates) => {
                self.tree_items = build_tree_items(&templates);
                self.templates = templates;
                let mut list_state = ListState::default();
                if !self.tree_items.is_empty() {
                    list_state.select(Some(0));
                }
                self.list_state = list_state;
                self.list_scroll = 0;
            }
            Err(err) => self.set_list_status(&err),
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

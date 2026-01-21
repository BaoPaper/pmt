use rand::seq::IndexedRandom;

use crate::models::{Field, Template, Token, TreeItem};

pub(crate) fn parse_templates(content: &str) -> Vec<Template> {
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

pub(crate) fn build_tree_items(templates: &[Template]) -> Vec<TreeItem> {
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
        let child = self.children.iter_mut().find(|child| child.name == part);
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

pub(crate) fn parse_tokens(body: &str) -> Vec<Token> {
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
        let choice = options.choose(&mut rng).cloned().unwrap_or_default();
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

pub(crate) fn collect_fields(tokens: &[Token]) -> Vec<Field> {
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

pub(crate) fn render_template(tokens: &[Token], fields: &[Field]) -> String {
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
            Token::Random { choice, raw, .. } => {
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

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use arboard::Clipboard;
use crossterm::cursor::MoveTo;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::models::Template;
use crate::parser::parse_templates;

const DEFAULT_PROMPTS: &str = "## 示例/问候\n写一封给 {name|收件人} 的简短问候邮件，主题是 {topic|主题}。\n\n## 示例/评审/检查清单\n请评审 {area|模块}，并列出 {random|\"安全\" \"性能\" \"可用性\"} 风险。\n";

pub(crate) fn load_templates() -> Result<Vec<Template>, String> {
    let path = ensure_prompts_file()?;
    let content =
        fs::read_to_string(&path).map_err(|err| format!("读取失败: {} ({err})", path.display()))?;
    let templates = parse_templates(&content);
    if templates.is_empty() {
        return Err("未找到任何模板，请检查是否有 `## 标题` 段落。".to_string());
    }
    Ok(templates)
}

pub(crate) fn ensure_prompts_file() -> Result<PathBuf, String> {
    let path = prompts_path().ok_or_else(|| "无法定位用户目录".to_string())?;
    if path.exists() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建目录失败: {} ({err})", parent.display()))?;
    }
    fs::write(&path, DEFAULT_PROMPTS)
        .map_err(|err| format!("创建模板文件失败: {} ({err})", path.display()))?;
    Ok(path)
}

pub(crate) fn run_editor_command(editor: &str, path: &PathBuf) -> Result<(), String> {
    let mut parts = editor.split_whitespace();
    let command = parts
        .next()
        .ok_or_else(|| "EDITOR 为空".to_string())
        .map(|value| value.to_string())?;
    let args: Vec<String> = parts.map(|part| part.to_string()).collect();

    disable_raw_mode().map_err(|err| format!("退出原始模式失败: {err}"))?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
        .map_err(|err| format!("退出全屏模式失败: {err}"))?;

    let status_result = Command::new(&command).args(&args).arg(path).status();

    let restore_result = execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )
    .map_err(|err| format!("恢复全屏模式失败: {err}"))
    .and_then(|_| enable_raw_mode().map_err(|err| format!("恢复原始模式失败: {err}")));

    let status = match status_result {
        Ok(status) => status,
        Err(err) => {
            let _ = restore_result;
            return Err(format!("启动编辑器失败: {err}"));
        }
    };
    if let Err(err) = restore_result {
        return Err(err);
    }
    if !status.success() {
        return Err(format!("编辑器退出异常: {status}"));
    }
    Ok(())
}

pub(crate) fn set_clipboard(text: &str) -> Result<(), String> {
    Clipboard::new()
        .and_then(|mut cb| cb.set_text(text.to_string()))
        .map_err(|err| format!("复制失败: {err}"))
}

fn prompts_path() -> Option<PathBuf> {
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)?;
    Some(home.join(".config").join("pmt").join("prompts.md"))
}

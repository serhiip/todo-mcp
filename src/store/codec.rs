use super::TodoItem;

pub(crate) const BODY_INDENT: &str = "  ";
pub(crate) const PENDING_PREFIX: &str = "- [ ] ";
pub(crate) const DONE_PREFIX: &str = "- [x] ";

fn parse_id_and_title(rest: &str) -> (u32, String) {
    let rest = rest.trim();
    if let Some(after_hash) = rest.strip_prefix('#') {
        let digits_end = after_hash.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits_end > 0 {
            let id_str = &after_hash[..digits_end];
            if let Ok(id) = id_str.parse::<u32>() {
                let title = after_hash[digits_end..].trim_start().to_string();
                return (id, title);
            }
        }
    }
    (0, rest.to_string())
}

pub fn parse_content(content: &str) -> Vec<TodoItem> {
    let mut items = Vec::new();
    let mut current: Option<(u32, bool, String, String)> = None;
    for line in content.lines() {
        if let Some(stripped) = line.strip_prefix(BODY_INDENT) {
            if let Some((_, _, _, b)) = current.as_mut() {
                if !b.is_empty() {
                    b.push('\n');
                }
                b.push_str(stripped.trim_end());
            }
        } else {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(DONE_PREFIX) {
                if let Some((id, completed, title, body)) = current.take() {
                    items.push(TodoItem { id, completed, title, body });
                }
                let (id, title) = parse_id_and_title(rest.trim());
                current = Some((id, true, title, String::new()));
            } else if let Some(rest) = trimmed.strip_prefix(PENDING_PREFIX) {
                if let Some((id, completed, title, body)) = current.take() {
                    items.push(TodoItem { id, completed, title, body });
                }
                let (id, title) = parse_id_and_title(rest.trim());
                current = Some((id, false, title, String::new()));
            } else if let Some(rest) = trimmed.strip_prefix("- ") {
                if let Some((id, completed, title, body)) = current.take() {
                    items.push(TodoItem { id, completed, title, body });
                }
                let (id, title) = parse_id_and_title(rest.trim());
                current = Some((id, false, title, String::new()));
            }
        }
    }
    if let Some((id, completed, title, body)) = current.take() {
        items.push(TodoItem { id, completed, title, body });
    }
    assign_ids_if_needed(&mut items);
    items
}

pub(crate) fn assign_ids_if_needed(items: &mut [TodoItem]) {
    let mut next = 1u32;
    for item in items.iter_mut() {
        if item.id == 0 {
            item.id = next;
            next += 1;
        } else if item.id >= next {
            next = item.id + 1;
        }
    }
}

pub fn format_item(item: &TodoItem) -> String {
    let title_part = if item.title.is_empty() {
        format!("#{}", item.id)
    } else {
        format!("#{} {}", item.id, item.title)
    };
    let header = if item.completed {
        format!("{}{}", DONE_PREFIX, title_part)
    } else {
        format!("{}{}", PENDING_PREFIX, title_part)
    };
    if item.body.is_empty() {
        header
    } else {
        let body_lines: Vec<&str> = item.body.lines().collect();
        format!(
            "{}\n{}",
            header,
            body_lines
                .iter()
                .map(|s| format!("{}{}", BODY_INDENT, s))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

// [INPUT]: 已确认停止的独立 Chromium profile、原有书签和偏好、一次性初始化标记。
// [OUTPUT]: 首次预置六项收藏与 Google；旧模板仅补官方仓库，不恢复用户删除的收藏或覆盖偏好。
// [POS]: 独立环境冷启动模板；不访问身份联动或其他浏览器资料，不在运行中写文件。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
use super::{private, profile, server};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const MARKER: &str = "ccbaga-defaults-v1.json";
pub(super) const START_PAGE: &str = "https://www.google.com/";
const LINKS: &[(&str, &str)] = &[
    ("Claude 新对话", "https://claude.ai/new"),
    ("ChatGPT", "https://chatgpt.com/"),
    ("Gmail", "https://mail.google.com/"),
    ("Outlook", "https://outlook.live.com/mail/"),
    ("Claude Platform", "https://platform.claude.com/"),
    ("CCBaga GitHub", "https://github.com/liseami/CCBaga"),
];
const LIMIT: u64 = 32 * 1024 * 1024;

fn read(profile: &Path, path: &Path) -> Result<Option<Value>, String> {
    profile::check_path(profile, path)?;
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("无法读取浏览器初始设置。".into()),
        Ok(_) => private::safe(path)?,
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| "无法读取浏览器初始设置。")?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取浏览器初始设置。")?;
    if bytes.len() as u64 > LIMIT {
        return Err("浏览器初始设置文件过大，未修改。".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "浏览器书签或偏好损坏，未覆盖。".into())
}

fn stamp() -> String {
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    (unix + 11_644_473_600_000_000u128).to_string()
}

fn guid() -> Result<String, String> {
    let raw = server::nonce()?;
    // UUIDv4 layout, with the remaining bits from the existing cryptographic nonce.
    let variant = b"89ab"[(raw.as_bytes()[16] & 3) as usize] as char;
    Ok(format!(
        "{}-{}-4{}-{}{}-{}",
        &raw[..8],
        &raw[8..12],
        &raw[13..16],
        variant,
        &raw[17..20],
        &raw[20..32]
    ))
}

fn folder(id: i64, name: &str, date: &str) -> Result<Value, String> {
    Ok(
        json!({"children":[], "date_added":date, "date_modified":date, "date_last_used":"0",
        "guid":guid()?, "id":id.to_string(), "name":name, "type":"folder"}),
    )
}

fn empty_bookmarks(date: &str) -> Result<Value, String> {
    Ok(json!({"version":1, "roots":{
        "bookmark_bar":folder(1, "书签栏", date)?,
        "other":folder(2, "其他书签", date)?,
        "synced":folder(3, "移动设备书签", date)?
    }}))
}

fn canonical(url: &str) -> String {
    reqwest::Url::parse(url)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| url.into())
}

fn scan(
    node: &Value,
    ids: &mut HashSet<i64>,
    urls: &mut HashSet<String>,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 || !node.is_object() {
        return Err("书签结构无效，未覆盖。".into());
    }
    let id = node
        .get("id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .ok_or("书签编号无效，未覆盖。")?;
    if !ids.insert(id) || !node.get("name").is_some_and(Value::is_string) {
        return Err("书签编号或名称无效，未覆盖。".into());
    }
    match node.get("type").and_then(Value::as_str) {
        Some("folder") => {
            for child in node
                .get("children")
                .and_then(Value::as_array)
                .ok_or("书签目录无效，未覆盖。")?
            {
                scan(child, ids, urls, depth + 1)?;
            }
        }
        Some("url") => {
            urls.insert(canonical(
                node.get("url")
                    .and_then(Value::as_str)
                    .ok_or("书签地址无效，未覆盖。")?,
            ));
        }
        _ => return Err("书签类型无效，未覆盖。".into()),
    }
    Ok(())
}

fn supplement(bookmarks: &mut Value, date: &str, links: &[(&str, &str)]) -> Result<bool, String> {
    if bookmarks.get("version").and_then(Value::as_i64) != Some(1) {
        return Err("书签版本无效，未覆盖。".into());
    }
    let roots = bookmarks
        .get("roots")
        .and_then(Value::as_object)
        .ok_or("书签根目录无效，未覆盖。")?;
    for key in ["bookmark_bar", "other", "synced"] {
        if roots
            .get(key)
            .and_then(|v| v.get("type"))
            .and_then(Value::as_str)
            != Some("folder")
        {
            return Err("书签根目录无效，未覆盖。".into());
        }
    }
    let mut ids = HashSet::new();
    let mut urls = HashSet::new();
    // Preserve extra roots as well, including their IDs and URL deduplication.
    for (key, node) in roots {
        if ["bookmark_bar", "other", "synced"].contains(&key.as_str())
            || node
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| matches!(kind, "folder" | "url"))
        {
            scan(node, &mut ids, &mut urls, 0)?;
        }
    }
    let mut next = ids.iter().copied().max().unwrap_or(0);
    let mut added = Vec::new();
    for (name, url) in links {
        if urls.insert(canonical(url)) {
            next = next.checked_add(1).ok_or("书签编号已耗尽。")?;
            added.push(
                json!({"date_added":date, "date_last_used":"0", "guid":guid()?,
                "id":next.to_string(), "name":name, "type":"url", "url":url}),
            );
        }
    }
    if added.is_empty() {
        return Ok(false);
    }
    let bar = &mut bookmarks["roots"]["bookmark_bar"];
    bar["children"]
        .as_array_mut()
        .ok_or("书签目录无效，未覆盖。")?
        .extend(added);
    bar["date_modified"] = json!(date);
    // These are derived fields, not user data. Chromium rebuilds them when it writes.
    // Codec: chromium/components/bookmarks/browser/bookmark_codec.cc (version 1).
    let object = bookmarks.as_object_mut().ok_or("书签结构无效，未覆盖。")?;
    object.remove("checksum");
    object.remove("checksum_sha256");
    Ok(true)
}

fn preferences(value: &mut Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("浏览器偏好格式无效，未覆盖。".into());
    }
    for key in ["bookmark_bar", "session"] {
        if !value[key].is_null() && !value[key].is_object() {
            return Err("浏览器偏好字段无效，未覆盖。".into());
        }
    }
    if !value["session"]["startup_urls"].is_null() && !value["session"]["startup_urls"].is_array() {
        return Err("浏览器启动页设置无效，未覆盖。".into());
    }
    if value["session"]["startup_urls"]
        .as_array()
        .is_some_and(|urls| urls.iter().any(|url| !url.is_string()))
    {
        return Err("浏览器启动页设置无效，未覆盖。".into());
    }
    let mode = value["session"]["restore_on_startup"].as_i64();
    if !value["session"]["restore_on_startup"].is_null() && mode.is_none() {
        return Err("浏览器启动模式无效，未覆盖。".into());
    }
    let custom_urls = value["session"]["startup_urls"]
        .as_array()
        .is_some_and(|a| !a.is_empty());
    if matches!(mode, None | Some(5)) && !custom_urls {
        value["session"]["restore_on_startup"] = json!(4);
        value["session"]["startup_urls"] = json!([START_PAGE]);
    }
    if value["homepage"].is_null() && value["homepage_is_newtabpage"].is_null() {
        value["homepage"] = json!(START_PAGE);
        value["homepage_is_newtabpage"] = json!(false);
    }
    value["bookmark_bar"]["show_on_all_tabs"] = json!(true);
    Ok(())
}

/// Per-file atomic commits, with all data parsed/validated first. A failed commit leaves
/// no success marker; retries deduplicate already-added URLs before committing again.
pub(super) fn apply(
    profile_path: &Path,
    mut stopped: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    stopped()?;
    private::parents(profile_path)?;
    private::safe(profile_path)?;
    let marker = profile_path.join(MARKER);
    let upgrade = match read(profile_path, &marker)? {
        None => false,
        Some(value) if value == json!({"version":1}) => true,
        Some(value) if value == json!({"version":2}) => return Ok(()),
        Some(_) => return Err("浏览器初始设置标记无效，未覆盖。".into()),
    };
    let default = profile_path.join("Default");
    profile::check_path(profile_path, &default)?;
    private::safe(&default)?;
    let bookmarks_path = default.join("Bookmarks");
    let prefs_path = default.join("Preferences");
    let date = stamp();
    let existing = read(profile_path, &bookmarks_path)?;
    let is_new = existing.is_none();
    let mut bookmarks = existing.unwrap_or(empty_bookmarks(&date)?);
    let changed = supplement(
        &mut bookmarks,
        &date,
        if upgrade { &LINKS[5..] } else { LINKS },
    )? || is_new;
    // Upgrading an existing template must not reapply old bookmarks or any preference.
    let prefs = if upgrade {
        None
    } else {
        let mut value = read(profile_path, &prefs_path)?.unwrap_or(json!({}));
        preferences(&mut value)?;
        Some(value)
    };
    // All paths, including the marker, were checked before the first mutation.
    stopped()?;
    if changed {
        private::write(
            &bookmarks_path,
            &serde_json::to_vec_pretty(&bookmarks).map_err(|_| "书签编码失败。")?,
        )?;
    }
    stopped()?;
    if let Some(prefs) = prefs {
        private::write(
            &prefs_path,
            &serde_json::to_vec_pretty(&prefs).map_err(|_| "偏好编码失败。")?,
        )?;
    }
    stopped()?;
    private::write(&marker, b"{\"version\":2}")
}

#[cfg(test)]
mod tests;

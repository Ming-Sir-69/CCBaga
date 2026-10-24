// [INPUT]: 独占测试目录和虚构书签/偏好，无用户凭据。
// [OUTPUT]: 一次性模板、数据保留、去重、损坏与链接拒绝回归。
// [POS]: defaults 的纯文件边界验证；真实浏览器读回由主线程执行。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
use super::*;

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../_trash/bookmark-default-tests");
        fs::create_dir_all(&base).unwrap();
        let path = base.join(server::nonce().unwrap());
        private::directory(&path.join("Default")).unwrap();
        private::write(&path.join("Default/Preferences"), b"{}").unwrap();
        Self(path)
    }
    fn read(&self, name: &str) -> Value {
        serde_json::from_slice(&fs::read(self.0.join(name)).unwrap()).unwrap()
    }
    fn write(&self, name: &str, value: &Value) {
        private::write(&self.0.join(name), &serde_json::to_vec(value).unwrap()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fresh_profile_gets_six_valid_bookmarks_and_google_once() {
    let fixture = Fixture::new();
    apply(&fixture.0, || Ok(())).unwrap();
    let b = fixture.read("Default/Bookmarks");
    let children = b["roots"]["bookmark_bar"]["children"].as_array().unwrap();
    assert_eq!(children.len(), 6);
    let mut ids = HashSet::new();
    let mut urls = HashSet::new();
    for node in b["roots"].as_object().unwrap().values() {
        scan(node, &mut ids, &mut urls, 0).unwrap();
    }
    assert_eq!(ids.len(), 9);
    for (node, (_, url)) in children.iter().zip(LINKS) {
        assert_eq!(node["url"], *url);
        assert!(
            node["date_added"].as_str().unwrap().parse::<u64>().unwrap() > 11_644_473_600_000_000
        );
        let id = node["guid"].as_str().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!("89ab".contains(&id[19..20]));
    }
    let p = fixture.read("Default/Preferences");
    assert_eq!(p["bookmark_bar"]["show_on_all_tabs"], true);
    assert_eq!(p["session"]["restore_on_startup"], 4);
    assert_eq!(p["session"]["startup_urls"], json!([START_PAGE]));
    assert_eq!(p["homepage"], START_PAGE);
    assert_eq!(fixture.read(MARKER), json!({"version":2}));
}

#[test]
fn supplement_preserves_existing_names_order_extra_fields_and_deduplicates_all_roots() {
    let fixture = Fixture::new();
    let mut original = empty_bookmarks("123").unwrap();
    let keep = json!({"id":"30","name":"我的自定义 ChatGPT 名字","type":"url","url":"https://chatgpt.com","guid":guid().unwrap(),"unknown":{"keep":true}});
    original["roots"]["bookmark_bar"]["children"] = json!([keep]);
    original["roots"]["other"]["children"] = json!([{"id":"31","name":"邮箱旧名称","type":"url","url":"https://mail.google.com/","guid":guid().unwrap()}]);
    original["unknown_top"] = json!("retain");
    original["roots"]["unknown_metadata"] = json!({"retain":1});
    fixture.write("Default/Bookmarks", &original);
    fixture.write("Default/Preferences", &json!({"session":{"restore_on_startup":1,"startup_urls":["https://example.org/"]},"homepage":"https://example.org/","unknown":"keep"}));
    apply(&fixture.0, || Ok(())).unwrap();
    let b = fixture.read("Default/Bookmarks");
    assert_eq!(b["roots"]["bookmark_bar"]["children"][0], keep);
    assert_eq!(b["roots"]["other"], original["roots"]["other"]);
    assert_eq!(
        b["roots"]["bookmark_bar"]["children"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(b["unknown_top"], "retain");
    assert_eq!(b["roots"]["unknown_metadata"], json!({"retain":1}));
    let p = fixture.read("Default/Preferences");
    assert_eq!(
        p["session"],
        json!({"restore_on_startup":1,"startup_urls":["https://example.org/"]})
    );
    assert_eq!(p["homepage"], "https://example.org/");
    assert_eq!(p["unknown"], "keep");
}

#[test]
fn successful_marker_does_not_restore_user_deleted_links_or_override_preferences_again() {
    let fixture = Fixture::new();
    apply(&fixture.0, || Ok(())).unwrap();
    let mut b = fixture.read("Default/Bookmarks");
    b["roots"]["bookmark_bar"]["children"] = json!([]);
    fixture.write("Default/Bookmarks", &b);
    let p = json!({"bookmark_bar":{"show_on_all_tabs":false},"session":{"restore_on_startup":1}});
    fixture.write("Default/Preferences", &p);
    apply(&fixture.0, || Ok(())).unwrap();
    assert_eq!(fixture.read("Default/Bookmarks"), b);
    assert_eq!(fixture.read("Default/Preferences"), p);
}

#[test]
fn malformed_bookmarks_or_preferences_are_rejected_before_any_default_write() {
    for bad in [
        json!({"session":"bad"}),
        json!({"bookmark_bar":false}),
        json!({"session":{"startup_urls":[1]}}),
    ] {
        let fixture = Fixture::new();
        let b = empty_bookmarks("123").unwrap();
        fixture.write("Default/Bookmarks", &b);
        fixture.write("Default/Preferences", &bad);
        assert!(apply(&fixture.0, || Ok(())).is_err());
        assert_eq!(fixture.read("Default/Bookmarks"), b);
        assert_eq!(fixture.read("Default/Preferences"), bad);
        assert!(!fixture.0.join(MARKER).exists());
    }
    let fixture = Fixture::new();
    fixture.write("Default/Bookmarks", &json!({"version":1}));
    assert!(apply(&fixture.0, || Ok(())).is_err());
    assert_eq!(fixture.read("Default/Preferences"), json!({}));
    assert!(!fixture.0.join(MARKER).exists());
}

#[test]
fn stopped_guard_failure_leaves_no_marker_and_retry_does_not_duplicate() {
    let fixture = Fixture::new();
    let mut calls = 0;
    assert!(apply(&fixture.0, || {
        calls += 1;
        if calls == 3 {
            Err("started".into())
        } else {
            Ok(())
        }
    })
    .is_err());
    assert!(!fixture.0.join(MARKER).exists());
    assert_eq!(
        fixture.read("Default/Bookmarks")["roots"]["bookmark_bar"]["children"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    apply(&fixture.0, || Ok(())).unwrap();
    assert_eq!(
        fixture.read("Default/Bookmarks")["roots"]["bookmark_bar"]["children"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn explicit_startup_page_is_preserved_even_with_new_tab_mode() {
    let mut prefs =
        json!({"session":{"restore_on_startup":5,"startup_urls":["https://example.org/"]}});
    let session = prefs["session"].clone();
    preferences(&mut prefs).unwrap();
    assert_eq!(prefs["session"], session);
}

#[cfg(unix)]
#[test]
fn linked_bookmark_file_is_never_followed_or_overwritten() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = Fixture::new();
    let target = outside.0.join("valuable");
    fs::write(&target, "retain").unwrap();
    symlink(&target, fixture.0.join("Default/Bookmarks")).unwrap();
    assert!(apply(&fixture.0, || Ok(())).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "retain");
    assert_eq!(fixture.read("Default/Preferences"), json!({}));
}

#[test]
fn v1_upgrade_only_adds_github_and_never_reapplies_old_links_or_preferences() {
    let fixture = Fixture::new();
    let mut b = empty_bookmarks("123").unwrap();
    let keep = json!({"id":"4","name":"保留自定义入口","type":"url","url":"https://example.org/","guid":guid().unwrap()});
    b["roots"]["bookmark_bar"]["children"] = json!([keep]);
    fixture.write("Default/Bookmarks", &b);
    fixture.write(MARKER, &json!({"version":1}));
    let prefs = json!({"session":{"restore_on_startup":1},"bookmark_bar":{"show_on_all_tabs":false},"homepage":"https://example.org/"});
    fixture.write("Default/Preferences", &prefs);
    let before = fs::read(fixture.0.join("Default/Preferences")).unwrap();
    apply(&fixture.0, || Ok(())).unwrap();
    let after = fixture.read("Default/Bookmarks");
    let children = after["roots"]["bookmark_bar"]["children"]
        .as_array()
        .unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], keep);
    assert_eq!(children[1]["url"], "https://github.com/liseami/CCBaga");
    assert_eq!(
        fs::read(fixture.0.join("Default/Preferences")).unwrap(),
        before
    );
    assert_eq!(fixture.read(MARKER), json!({"version":2}));
    let mut deleted = after;
    deleted["roots"]["bookmark_bar"]["children"] = json!([keep]);
    fixture.write("Default/Bookmarks", &deleted);
    apply(&fixture.0, || Ok(())).unwrap();
    assert_eq!(fixture.read("Default/Bookmarks"), deleted);
}

#[test]
fn v1_upgrade_deduplicates_existing_github_and_rejects_corruption_without_marker_change() {
    let fixture = Fixture::new();
    let mut b = empty_bookmarks("123").unwrap();
    let existing = json!({"id":"4","name":"作者源码","type":"url","url":"https://github.com/liseami/CCBaga","guid":guid().unwrap()});
    b["roots"]["other"]["children"] = json!([existing]);
    fixture.write("Default/Bookmarks", &b);
    fixture.write(MARKER, &json!({"version":1}));
    apply(&fixture.0, || Ok(())).unwrap();
    assert_eq!(fixture.read("Default/Bookmarks"), b);
    fixture.write(MARKER, &json!({"version":1}));
    fixture.write("Default/Bookmarks", &json!({"version":1}));
    assert!(apply(&fixture.0, || Ok(())).is_err());
    assert_eq!(fixture.read(MARKER), json!({"version":1}));
    assert_eq!(fixture.read("Default/Preferences"), json!({}));
}

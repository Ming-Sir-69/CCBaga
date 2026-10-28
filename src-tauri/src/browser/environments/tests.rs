// [INPUT]: 临时独占环境目录、虚构机器配置、可注入的进程占用检查。
// [OUTPUT]: 环境隔离、配置保留、生命周期与删除保护回归；不访问真实账号。
// [POS]: 独立环境持久边界测试，不把文件样本视为真实浏览器验收。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
use super::*;
use serde_json::{json, Value};

#[cfg(target_os = "macos")]
#[test]
fn native_activity_decodes_boolean_fields_for_an_unused_profile() {
    let fixture = Fixture::new();
    let observed = profile::activity(&fixture.0, false).unwrap();
    assert!(!observed.main_running);
    assert_eq!(observed.process_count, 0);
    assert_eq!(observed.visible_window, Some(false));
}

#[test]
fn lifecycle_distinguishes_visible_windows_background_residual_and_unknown() {
    let open = profile::Activity { main_running: true, process_count: 3, visible_window: Some(true) };
    assert_eq!(lifecycle(Some(&open), true, false), "open");
    let background = profile::Activity { visible_window: Some(false), ..open.clone() };
    assert_eq!(lifecycle(Some(&background), true, false), "background");
    assert!(background.busy()); // Red-close is not process termination.
    let residual = profile::Activity { main_running: false, process_count: 1, visible_window: Some(false) };
    assert_eq!(lifecycle(Some(&residual), false, false), "residual");
    assert!(residual.busy()); // A vanished lock/main PID must not authorize cleanup.
    let stopped = profile::Activity { main_running: false, process_count: 0, visible_window: Some(false) };
    assert_eq!(lifecycle(Some(&stopped), false, false), "stopped");
    assert!(!stopped.busy());
    assert_eq!(lifecycle(None, false, false), "unknown");
}

#[test]
fn lifecycle_preserves_start_and_reap_guards_without_claiming_a_window() {
    let no_process = profile::Activity { main_running: false, process_count: 0, visible_window: Some(false) };
    assert_eq!(lifecycle(Some(&no_process), true, true), "starting");
    assert_eq!(lifecycle(Some(&no_process), true, false), "closing");
    assert_eq!(lifecycle(Some(&no_process), false, true), "checking");
    let unknown_window = profile::Activity { main_running: true, process_count: 1, visible_window: None };
    assert_eq!(lifecycle(Some(&unknown_window), false, false), "running");
}

#[test]
fn imported_brave_profile_is_owned_and_original_source_is_not_a_runtime_dependency() {
    let registry = Fixture::new();
    let original = Fixture::new();
    fs::write(original.0.join("original-login"), "synthetic login").unwrap();
    let mut item = create(&registry.0, "Imported", false, None, &machine()).unwrap();
    item.browser = BrowserKind::Brave;
    save(&registry.0, &item).unwrap();
    let profile = profile_data(&registry.0, &item.id, "synthetic login");
    assert!(load(&registry.0, &item.id).unwrap().browser == BrowserKind::Brave);
    rename(&registry.0, &item.id, "My account").unwrap();
    assert_eq!(fs::read_to_string(profile.join("Default/Cookies")).unwrap(), "synthetic login");
    assert!(remove(&registry.0, &item.id, || Err("running".into())).is_err());
    assert!(profile.exists());
    remove(&registry.0, &item.id, || Ok(())).unwrap();
    assert!(!profile.exists());
    assert_eq!(fs::read_to_string(original.0.join("original-login")).unwrap(), "synthetic login");
}

#[test]
fn brave_launch_keeps_native_browser_settings_and_never_injects_fingerprint_flags() {
    let args = brave_launch_arguments(Path::new("/test/Owned Brave"));
    assert!(args.contains(&"--user-data-dir=/test/Owned Brave".into()));
    assert!(args.contains(&"/Applications/Brave Browser.app".into()));
    assert_eq!(args.last().unwrap(), "brave://newtab/");
    assert!(!args.iter().any(|a| a.contains("fingerprint") || a.contains("proxy") || a.contains("timezone") || a.contains("lang=")));
}

#[test]
fn first_window_is_created_but_session_restore_never_adds_a_window_or_page() {
    let profile = Path::new("/test/independent profile");
    let config = Config::default();
    let cold = window_arguments(&config, &machine(), profile, "148.0.0.0", false, &StartupPlan::Google);
    assert_eq!(cold.last().unwrap(), defaults::START_PAGE);
    assert_eq!(cold.iter().filter(|arg| arg.as_str() == "--new-window").count(), 1);
    assert!(!cold.iter().any(|arg| arg.is_empty()));
    assert!(cold.iter().any(|arg| arg == "--user-data-dir=/test/independent profile"));
    let restored = window_arguments(&config, &machine(), profile, "148.0.0.0", false, &StartupPlan::Restore);
    assert!(restored.iter().any(|arg| arg == "--restore-last-session"));
    assert!(restored.iter().all(|arg| arg.starts_with("--") && arg != "--new-window"));
    let empty_running = window_arguments(&config, &machine(), profile, "148.0.0.0", true, &StartupPlan::Restore);
    assert_eq!(empty_running.last().unwrap(), defaults::START_PAGE);
    assert_eq!(empty_running.iter().filter(|arg| arg.as_str() == "--new-window").count(), 1);
    let native = window_arguments(&config, &machine(), profile, "148.0.0.0", false, &StartupPlan::Native);
    assert_eq!(native.iter().filter(|arg| arg.as_str() == "--new-window").count(), 1);
    assert!(!native.iter().any(|arg| arg == defaults::START_PAGE));
    let custom = window_arguments(&config, &machine(), profile, "148.0.0.0", false, &StartupPlan::Pages(vec!["https://example.org/".into()]));
    assert_eq!(custom.iter().filter(|arg| arg.as_str() == "--new-window").count(), 1);
    assert_eq!(custom.last().unwrap(), "https://example.org/");
}

#[test]
fn native_startup_choice_and_custom_pages_override_the_google_default() {
    use serde_json::json;
    let none = serde_json::Value::Null;
    assert_eq!(startup_plan_from_values(&none, &none).unwrap(), StartupPlan::Google);
    let restore = json!({"session":{"restore_on_startup":1}});
    assert_eq!(startup_plan_from_values(&restore, &none).unwrap(), StartupPlan::Restore);
    let native = json!({"session":{"restore_on_startup":5}});
    assert_eq!(startup_plan_from_values(&restore, &native).unwrap(), StartupPlan::Native);
    let custom = json!({"session":{"restore_on_startup":4,"startup_urls":["https://example.org/","chrome://settings/"]}});
    assert_eq!(startup_plan_from_values(&custom, &none).unwrap(), StartupPlan::Pages(vec!["https://example.org/".into(),"chrome://settings/".into()]));
    let invalid = json!({"session":{"restore_on_startup":4,"startup_urls":["--user-data-dir=/other"]}});
    assert!(startup_plan_from_values(&invalid, &none).is_err());
}

#[test]
fn startup_policy_reads_secure_session_fields_without_modifying_protected_preferences() {
    use serde_json::json;
    let root = Fixture::new();
    let item = create(&root.0, "Read-only prefs", false, None, &machine()).unwrap();
    let profile = profile_data(&root.0, &item.id, "synthetic cookie");
    let ordinary = profile.join("Default/Preferences");
    let secure = profile.join("Default/Secure Preferences");
    profile::write_json(&ordinary, &json!({"session":{"restore_on_startup":4,"startup_urls":[defaults::START_PAGE]}})).unwrap();
    profile::write_json(&secure, &json!({"session":{"restore_on_startup":1},"protection":{"macs":{"test":"fixture"}}})).unwrap();
    let ordinary_before=fs::read(&ordinary).unwrap();
    let secure_before=fs::read(&secure).unwrap();
    assert_eq!(startup_plan(&profile).unwrap(), StartupPlan::Restore);
    assert_eq!(fs::read(&ordinary).unwrap(), ordinary_before);
    assert_eq!(fs::read(&secure).unwrap(), secure_before);
    assert_eq!(fs::read_to_string(profile.join("Default/Cookies")).unwrap(), "synthetic cookie");
}

#[test]
fn profile_display_name_keeps_saved_startup_preferences_and_legacy_history_target() {
    let root = Fixture::new();
    let item = create(&root.0, "Display name", false, None, &machine()).unwrap();
    let profile = profile_data(&root.0, &item.id, "synthetic cookie");
    profile::preferences(&profile, &item.config, &machine()).unwrap();
    let prefs = profile.join("Default/Preferences");
    let mut original: serde_json::Value = serde_json::from_slice(&fs::read(&prefs).unwrap()).unwrap();
    original["session"] = serde_json::json!({"restore_on_startup": 1, "startup_urls": ["https://example.org/"]});
    original["homepage"] = serde_json::json!("https://example.org/");
    profile::write_json(&prefs, &original).unwrap();
    fs::write(profile.join("CCBaga Start.html"), "legacy history target").unwrap();
    name_profile(&profile, &item).unwrap();
    let updated: serde_json::Value = serde_json::from_slice(&fs::read(&prefs).unwrap()).unwrap();
    assert_eq!(updated["session"], original["session"]);
    assert_eq!(updated["homepage"], original["homepage"]);
    assert_eq!(updated["profile"]["name"], "Display name");
    assert_eq!(fs::read_to_string(profile.join("Default/Cookies")).unwrap(), "synthetic cookie");
    assert_eq!(fs::read_to_string(profile.join("CCBaga Start.html")).unwrap(), "legacy history target");
}

#[test]
fn rename_changes_only_display_metadata_without_rewriting_live_profile() {
    let root = Fixture::new();
    let item = create(&root.0, "原名称", false, None, &machine()).unwrap();
    let profile = profile_data(&root.0, &item.id, "synthetic login");
    let mut active = load(&root.0, &item.id).unwrap();
    active.launched_at = Some(now());
    save(&root.0, &active).unwrap();
    let config = serde_json::to_value(&active.config).unwrap();
    rename(&root.0, &item.id, "新的名称").unwrap();
    let updated = load(&root.0, &item.id).unwrap();
    assert_eq!(updated.name, "新的名称");
    assert_eq!(updated.launched_at, active.launched_at);
    assert_eq!(serde_json::to_value(&updated.config).unwrap(), config);
    assert_eq!(fs::read_to_string(profile.join("Default/Cookies")).unwrap(), "synthetic login");
    assert!(rename(&root.0, &item.id, "\n").is_err());
    assert_eq!(load(&root.0, &item.id).unwrap().name, "新的名称");
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../_trash/environment-tests");
        fs::create_dir_all(&base).unwrap();
        let path = base.join(server::nonce().unwrap());
        private::create_directory(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn machine() -> Machine {
    Machine {
        platform: "macos".into(),
        version: "15.0.0".into(),
        cores: 8,
        timezone: "Asia/Taipei".into(),
        languages: vec!["zh-Hant".into()],
        locale: "zh_TW".into(),
        locale_country: "TW".into(),
        location_enabled: None,
        timezones: vec!["Asia/Taipei".into(), "America/New_York".into()],
        system_proxy: String::new(),
    }
}
fn profile_data(root: &Path, id: &str, value: &str) -> PathBuf {
    let profile = directory(root, id).unwrap().join("profile");
    private::directory(&profile.join("Default/Local Storage")).unwrap();
    fs::write(profile.join("Default/Cookies"), value).unwrap();
    fs::write(profile.join("Default/Local Storage/test"), value).unwrap();
    profile
}

#[test]
fn named_environments_keep_distinct_storage_and_stable_configs() {
    let root = Fixture::new();
    let a = create(&root.0, "长期甲", false, None, &machine()).unwrap();
    let b = create(&root.0, "长期乙", false, None, &machine()).unwrap();
    assert_ne!(a.id, b.id);
    assert_ne!(a.config.seed, b.config.seed);
    let pa = profile_data(&root.0, &a.id, "A");
    let pb = profile_data(&root.0, &b.id, "B");
    let mut edited = load(&root.0, &a.id).unwrap();
    edited.name = "新名称".into();
    edited.config.timezone = "Asia/Taipei".into();
    save(&root.0, &edited).unwrap();
    assert_eq!(load(&root.0, &a.id).unwrap().config.seed, a.config.seed);
    assert_eq!(fs::read_to_string(pa.join("Default/Cookies")).unwrap(), "A");
    assert_eq!(fs::read_to_string(pb.join("Default/Cookies")).unwrap(), "B");
    remove(&root.0, &a.id, || Ok(())).unwrap();
    assert!(!pa.exists());
    assert!(pb.exists());
    assert_eq!(list(&root.0).unwrap().len(), 1);
}

#[test]
fn deletion_preserves_identity_profiles_and_refuses_running_or_unknown_files() {
    let fixture = Fixture::new();
    let root = fixture.0.join("environments");
    let identity = fixture.0.join("contexts/existing/profile");
    private::directory(&identity).unwrap();
    fs::write(identity.join("Cookies"), "existing login").unwrap();
    let item = create(&root, "临时", true, None, &machine()).unwrap();
    let profile = profile_data(&root, &item.id, "temporary");
    assert!(remove(&root, &item.id, || Err("running".into())).is_err());
    assert!(profile.join("Default/Cookies").exists());
    let unknown = directory(&root, &item.id).unwrap().join("unexpected.txt");
    fs::write(&unknown, "retain").unwrap();
    assert!(remove(&root, &item.id, || Ok(())).is_err());
    assert!(profile.join("Default/Cookies").exists());
    fs::remove_file(unknown).unwrap();
    remove(&root, &item.id, || Ok(())).unwrap();
    assert_eq!(
        fs::read_to_string(identity.join("Cookies")).unwrap(),
        "existing login"
    );
}

#[test]
fn temporary_lifecycle_preserves_unopened_and_starting_environments() {
    let root = Fixture::new();
    let mut item = create(&root.0, "一次性", true, None, &machine()).unwrap();
    assert!(!cleanup_candidate(&item, 100));
    item.cleanup_pending = true;
    item.launched_at = Some(100);
    assert!(!cleanup_candidate(&item, 110));
    assert!(cleanup_candidate(&item, 131));
    item.launched_at = None; // explicit normal exit
    assert!(cleanup_candidate(&item, 101));
    item.temporary = false;
    assert!(!cleanup_candidate(&item, 1000));
}

#[test]
fn invalid_ids_names_and_corrupt_manifests_never_select_other_directories() {
    let root = Fixture::new();
    for id in [
        "../contexts",
        "/tmp/x",
        "",
        "A0000000000000000000000000000000",
    ] {
        assert!(directory(&root.0, id).is_err());
    }
    for label in ["", "  ", "bad\nname", &"字".repeat(81)] {
        assert!(create(&root.0, label, false, None, &machine()).is_err());
    }
    let item = create(&root.0, "valid", false, None, &machine()).unwrap();
    let file = directory(&root.0, &item.id).unwrap().join(MANIFEST);
    fs::write(&file, "{}").unwrap();
    assert!(remove(&root.0, &item.id, || Ok(())).is_err());
    assert!(file.exists());
}

#[cfg(unix)]
#[test]
fn symlinked_environment_and_profile_are_rejected_before_deletion() {
    use std::os::unix::fs::symlink;
    let root = Fixture::new();
    let other = Fixture::new();
    fs::write(other.0.join("private"), "retain").unwrap();
    let id = "a".repeat(32);
    symlink(&other.0, root.0.join(&id)).unwrap();
    assert!(load(&root.0, &id).is_err());
    fs::remove_file(root.0.join(&id)).unwrap();
    let item = create(&root.0, "link", true, None, &machine()).unwrap();
    symlink(
        &other.0,
        directory(&root.0, &item.id).unwrap().join("profile"),
    )
    .unwrap();
    assert!(remove(&root.0, &item.id, || Ok(())).is_err());
    assert_eq!(
        fs::read_to_string(other.0.join("private")).unwrap(),
        "retain"
    );
}

#[test]
fn restart_during_cleanup_preserves_manifest_for_retry() {
    let root = Fixture::new();
    let item = create(&root.0, "retry", true, None, &machine()).unwrap();
    profile_data(&root.0, &item.id, "data");
    let mut calls = 0;
    assert!(remove(&root.0, &item.id, || {
        calls += 1;
        if calls >= 3 {
            Err("restarted".into())
        } else {
            Ok(())
        }
    })
    .is_err());
    assert!(load(&root.0, &item.id).is_ok());
    remove(&root.0, &item.id, || Ok(())).unwrap();
    assert!(list(&root.0).unwrap().is_empty());
}

#[test]
fn a_corrupt_or_interrupted_environment_does_not_block_other_cleanup() {
    let root = Fixture::new();
    let broken = create(&root.0, "broken", true, None, &machine()).unwrap();
    let broken_profile = profile_data(&root.0, &broken.id, "retain");
    fs::write(
        directory(&root.0, &broken.id).unwrap().join(MANIFEST),
        "{broken",
    )
    .unwrap();
    let interrupted = "f".repeat(32);
    private::create_directory(&root.0.join(&interrupted)).unwrap();
    let mut valid = create(&root.0, "valid", true, None, &machine()).unwrap();
    valid.cleanup_pending = true;
    save(&root.0, &valid).unwrap();
    profile_data(&root.0, &valid.id, "delete");
    let (items, errors) = inventory(&root.0).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(errors.len(), 2);
    for item in items {
        if cleanup_candidate(&item, 100) {
            remove(&root.0, &item.id, || Ok(())).unwrap();
        }
    }
    assert!(!directory(&root.0, &valid.id).unwrap().exists());
    assert!(broken_profile.join("Default/Cookies").exists());
    assert!(root.0.join(interrupted).exists());
}

#[test]
fn saved_multi15_manifest_remains_readable_and_future_formats_fail_closed() {
    let root = Fixture::new();
    let item = create(&root.0, "已有环境", false, None, &machine()).unwrap();
    let data = profile_data(&root.0, &item.id, "synthetic login");
    let path = directory(&root.0, &item.id).unwrap().join(MANIFEST);
    let mut old: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("browser");
    profile::write_json(&path, &old).unwrap();
    assert!(load(&root.0, &item.id).unwrap().browser == BrowserKind::Fingerprint);
    old["browser"] = json!("brave");
    profile::write_json(&path, &old).unwrap();
    assert!(load(&root.0, &item.id).unwrap().browser == BrowserKind::Brave);
    let retained = fs::read(&path).unwrap();
    old["format"] = json!(999);
    profile::write_json(&path, &old).unwrap();
    assert!(load(&root.0, &item.id).is_err());
    assert_eq!(fs::read_to_string(data.join("Default/Cookies")).unwrap(), "synthetic login");
    fs::write(&path, retained).unwrap();
    assert!(load(&root.0, &item.id).is_ok());
    assert_eq!(fs::read_to_string(data.join("Default/Cookies")).unwrap(), "synthetic login");
}

#[test]
fn integrated_catalog_preserves_legacy_profiles_and_official_native_root() {
    let f = Fixture::new();
    let official = f.0.join("Library/Application Support/ai.local.requirement-french/browser/environments");
    assert_eq!(retained_environments_root(&f.0, &official, true).unwrap(), official);
    let legacy = f.0.join("Library/Application Support/ai.local.mingle.ccbaga.environments/browser/environments");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("synthetic-data"), "unchanged").unwrap();
    assert_eq!(retained_environments_root(&f.0, &official, true).unwrap(), legacy);
    assert_eq!(retained_environments_root(&f.0, &official, false).unwrap(), official);
    assert_eq!(fs::read_to_string(legacy.join("synthetic-data")).unwrap(), "unchanged");
    assert!(!official.exists());
}
#[cfg(unix)]
#[test]
fn integrated_catalog_refuses_redirects() {
    let f = Fixture::new();
    let legacy = f.0.join("Library/Application Support/ai.local.mingle.ccbaga.environments/browser/environments");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    let other = f.0.join("other");fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, &legacy).unwrap();
    assert!(retained_environments_root(&f.0, &f.0.join("official"), true).is_err());
}
